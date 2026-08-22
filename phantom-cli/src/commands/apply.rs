use phantom_cli::{apply, profile};

pub fn run(name: String, layers: String) {
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
    let mut access_denied = false;

    let looks_like_access_denied =
        |msg: &str| msg.contains("os error 5") || msg.contains("Access is denied");

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
                        if looks_like_access_denied(err) {
                            access_denied = true;
                        }
                        println!("    ! {} : {}", item, err);
                    }
                }
            }
            Err(e) => {
                any_failure = true;
                if looks_like_access_denied(e) {
                    access_denied = true;
                }
                println!("  {} - {}", layer.name(), e);
            }
        }
    }

    println!();
    if any_failure {
        if access_denied {
            // Writing HKLM identity keys needs an elevated token.
            // Surface that plainly instead of leaving the operator
            // to decode a wall of "Access is denied (os error 5)".
            eprintln!("  Access denied. `phantom apply` writes machine-wide registry keys and");
            eprintln!("  must run elevated. Re-run from an Administrator terminal (right-click");
            eprintln!("  > Run as administrator).");
        } else {
            eprintln!(
                "  Some operations failed. Run 'phantom validate {}' to check consistency.",
                name
            );
        }
        std::process::exit(1);
    } else {
        // Record the applied profile so the boot-time reapply can restore
        // it. On Linux a spoofed MAC does not survive a reboot, so the
        // systemd unit reads this and reapplies. Gated to Linux: on Windows
        // the registry values persist on their own and the service manages
        // this record itself, so the CLI must not change that behavior.
        #[cfg(target_os = "linux")]
        {
            if let Err(e) = apply::ActiveConfig::record_applied(&name, &layers) {
                eprintln!("  Warning: could not record active profile for reapply-on-boot: {e}");
            }
        }

        println!(
            "  Done. Run 'phantom validate {}' to verify consistency.",
            name
        );
    }
}
