mod handler;
mod logging;
mod service;
mod state;

use std::env;

fn main() {
    // Redact secrets from any panic message before it reaches the
    // log rotator or stderr.
    phantom_license::redact::install_panic_hook();

    // Harden this process before doing anything else. On Linux this
    // disables core dumps (PR_SET_DUMPABLE=0) so a crash cannot spill
    // the master key onto disk. No-op on Windows for now.
    phantom_license::integrity::harden_process();

    let args: Vec<String> = env::args().collect();

    if args.len() > 1 {
        match args[1].as_str() {
            "--standalone" | "-s" => {
                logging::init(true);
                tracing::info!("Phantom Service starting in standalone mode");
                println!("Phantom Service [standalone mode]");
                service::run_standalone();
            }
            "--install" => {
                service::install_service();
            }
            "--uninstall" => {
                service::uninstall_service();
            }
            "--cleanup" => {
                logging::init(false);
                run_cleanup();
            }
            "--help" | "-h" => {
                print_usage();
            }
            other => {
                eprintln!("Unknown argument: {}", other);
                print_usage();
                std::process::exit(1);
            }
        }
    } else {
        logging::init(false);
        tracing::info!("Phantom Service starting via SCM");
        service::run_as_service();
    }
}

fn run_cleanup() {
    println!("Phantom pre-uninstall cleanup:");

    print!("  Reverting identity layers... ");
    if let Ok(mut client) = phantom_ipc::client::PhantomClient::connect() {
        match client.request(&phantom_ipc::message::Request::Unprotect) {
            Ok(_) => println!("done (via service)."),
            Err(_) => {
                let results = phantom_cli::apply::revert_all();
                let errs: Vec<_> = results.iter().filter(|(_, r)| r.is_err()).collect();
                if errs.is_empty() {
                    println!("done (direct).");
                } else {
                    println!("done ({} errors).", errs.len());
                }
            }
        }
    } else {
        let results = phantom_cli::apply::revert_all();
        let errs: Vec<_> = results.iter().filter(|(_, r)| r.is_err()).collect();
        if errs.is_empty() {
            println!("done (direct).");
        } else {
            println!("done ({} errors).", errs.len());
        }
    }

    print!("  Removing tray auto-start... ");
    remove_tray_autostart();
    println!("done.");

    // Clear the service's auto-apply state so a reinstall does not
    // re-protect on its own. The registry backup is consumed by the
    // revert above (revert_all deletes it on success). Profiles, license,
    // and CLI config are intentionally LEFT in place so a reinstall picks
    // the operator's setup back up (see docs/user/uninstall.md); a full
    // wipe is a documented manual step.
    print!("  Clearing service state... ");
    let config_path = phantom_cli::profile::profiles_dir().join(".config.json");
    let _ = std::fs::remove_file(config_path);
    println!("done.");

    println!("  Cleanup complete.");
}

fn remove_tray_autostart() {
    #[cfg(windows)]
    {
        extern "system" {
            fn RegOpenKeyExA(
                hKey: isize,
                lpSubKey: *const u8,
                ulOptions: u32,
                samDesired: u32,
                phkResult: *mut isize,
            ) -> i32;
            fn RegDeleteValueA(hKey: isize, lpValueName: *const u8) -> i32;
            fn RegCloseKey(hKey: isize) -> i32;
        }
        const HKEY_LOCAL_MACHINE: isize = -2147483646i64 as isize;
        const HKEY_CURRENT_USER: isize = -2147483647i64 as isize;
        const KEY_SET_VALUE: u32 = 0x0002;

        let key = b"SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run\0";
        let val = b"PhantomTray\0";

        for hive in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
            let mut hkey: isize = 0;
            unsafe {
                if RegOpenKeyExA(hive, key.as_ptr(), 0, KEY_SET_VALUE, &mut hkey) == 0 {
                    RegDeleteValueA(hkey, val.as_ptr());
                    RegCloseKey(hkey);
                }
            }
        }
    }
}

fn print_usage() {
    println!("Phantom Service - Hardware identity privacy orchestrator");
    println!();
    println!("Usage: phantom-svc [OPTION]");
    println!();
    println!("  (no args)       Run as Windows service (started by SCM)");
    println!("  --standalone    Run in foreground (for development/debugging)");
    println!("  --install       Install as a Windows service");
    println!("  --uninstall     Remove the Windows service");
    println!("  --cleanup       Pre-uninstall: revert layers, remove auto-start");
    println!("  --help          Show this help");
}
