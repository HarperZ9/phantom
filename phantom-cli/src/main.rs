use phantom_cli::json_out::{
    ConfigPayload, Envelope, LayerStatus, LicenseStatusPayload, MaxProfiles, ProfileListEntry,
    StatusPayload, VersionPayload,
};
use phantom_cli::{apply, audit, build_info, config, profile, validator};

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "phantom",
    about = "Hardware identity privacy tool",
    long_about = "Phantom generates realistic, internally-consistent hardware identity profiles \
                  and applies them across multiple system layers, giving users control over \
                  what their machine reports to software.",
    version
)]
struct Cli {
    /// Emit machine-readable JSON to stdout instead of the pretty text UI.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Read and report all hardware identifiers currently visible on this machine
    Audit,

    /// Manage hardware identity profiles
    Profile {
        #[command(subcommand)]
        action: ProfileAction,
    },

    /// Apply a profile to the system
    Apply {
        /// Profile name to apply
        name: String,

        /// Comma-separated layers to apply (0=firmware, 1=kernel, 2=userland)
        #[arg(long, default_value = "2")]
        layers: String,
    },

    /// Validate that active spoofing is consistent across all identifier sources
    Validate {
        /// Profile name to validate against
        name: String,
    },

    /// Restore original hardware identifiers
    Revert,

    /// Show current spoofing status for each layer
    Status,

    /// Communicate with the Phantom background service
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },

    /// Manage license activation
    License {
        #[command(subcommand)]
        action: LicenseAction,
    },

    /// Show or edit Phantom configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Print detailed version and build information
    Version,
}

#[derive(Subcommand)]
enum ProfileAction {
    /// Generate a new hardware identity profile
    Generate {
        /// Profile name
        name: String,

        /// Seed string for deterministic generation (same seed = same profile)
        #[arg(long)]
        seed: Option<String>,

        /// Print profile to stdout without saving
        #[arg(long)]
        dry_run: bool,
    },

    /// Display a saved profile
    Show {
        /// Profile name
        name: String,
    },

    /// List all saved profiles
    List,

    /// Export a profile as JSON to stdout
    Export {
        /// Profile name
        name: String,
    },

    /// Import a profile from a JSON file
    Import {
        /// Path to JSON file
        path: String,
    },

