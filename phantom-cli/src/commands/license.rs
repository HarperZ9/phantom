use crate::cli::LicenseAction;
use crate::commands::util::print_document;
use phantom_cli::json_out::{
    Envelope, LicenseRequestPayload, LicenseStatusPayload, MaxProfiles, SelfCheckBuild,
};
use phantom_cli::{build_info, config};

pub fn run(action: LicenseAction, json: bool) {
    match action {
        LicenseAction::Status => {
            let guard = phantom_license::LicenseGuard::load();

            if json {
                let layers_allowed: Vec<u8> = (0u8..=2)
                    .filter(|l| guard.tier().allows_layer(*l))
                    .collect();
                let max_profiles = match guard.tier() {
                    phantom_license::LicenseTier::Free => MaxProfiles::Limited(2),
                    phantom_license::LicenseTier::Pro => MaxProfiles::Limited(50),
                    phantom_license::LicenseTier::Enterprise => MaxProfiles::Unlimited,
                };
                let fp = phantom_license::MachineFingerprint::collect();
                Envelope::ok(
                    "license status",
                    LicenseStatusPayload {
                        tier: guard.tier().to_string(),
                        licensed: guard.is_licensed(),
                        days_remaining: guard.days_remaining(),
                        layers_allowed,
                        max_profiles,
                        machine_fingerprint: fp.hex(),
                    },
                )
                .print();
            } else {
                println!("\n  Phantom License\n  {}\n", "=".repeat(50));
                println!("  Tier:       {}", guard.tier());
                println!(
                    "  Licensed:   {}",
                    if guard.is_licensed() {
                        "YES"
                    } else {
                        "NO (Free tier)"
                    }
                );
                if let Some(days) = guard.days_remaining() {
                    println!("  Expires in: {} days", days);
                } else if guard.is_licensed() {
                    println!("  Expires:    never (perpetual)");
                }
                println!(
                    "  Layers:     {}",
                    match guard.tier() {
                        phantom_license::LicenseTier::Free => "Layer 2 (Registry) only",
                        _ => "All layers (Firmware, Kernel, Registry)",
                    }
                );
                println!(
                    "  Profiles:   up to {}",
                    match guard.tier() {
                        phantom_license::LicenseTier::Free => "2".to_string(),
                        phantom_license::LicenseTier::Pro => "50".to_string(),
                        phantom_license::LicenseTier::Enterprise => "unlimited".to_string(),
                    }
                );
                println!();
            }
        }

        LicenseAction::Activate {
            key,
            accept_tou,
            acknowledge_privacy_notice,
        } => {
            // Enforce ToU + Privacy Notice acknowledgment BEFORE
            // touching key material. Both documents ship at pinned
            // versions in phantom_license::legal; version bumps
            // force re-acknowledgment. Non-interactive callers
            // must pass --accept-tou and --acknowledge-privacy-
            // notice; interactive TTYs are prompted.
            if let Err(msg) = ensure_disclosures(accept_tou, acknowledge_privacy_notice) {
                eprintln!("  {}", msg);
                std::process::exit(1);
            }

            match phantom_license::LicenseGuard::activate(&key) {
                Ok(guard) => {
                    println!("  License activated: {} tier", guard.tier());
                    if let Some(cfg) = config_after_activate() {
                        if cfg.phone_home_active() {
                            println!(
                                "  Phone-home is ENABLED (endpoint: {}). \
                                 Disable with: phantom config set phone_home_enabled false",
                                cfg.phone_home_url.as_deref().unwrap_or("<unset>")
                            );
                        }
                    }
                }
                Err(phantom_license::LicenseError::RateLimited(secs)) => {
                    eprintln!(
                        "  Rate-limited: too many failed attempts. Wait {} seconds and try again.",
                        secs
                    );
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("  Activation failed: {}", e);
                    std::process::exit(1);
                }
            }
        }

        LicenseAction::Deactivate => {
            let mut guard = phantom_license::LicenseGuard::load();
            guard.deactivate();
            println!("  License deactivated. Reverted to Free tier.");
        }

        LicenseAction::Fingerprint => {
            let fp = phantom_license::MachineFingerprint::collect();
            println!("  Machine fingerprint: {}", fp.hex());
        }

        LicenseAction::Request { tier } => {
            let fp = phantom_license::MachineFingerprint::collect();
            let guard = phantom_license::LicenseGuard::load();
            let requested = match tier.to_ascii_lowercase().as_str() {
                "free" => "Free",
                "pro" => "Pro",
                "enterprise" | "ent" => "Enterprise",
                other => {
                    eprintln!(
                        "  Unknown tier '{}'. Use one of: free, pro, enterprise.",
                        other
                    );
                    std::process::exit(1);
                }
            };
            let platform = if cfg!(target_os = "windows") {
                "windows"
            } else if cfg!(target_os = "linux") {
                "linux"
            } else if cfg!(target_os = "macos") {
                "macos"
            } else {
                "other"
            };

            if json {
                Envelope::ok(
                    "license request",
                    LicenseRequestPayload {
                        machine_fingerprint: fp.hex(),
                        requested_tier: requested.to_string(),
                        current_tier: guard.tier().to_string(),
                        build: SelfCheckBuild {
                            version: build_info::VERSION,
                            git_commit: build_info::GIT_COMMIT,
                            target: build_info::BUILD_TARGET,
                            profile: build_info::BUILD_PROFILE,
                        },
                        master_key_generation: phantom_license::master_key_generation(),
                        platform,
                    },
                )
                .print();
            } else {
                println!("\n  Phantom License Request");
                println!("  {}\n", "=".repeat(50));
                println!("  Machine fingerprint : {}", fp.hex());
                println!("  Requested tier      : {}", requested);
                println!("  Current tier        : {}", guard.tier());
                println!("  Platform            : {}", platform);
                println!(
                    "  Master key gen      : {}",
                    phantom_license::master_key_generation()
                );
                println!(
                    "  Build               : {}",
                    build_info::full_version_string()
                );
                println!("\n  Send the block above to your Phantom licensing contact.");
                println!("  They will issue a key bound to the machine fingerprint");
                println!("  shown here. The key is worthless on any other machine.\n");
            }
        }
    }
}

