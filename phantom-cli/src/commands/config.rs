use crate::cli::ConfigAction;
use phantom_cli::config;
use phantom_cli::json_out::{ConfigPayload, Envelope};

pub fn run(action: ConfigAction, json: bool) {
    match action {
        ConfigAction::Show => {
            let r = config::resolved();
            let path = config::config_file_path();

            if json {
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
                pipe_name: Some(config::DEFAULT_PIPE_NAME.to_string()),
                log_level: Some(config::DEFAULT_LOG_LEVEL.to_string()),
                telemetry_enabled: Some(false),
                ..config::PhantomConfig::empty()
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
                // The phone-home callback the operator opts into. The
                // design is operator-configured (no forced vendor URL);
                // this is the supported way to set it. Empty / "none" /
                // "unset" clears it, which turns phone-home off entirely.
                "phone_home_url" => {
                    cfg.phone_home_url = if value.is_empty()
                        || value.eq_ignore_ascii_case("none")
                        || value.eq_ignore_ascii_case("unset")
                    {
                        None
                    } else {
                        Some(value)
                    };
                }
                // Opt out without forgetting the URL. The error path in
                // `license status` points operators here.
                "phone_home_enabled" => {
                    cfg.phone_home_enabled = Some(matches!(
                        value.to_ascii_lowercase().as_str(),
                        "1" | "true" | "yes" | "on" | "enable" | "enabled"
                    ));
                }
                // Seconds between calls (default 24h). 0 = call on every
                // invocation; useful for testing and for operators who
                // want tighter revocation latency.
                "phone_home_interval_secs" => match value.parse::<u64>() {
                    Ok(n) => cfg.phone_home_interval_secs = Some(n),
                    Err(_) => {
                        eprintln!("  phone_home_interval_secs must be a non-negative integer");
                        std::process::exit(1);
                    }
                },
                other => {
                    eprintln!("  Unknown config key: '{}'", other);
                    eprintln!("  Valid keys: data_dir, pipe_name, log_level, license_key, telemetry_enabled, phone_home_url, phone_home_enabled, phone_home_interval_secs");
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
    }
}
