use phantom_cli::{build_info, config, profile};

use clap::Parser;

mod cli;
mod commands;

use cli::{Cli, Commands};

fn main() {
    // Redact license keys, HMAC hexes, and fingerprints from any
    // panic message before it reaches stderr or a crash reporter.
    phantom_license::redact::install_panic_hook();

    // Harden this process before we do anything else. On Linux this
    // disables core dumps and blocks foreign-UID ptrace via
    // PR_SET_DUMPABLE=0 — closing the "core-dump the process and
    // grep for the master key" attack. No-op on Windows for now.
    phantom_license::integrity::harden_process();

    // Opportunistic license phone-home. Fires only when the operator
    // has acknowledged the Privacy Notice AND phone_home_enabled is
    // not explicitly false AND phone_home_url is set AND the interval
    // has elapsed since the last call. Runs concurrently with the
    // command so the user's output is never blocked on the network; we
    // join it just before exit so the call actually lands.
    let phone_home_handle = maybe_spawn_phone_home();

    let cli = Cli::parse();

    match cli.command {
        Commands::Audit => commands::audit::run(),

        Commands::Profile { action } => commands::profile::run(action, cli.json),

        Commands::Apply { name, layers } => commands::apply::run(name, layers),

        Commands::Validate { name } => commands::validate::run(name),

        Commands::Revert => commands::revert::run(),

        Commands::Status => commands::status::run(cli.json),

        Commands::License { action } => commands::license::run(action, cli.json),

        Commands::Service { action } => commands::service::run(action),

        Commands::Config { action } => commands::config::run(action, cli.json),

        Commands::Version => commands::meta::version(cli.json),

        Commands::SelfCheck => commands::meta::self_check(cli.json),

        Commands::TamperReport { clear } => commands::meta::tamper_report(clear, cli.json),

        Commands::PrivacyNotice => commands::meta::privacy_notice(),

        Commands::Tou => commands::meta::tou(),
    }

    // Let the phone-home call finish before the process exits, so a
    // revocation actually reaches the endpoint. The command's output has
    // already printed; this only delays the final return, bounded by
    // curl's --max-time (and skipped entirely when no call was started).
    if let Some(handle) = phone_home_handle {
        let _ = handle.join();
    }
}

/// Start the license phone-home in a background thread if configuration
/// says it's active, returning its handle so `main` can let it finish
/// before the process exits.
///
/// The command's own output is not blocked — the call runs concurrently
/// while the command executes and prints. Only at the very end does
/// `main` join this handle, so the process lingers just long enough for
/// the call to land (bounded by curl's `--max-time`). This matters for
/// revocation: a fire-and-forget thread is killed when a fast command
/// like `license status` exits, so the callback never reaches the
/// endpoint. On a `{"revoked": true}` response, the tripwire records a
/// High-severity event and the next `LicenseGuard::load()` silently
/// downgrades the install.
fn maybe_spawn_phone_home() -> Option<std::thread::JoinHandle<()>> {
    let cfg = config::load_from_file();
    if !cfg.phone_home_active() {
        return None;
    }
    let url = cfg.phone_home_url.clone()?;
    let interval = cfg
        .phone_home_interval_secs
        .unwrap_or(phantom_license::phone_home::DEFAULT_INTERVAL_SECS);
    let data_dir = profile::data_dir();

    if !phantom_license::phone_home::is_due(&data_dir, &url, interval) {
        return None;
    }

    let guard = phantom_license::LicenseGuard::load();
    // Source the key from the activated license (LicenseGuard reads it
    // from .license.json), NOT from cfg.license_key, which `activate`
    // never populates. The proof-of-possession is an HMAC over this key;
    // sourcing it wrong sends an unlicensed serial + empty proof, which
    // the endpoint reads as a revoked/forged install.
    let key_str = guard.key_str().map(|s| s.to_string());
    let tier = guard.tier().to_string();
    let version = build_info::VERSION.to_string();
    let tripwire = phantom_license::tripwire::read_events(&data_dir);
    let (low, high) = tripwire
        .iter()
        .fold((0u32, 0u32), |(l, h), e| match e.severity {
            phantom_license::tripwire::Severity::Low => (l + 1, h),
            phantom_license::tripwire::Severity::High => (l, h + 1),
        });

    Some(std::thread::spawn(move || {
        let payload = phantom_license::phone_home::build_payload(
            key_str.as_deref(),
            &tier,
            &version,
            low,
            high,
        );
        let _ =
            phantom_license::phone_home::maybe_phone_home(&data_dir, Some(&url), interval, payload);
    }))
}
