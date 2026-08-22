//! The Linux systemd integration: the boot-time reapply unit and the
//! install/uninstall of it.
//!
//! The unit is `Type=oneshot`. It runs `phantom-svc --reapply` once at
//! boot to restore a spoofed MAC (which does not survive a reboot) and
//! exits. There is no long-running daemon in this phase: on Linux the CLI
//! applies and reverts directly as root, so the only job the service has
//! is reapply-on-boot.

/// The install path the shipped unit and the `.deb`/`.rpm` package use.
pub const PACKAGED_BIN: &str = "/usr/sbin/phantom-svc";

/// The unit as shipped, with the packaged binary path. This committed
/// file is the single source of truth: packaging ships it verbatim and
/// `render_unit` derives the self-install form from it.
const PACKAGED_UNIT: &str = include_str!("../../dist/systemd/phantom.service");

/// The `phantom.service` unit text with `exec_path` as the binary
/// location. `--install` passes the running binary's real path so the
/// unit works wherever the binary sits; packaging uses the shipped path.
pub fn render_unit(exec_path: &str) -> String {
    PACKAGED_UNIT.replace(
        &format!("ExecStart={PACKAGED_BIN} --reapply"),
        &format!("ExecStart={exec_path} --reapply"),
    )
}

/// The path the unit installs to.
#[cfg(target_os = "linux")]
const UNIT_PATH: &str = "/etc/systemd/system/phantom.service";

#[cfg(target_os = "linux")]
pub fn install() {
    // Point the unit at the binary actually running, so a self-install
    // from any location works. Fall back to the packaged path.
    let exe = std::fs::read_link("/proc/self/exe")
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| PACKAGED_BIN.to_string());

    let unit = render_unit(&exe);

    if let Err(e) = std::fs::write(UNIT_PATH, &unit) {
        eprintln!("Failed to write {UNIT_PATH}: {e}");
        eprintln!("Installing the service needs root. Re-run with sudo.");
        std::process::exit(1);
    }

    systemctl(&["daemon-reload"]);
    systemctl(&["enable", "phantom.service"]);

    println!("  Installed {UNIT_PATH} and enabled phantom.service.");
    println!("  It reapplies the active profile on boot so a spoofed MAC survives a reboot.");
    println!("  Apply a profile now with: phantom apply <profile>");
}

#[cfg(target_os = "linux")]
pub fn uninstall() {
    // Stop and disable, then remove the unit. This does NOT revert the
    // identity (revert is explicit, the same as on Windows). Package
    // removal runs `--cleanup` first to restore the true identity.
    systemctl(&["disable", "--now", "phantom.service"]);

    match std::fs::remove_file(UNIT_PATH) {
        Ok(()) => println!("  Removed {UNIT_PATH}."),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            println!("  {UNIT_PATH} was not present.");
        }
        Err(e) => {
            eprintln!("Failed to remove {UNIT_PATH}: {e}");
            eprintln!("Removing the service needs root. Re-run with sudo.");
            std::process::exit(1);
        }
    }

    systemctl(&["daemon-reload"]);
    println!("  phantom.service removed. Run 'phantom revert' to restore identity now.");
}

#[cfg(target_os = "linux")]
fn systemctl(args: &[&str]) {
    match std::process::Command::new("systemctl").args(args).status() {
        Ok(s) if s.success() => {}
        Ok(s) => eprintln!("  Warning: systemctl {} exited with {}", args.join(" "), s),
        Err(e) => eprintln!(
            "  Warning: could not run systemctl {}: {}",
            args.join(" "),
            e
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packaged_unit_has_expected_execstart() {
        assert!(
            PACKAGED_UNIT.contains(&format!("ExecStart={PACKAGED_BIN} --reapply")),
            "the shipped unit must invoke the packaged binary with --reapply"
        );
    }

    #[test]
    fn render_unit_is_identity_for_packaged_path() {
        assert_eq!(render_unit(PACKAGED_BIN), PACKAGED_UNIT);
    }

    #[test]
    fn render_unit_substitutes_exec_path() {
        let u = render_unit("/opt/phantom/phantom-svc");
        assert!(u.contains("ExecStart=/opt/phantom/phantom-svc --reapply"));
        assert!(!u.contains("ExecStart=/usr/sbin/phantom-svc --reapply"));
        // The rest of the unit is unchanged.
        assert!(u.contains("Type=oneshot"));
        assert!(u.contains("RemainAfterExit=yes"));
        assert!(u.contains("WantedBy=multi-user.target"));
        assert!(u.contains("Before=network-pre.target"));
    }

    #[test]
    fn unit_is_oneshot_and_enabled_for_boot() {
        assert!(PACKAGED_UNIT.contains("Type=oneshot"));
        assert!(PACKAGED_UNIT.contains("WantedBy=multi-user.target"));
    }
}
