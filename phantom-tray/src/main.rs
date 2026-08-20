mod autostart;
mod icons;
mod popup;
mod toast;
pub mod tray;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 {
        match args[1].as_str() {
            "--install-autostart" => {
                match autostart::install() {
                    Ok(()) => println!("  Auto-start enabled. Phantom Tray will launch at login."),
                    Err(e) => eprintln!("  Failed to enable auto-start: {}", e),
                }
                return;
            }
            "--remove-autostart" => {
                match autostart::remove() {
                    Ok(()) => println!("  Auto-start disabled."),
                    Err(e) => eprintln!("  Failed to disable auto-start: {}", e),
                }
                return;
            }
            "--help" | "-h" => {
                println!("Phantom Tray — system tray identity shield");
                println!();
                println!("Usage:");
                println!("  phantom-tray              Launch tray application");
                println!("  phantom-tray --install-autostart");
                println!("                            Start at login");
                println!("  phantom-tray --remove-autostart");
                println!("                            Remove login auto-start");
                println!("  phantom-tray --help       Show this help");
                return;
            }
            other => {
                eprintln!("  Unknown option: {}", other);
                eprintln!("  Run phantom-tray --help for usage.");
                std::process::exit(1);
            }
        }
    }

    println!("  Phantom Tray");
    println!("  Connecting to Phantom service...");

    tray::run();
}