    /// Delete a saved profile
    Delete {
        /// Profile name
        name: String,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Audit => {
            audit::run_audit();
        }

        Commands::Profile { action } => match action {
            ProfileAction::Generate {
                name,
                seed,
                dry_run,
            } => {
                if !dry_run {
                    let guard = phantom_license::LicenseGuard::load();
                    let current = profile::list_profiles().unwrap_or_default().len();
                    if let Err(_) = guard.check_profile_limit(current) {
                        eprintln!(
                            "  Profile limit reached for {} tier (max {}).",
                            guard.tier(),
                            guard.tier().max_profiles()
                        );
                        eprintln!("  Upgrade your license or delete existing profiles.");
                        std::process::exit(1);
                    }
                }

                let seed_str = seed.unwrap_or_else(|| generate_random_seed());
                println!(
                    "  Generating profile '{}' with seed '{}'...",
                    name, seed_str
                );

                let prof = profile::engine::generate_profile(&seed_str, &name);

                if dry_run {
                    validator::report::print_profile_summary(&prof);
                } else {
                    match profile::save_profile(&prof) {
                        Ok(path) => {
                            println!("  Saved to: {}\n", path.display());
                            validator::report::print_profile_summary(&prof);
                        }
                        Err(e) => {
                            eprintln!("  Error saving profile: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
            }

            ProfileAction::Show { name } => match profile::load_profile(&name) {
                Ok(prof) => validator::report::print_profile_summary(&prof),
                Err(e) => {
                    eprintln!("  Error loading profile '{}': {}", name, e);
                    std::process::exit(1);
                }
            },

            ProfileAction::List => match profile::list_profiles() {
                Ok(names) => {
                    if cli.json {
                        let entries: Vec<ProfileListEntry> = names
                            .iter()
                            .filter_map(|n| {
                                profile::load_profile(n).ok().map(|p| ProfileListEntry {
                                    name: n.clone(),
                                    seed: p.metadata.seed.clone(),
                                    identifier_count: p.identifier_count(),
                                })
                            })
                            .collect();
                        Envelope::ok("profile list", entries).print();
                    } else if names.is_empty() {
                        println!("  No saved profiles.");
                        println!("  Use 'phantom profile generate <name>' to create one.");
                    } else {
                        println!("\n  Saved profiles:\n");
                        for name in &names {
                            match profile::load_profile(name) {
                                Ok(p) => println!(
                                    "    {:<20} seed={:<20} vectors={}",
                                    name,
                                    p.metadata.seed,
                                    p.identifier_count(),
                                ),
                                Err(_) => println!("    {:<20} (error reading)", name),
                            }
                        }
                        println!();
                    }
                }
                Err(e) => {
                    if cli.json {
                        Envelope::<()>::error("profile list", e.to_string()).print();
                    } else {
                        eprintln!("  Error listing profiles: {}", e);
                    }
                    std::process::exit(1);
                }
            },

            ProfileAction::Export { name } => match profile::load_profile(&name) {
                Ok(prof) => {
                    let json = serde_json::to_string_pretty(&prof).unwrap();
                    println!("{}", json);
                }
                Err(e) => {
                    eprintln!("  Error loading profile '{}': {}", name, e);
                    std::process::exit(1);
                }
            },

            ProfileAction::Import { path } => match std::fs::read_to_string(&path) {
                Ok(json) => match serde_json::from_str::<profile::schema::HardwareProfile>(&json) {
                    Ok(prof) => match profile::save_profile(&prof) {
                        Ok(saved_path) => {
                            println!(
                                "  Imported profile '{}' to {}",
                                prof.metadata.name,
                                saved_path.display()
                            );
                        }
                        Err(e) => {
                            eprintln!("  Error saving imported profile: {}", e);
                            std::process::exit(1);
                        }
                    },
                    Err(e) => {
                        eprintln!("  Error parsing profile JSON: {}", e);
                        std::process::exit(1);
                    }
                },
                Err(e) => {
                    eprintln!("  Error reading file '{}': {}", path, e);
                    std::process::exit(1);
                }
            },

            ProfileAction::Delete { name } => {
                let dir = profile::profiles_dir();
                let filename = format!("{}.json", name);
                let path = dir.join(&filename);
                if path.exists() {
                    match std::fs::remove_file(&path) {
                        Ok(_) => println!("  Deleted profile '{}'.", name),
                        Err(e) => {
                            eprintln!("  Error deleting profile: {}", e);
                            std::process::exit(1);
                        }
                    }
                } else {
                    eprintln!("  Profile '{}' not found.", name);
                    std::process::exit(1);
                }
            }
        },

        Commands::Apply { name, layers } => {
            let guard = phantom_license::LicenseGuard::load();
            let layers = match apply::parse_layers(&layers) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("  Error: {}", e);
                    std::process::exit(1);
                }
            };

            for layer in &layers {
                let layer_num = match layer {
                    apply::Layer::Firmware => 0,
                    apply::Layer::Kernel => 1,
                    apply::Layer::Userland => 2,
                };
                if let Err(e) = guard.check_layer(layer_num) {
                    eprintln!("  License error for {}: {}", layer.name(), e);
                    eprintln!("  Upgrade to Pro or Enterprise for Layer 0/1 access.");
                    eprintln!("  Run 'phantom license status' for details.");
                    std::process::exit(1);
                }
            }

            let prof = match profile::load_profile(&name) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("  Error loading profile '{}': {}", name, e);
                    std::process::exit(1);
                }
            };

            println!(
                "\n  Applying profile '{}' to {} layer(s)...\n",
                name,
                layers.len()
            );

            let results = apply::apply_profile(&prof, &layers);
            let mut any_failure = false;

            for (layer, result) in &results {
                match result {
                    Ok(r) => {
                        if r.success() {
                            println!(
                                "  {} - {} identifiers applied",
                                layer.name(),
                                r.applied.len()
                            );
                            for item in &r.applied {
                                println!("    + {}", item);
                            }
                        } else {
                            any_failure = true;
                            println!("  {} - FAILED", layer.name());
                            for (item, err) in &r.failed {
                                println!("    ! {} : {}", item, err);
                            }
                        }
                    }
                    Err(e) => {
                        println!("  {} - {}", layer.name(), e);
                    }
                }
            }

            println!();
            if any_failure {
                eprintln!(
                    "  Some operations failed. Run 'phantom validate {}' to check consistency.",
                    name
                );
                std::process::exit(1);
            } else {
                println!(
                    "  Done. Run 'phantom validate {}' to verify consistency.",
                    name
                );
            }
        }

        Commands::Validate { name } => {
            let prof = match profile::load_profile(&name) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("  Error loading profile '{}': {}", name, e);
                    std::process::exit(1);
                }
            };

