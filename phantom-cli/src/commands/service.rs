use crate::cli::ServiceAction;

pub fn run(action: ServiceAction) {
    match action {
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
    }
}
