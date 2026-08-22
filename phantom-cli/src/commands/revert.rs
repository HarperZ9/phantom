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

    println!();
}