            println!("\n  Validating profile '{}' against live system...\n", name);
            let sources = validator::sources::read_all_sources();
            let result = validator::diff::validate_profile_against_sources(&prof, &sources);
            validator::report::print_validation_report(&result);

            if !result.is_consistent() {
                std::process::exit(1);
            }
        }

        Commands::Revert => {
            println!("\n  Reverting to original hardware identifiers...\n");

            let results = apply::revert_all();

            for (layer, result) in &results {
                match result {
                    Ok(r) => {
                        if r.success() {
                            println!(
                                "  {} - {} identifiers restored",
                                layer.name(),
                                r.applied.len()
                            );
                        } else {
                            println!("  {} - ERRORS", layer.name());
                            for (item, err) in &r.failed {
                                println!("    ! {} : {}", item, err);
                            }
                        }
                    }
                    Err(e) => println!("  {} - {}", layer.name(), e),
                }
            }

            println!();
        }

        Commands::Status => {
            let statuses = apply::status();
            let cfg = config::resolved();

            if cli.json {
                let layers = statuses
                    .iter()
                    .map(|(layer, status)| LayerStatus {
                        layer: match layer {
                            apply::Layer::Firmware => 0,
                            apply::Layer::Kernel => 1,
                            apply::Layer::Userland => 2,
                        },
                        name: layer.name(),
                        status: status.to_string(),
                    })
                    .collect();
                Envelope::ok(
                    "status",
                    StatusPayload {
                        layers,
                        data_dir: cfg.data_dir.display().to_string(),
                        pipe_name: cfg.pipe_name.clone(),
                    },
                )
                .print();
            } else {
                println!("\n  Phantom Status\n  {}\n", "=".repeat(50));
                for (layer, status) in &statuses {
                    println!("  {} : {}", layer.name(), status);
                }
                println!();
            }
        }

        Commands::License { action } => match action {
            LicenseAction::Status => {
                let guard = phantom_license::LicenseGuard::load();

                if cli.json {
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

            LicenseAction::Activate { key } => {
                match phantom_license::LicenseGuard::activate(&key) {
                    Ok(guard) => {
                        println!("  License activated: {} tier", guard.tier());
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
        },

        Commands::Service { action } => match action {
            ServiceAction::Ping => match phantom_ipc::PhantomClient::connect() {
                Ok(mut client) => match client.ping() {
                    Ok(version) => println!("  Service is running (protocol v{})", version),
                    Err(e) => {
                        eprintln!("  Service error: {}", e);
                        std::process::exit(1);
                    }
                },
                Err(e) => {
                    eprintln!("  Cannot connect to Phantom service: {}", e);
                    std::process::exit(1);
                }
            },

            ServiceAction::Status => match phantom_ipc::PhantomClient::connect() {
                Ok(mut client) => match client.status() {
                    Ok(s) => {
                        println!("\n  Phantom Service Status\n  {}\n", "=".repeat(50));
                        println!("  Protected:  {}", if s.protected { "YES" } else { "NO" });
                        if let Some(ref name) = s.active_profile {
                            println!("  Profile:    {}", name);
                        }
                        if !s.active_layers.is_empty() {
                            let layer_names: Vec<&str> = s
                                .active_layers
                                .iter()
                                .map(|l| match l {
                                    0 => "Firmware",
                                    1 => "Kernel",
                                    2 => "Userland",
                                    _ => "Unknown",
                                })
                                .collect();
                            println!("  Layers:     {}", layer_names.join(", "));
                        }
                        println!("  Uptime:     {}s", s.uptime_secs);
                        println!(
                            "  Driver:     {}",
                            if s.driver_connected {
                                "connected"
                            } else {
                                "not connected"
                            }
                        );
                        println!(
                            "  Firmware:   {}",
                            if s.firmware_detected {
                                "detected"
                            } else {
                                "not detected"
                            }
                        );
                        println!();
                    }
                    Err(e) => {
                        eprintln!("  Service error: {}", e);
                        std::process::exit(1);
                    }
                },
                Err(e) => {
                    eprintln!("  Cannot connect to Phantom service: {}", e);
                    eprintln!("  Is the service running? Try: phantom-svc --standalone");
                    std::process::exit(1);
                }
            },

            ServiceAction::Protect { name, layers } => {
                let layer_bytes: Vec<u8> = layers
                    .split(',')
                    .filter_map(|s| match s.trim() {
                        "0" | "firmware" | "dxe" => Some(0),
                        "1" | "kernel" | "driver" => Some(1),
                        "2" | "userland" | "registry" => Some(2),
                        _ => None,
                    })
                    .collect();

                if layer_bytes.is_empty() {
                    eprintln!("  No valid layers specified.");
                    std::process::exit(1);
                }

                match phantom_ipc::PhantomClient::connect() {
                    Ok(mut client) => match client.protect(&name, &layer_bytes) {
                        Ok(phantom_ipc::Response::Applied {
                            layers_applied,
                            identifiers,
                        }) => {
                            println!(
                                "  Protected: {} identifiers across {} layer(s)",
                                identifiers,
                                layers_applied.len()
                            );
                        }
                        Ok(phantom_ipc::Response::Error { message, .. }) => {
                            eprintln!("  Service error: {}", message);
                            std::process::exit(1);
                        }
                        Ok(_) => println!("  Profile applied."),
                        Err(e) => {
                            eprintln!("  Error: {}", e);
                            std::process::exit(1);
                        }
                    },
                    Err(e) => {
                        eprintln!("  Cannot connect to Phantom service: {}", e);
                        std::process::exit(1);
                    }
                }
            }

            ServiceAction::Unprotect => match phantom_ipc::PhantomClient::connect() {
                Ok(mut client) => match client.unprotect() {
                    Ok(phantom_ipc::Response::Reverted { warnings }) => {
                        println!("  Unprotected: original identifiers restored.");
                        for w in &warnings {
                            println!("  Warning: {}", w);
                        }
                    }
                    Ok(phantom_ipc::Response::Error { message, .. }) => {
                        eprintln!("  Service error: {}", message);
                        std::process::exit(1);
                    }
                    Ok(_) => println!("  Reverted."),
                    Err(e) => {
                        eprintln!("  Error: {}", e);
                        std::process::exit(1);
                    }
                },
                Err(e) => {
                    eprintln!("  Cannot connect to Phantom service: {}", e);
                    std::process::exit(1);
                }
            },
        },

        Commands::Config { action } => match action {
            ConfigAction::Show => {
                let r = config::resolved();
                let path = config::config_file_path();

                if cli.json {
                    Envelope::ok(
                        "config show",
                        ConfigPayload {
                            data_dir: r.data_dir.display().to_string(),
                            pipe_name: r.pipe_name.clone(),
                            log_level: r.log_level.clone(),
                            telemetry_enabled: r.telemetry_enabled,
                            config_file: path.display().to_string(),
                            config_file_present: r.source_file_present,
                        },
                    )
                    .print();
                } else {
                    println!("\n  Phantom Config\n  {}\n", "=".repeat(50));
                    println!("  Data dir:    {}", r.data_dir.display());
                    println!("  Pipe name:   {}", r.pipe_name);
                    println!("  Log level:   {}", r.log_level);
                    println!(
                        "  Telemetry:   {}",
                        if r.telemetry_enabled { "on" } else { "off" }
                    );
                    println!("  Config file: {}", path.display());
                    println!(
                        "  File loaded: {}",
                        if r.source_file_present {
                            "yes"
                        } else {
                            "no (using defaults + env)"
                        }
                    );
                    println!();
                }
            }

            ConfigAction::Path => {
                println!("{}", config::config_file_path().display());
            }

            ConfigAction::Init => {
                let path = config::config_file_path();
                if path.exists() {
                    eprintln!("  Config file already exists at: {}", path.display());
                    eprintln!("  Delete it first, or edit it by hand.");
                    std::process::exit(1);
                }
                let cfg = config::PhantomConfig {
                    data_dir: None,
                    pipe_name: Some(config::DEFAULT_PIPE_NAME.to_string()),
                    log_level: Some(config::DEFAULT_LOG_LEVEL.to_string()),
                    license_key: None,
                    telemetry_enabled: Some(false),
                };
                match config::save_to_file(&cfg) {
                    Ok(p) => println!("  Wrote default config to: {}", p.display()),
                    Err(e) => {
                        eprintln!("  Error writing config: {}", e);
                        std::process::exit(1);
                    }
                }
            }

            ConfigAction::Set { key, value } => {
                let mut cfg = config::load_from_file();
                match key.as_str() {
                    "data_dir" => cfg.data_dir = Some(value),
                    "pipe_name" => cfg.pipe_name = Some(value),
                    "log_level" => cfg.log_level = Some(value),
                    "license_key" => cfg.license_key = Some(value),
                    "telemetry_enabled" => {
                        cfg.telemetry_enabled = Some(matches!(
                            value.to_ascii_lowercase().as_str(),
                            "1" | "true" | "yes" | "on" | "enable" | "enabled"
                        ));
                    }
                    other => {
                        eprintln!("  Unknown config key: '{}'", other);
                        eprintln!("  Valid keys: data_dir, pipe_name, log_level, license_key, telemetry_enabled");
                        std::process::exit(1);
                    }
                }
                match config::save_to_file(&cfg) {
                    Ok(p) => println!("  Updated: {}", p.display()),
                    Err(e) => {
                        eprintln!("  Error saving config: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        },

        Commands::Version => {
            if cli.json {
                Envelope::ok(
                    "version",
                    VersionPayload {
                        name: build_info::PKG_NAME,
                        version: build_info::VERSION,
                        git_commit: build_info::GIT_COMMIT,
                        target: build_info::BUILD_TARGET,
                        profile: build_info::BUILD_PROFILE,
                    },
                )
                .print();
            } else {
                println!("{}", build_info::full_version_string());
            }
        }
    }
}

#[derive(Subcommand)]
enum ServiceAction {
    /// Check if the Phantom service is running
    Status,

    /// Ask the service to apply a profile (protected mode)
    Protect {
        /// Profile name
        name: String,

        /// Comma-separated layers (0=firmware, 1=kernel, 2=userland)
        #[arg(long, default_value = "2")]
        layers: String,
    },

    /// Ask the service to revert all spoofing (unprotected mode)
    Unprotect,

    /// Ping the service
    Ping,
}

#[derive(Subcommand)]
enum LicenseAction {
    /// Show current license status
    Status,

    /// Activate a license key
    Activate {
        /// License key string
        key: String,
    },

    /// Deactivate the current license
    Deactivate,

    /// Show this machine's hardware fingerprint (for license binding)
    Fingerprint,
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Show the resolved runtime configuration (env > file > defaults)
    Show,

    /// Print the config file path this session would read/write
    Path,

    /// Write a default config file to disk (fails if one already exists)
    Init,

    /// Set a single config key ('data_dir', 'pipe_name', 'log_level',
    /// 'license_key', 'telemetry_enabled') and save
    Set {
        /// Config key name
        key: String,
        /// Value to store
        value: String,
    },
}

fn generate_random_seed() -> String {
    use rand::Rng;
    let mut rng = rand::rngs::OsRng;
    let bytes: [u8; 16] = rng.gen();
    format!(
        "phantom-{}",
        bytes
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>()
    )
}