/// Present the ToU and Privacy Notice to the operator (if not
/// already accepted for the current version) and persist their
/// acknowledgment into the sealed config. Returns Err(msg) when the
/// operator declined or when a non-interactive caller failed to pass
/// the required flags.
///
/// When a compile-time default phone-home URL is baked in and the
/// operator has no URL configured, acknowledging the Privacy Notice
/// also populates `phone_home_url` from that default — so the
/// disclosure IS the moment the user learns which endpoint gets
/// called, not two config-lookups later.
fn ensure_disclosures(accept_tou_flag: bool, ack_privacy_flag: bool) -> Result<(), String> {
    let mut cfg = config::load_from_file();
    let mut changed = false;

    // ToU
    if !cfg.tou_current() {
        let accepted = if accept_tou_flag {
            true
        } else if atty_stdin() {
            print_document("TERMS OF USE", phantom_license::legal::TOU);
            prompt_yes_no("Do you accept these Terms of Use? [y/N] ", false)
        } else {
            return Err(
                "Terms of Use not yet accepted. Re-run with --accept-tou, or run \
                 `phantom tou` in an interactive shell to review and accept."
                    .into(),
            );
        };
        if !accepted {
            return Err("Terms of Use declined. Activation aborted.".into());
        }
        cfg.tou_accepted_at = Some(now_unix_secs());
        cfg.tou_version_accepted = Some(phantom_license::legal::TOU_VERSION);
        changed = true;
    }

    // Privacy notice
    if !cfg.privacy_notice_current() {
        let accepted = if ack_privacy_flag {
            true
        } else if atty_stdin() {
            print_document("PRIVACY NOTICE", phantom_license::legal::PRIVACY_NOTICE);
            prompt_yes_no(
                "Do you acknowledge and enable the phone-home callback? [Y/n] ",
                true,
            )
        } else {
            return Err("Privacy Notice not yet acknowledged. Re-run with \
                 --acknowledge-privacy-notice, or run `phantom privacy-notice` \
                 in an interactive shell to review and acknowledge."
                .into());
        };
        cfg.privacy_notice_acknowledged_at = Some(now_unix_secs());
        cfg.privacy_notice_version_accepted = Some(phantom_license::legal::PRIVACY_NOTICE_VERSION);
        // The default = enabled when acknowledged. Explicit later
        // `config set phone_home_enabled false` opts out.
        cfg.phone_home_enabled = Some(accepted);
        // Populate the phone-home URL from the compile-time default
        // if the operator hasn't set one — this is the moment they
        // consented to it being called.
        if cfg.phone_home_url.is_none() {
            if let Some(url) = config::compiled_default_phone_home_url() {
                cfg.phone_home_url = Some(url.to_string());
            }
        }
        changed = true;
    }

    if changed {
        if let Err(e) = config::save_to_file(&cfg) {
            return Err(format!("Failed to persist acknowledgment: {}", e));
        }
    }
    Ok(())
}

fn atty_stdin() -> bool {
    // Deliberately dep-free: check if stdin is a TTY via isatty(0).
    #[cfg(unix)]
    unsafe {
        extern "C" {
            fn isatty(fd: i32) -> i32;
        }
        isatty(0) != 0
    }
    #[cfg(windows)]
    {
        // No dep-free path on Windows without winapi; treat as TTY
        // so unattended installers must pass --accept-tou etc.
        // explicitly, which is the safer default.
        true
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

fn prompt_yes_no(prompt: &str, default_yes: bool) -> bool {
    use std::io::{BufRead, Write};
    print!("{}", prompt);
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    if std::io::stdin().lock().read_line(&mut line).is_err() {
        return default_yes;
    }
    let s = line.trim().to_ascii_lowercase();
    if s.is_empty() {
        return default_yes;
    }
    matches!(s.as_str(), "y" | "yes")
}

fn now_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn config_after_activate() -> Option<config::PhantomConfig> {
    Some(config::load_from_file())
}
