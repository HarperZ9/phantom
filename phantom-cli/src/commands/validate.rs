use phantom_cli::{profile, validator};

pub fn run(name: String) {
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
