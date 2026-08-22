use phantom_cli::apply;

pub fn run() {
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

    // Clear the active-profile record so a reboot does not reapply the
    // profile the operator just reverted. Gated to Linux for the same
    // reason the apply write is: on Windows the service owns this record.
    #[cfg(target_os = "linux")]
    {
        if let Err(e) = apply::ActiveConfig::clear() {
            eprintln!("  Warning: could not clear active-profile record: {e}");
        }
    }

    println!();
}
