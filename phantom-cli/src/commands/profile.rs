use crate::cli::ProfileAction;
use phantom_cli::json_out::{Envelope, ProfileListEntry};
use phantom_cli::{profile, validator};

pub fn run(action: ProfileAction, json: bool) {
    match action {
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
                if json {
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
                if json {
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

        ProfileAction::Import { path } => {
            match std::fs::read_to_string(&path) {
                Ok(json) => match serde_json::from_str::<profile::schema::HardwareProfile>(&json) {
                    Ok(prof) => {
                        // Enforce provenance policy before storing anything.
                        let verdict = profile::check_origin(&prof);
                        let guard = phantom_license::LicenseGuard::load();

                        match &verdict {
                            profile::ImportVerdict::ContentTampered => {
                                eprintln!("  Refusing import: profile content was modified after signing.");
                                eprintln!("  The origin_mark does not cover the current bytes.");
                                std::process::exit(1);
                            }
                            profile::ImportVerdict::Invalid => {
                                eprintln!("  Refusing import: origin mark is forged (MAC fails).");
                                std::process::exit(1);
                            }
                            profile::ImportVerdict::Malformed => {
                                eprintln!("  Refusing import: origin mark is malformed.");
                                std::process::exit(1);
                            }
                            profile::ImportVerdict::Foreign { origin_tier } => {
                                if guard.tier() == phantom_license::LicenseTier::Free {
                                    eprintln!(
                                    "  Refusing import: profile was generated on a different machine ({} tier).",
                                    origin_tier
                                );
                                    eprintln!("  Cross-machine profile import requires a Pro or Enterprise license.");
                                    std::process::exit(1);
                                }
                                println!(
                                    "  Note: importing foreign profile (origin tier: {}).",
                                    origin_tier
                                );
                            }
                            profile::ImportVerdict::Unmarked => {
                                println!("  Note: profile is unmarked (legacy or hand-authored).");
                            }
                            profile::ImportVerdict::Local => {}
                        }

                        match profile::save_profile(&prof) {
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
                        }
                    }
                    Err(e) => {
                        eprintln!("  Error parsing profile JSON: {}", e);
                        std::process::exit(1);
                    }
                },
                Err(e) => {
                    eprintln!("  Error reading file '{}': {}", path, e);
                    std::process::exit(1);
                }
            }
        }

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
    }
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
