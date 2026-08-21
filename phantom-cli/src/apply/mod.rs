pub mod driver_ipc;
pub mod firmware;
pub mod registry;

use crate::profile::schema::HardwareProfile;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Layer {
    Firmware = 0,
    Kernel = 1,
    Userland = 2,
}

impl Layer {
    pub fn name(&self) -> &'static str {
        match self {
            Layer::Firmware => "Layer 0 (DXE/UEFI Firmware)",
            Layer::Kernel => "Layer 1 (Kernel Driver)",
            Layer::Userland => "Layer 2 (Registry/Userland)",
        }
    }
}

pub fn parse_layers(input: &str) -> Result<Vec<Layer>, String> {
    let mut layers = Vec::new();
    for part in input.split(',') {
        match part.trim() {
            "0" | "firmware" | "dxe" => layers.push(Layer::Firmware),
            "1" | "kernel" | "driver" => layers.push(Layer::Kernel),
            "2" | "userland" | "registry" => layers.push(Layer::Userland),
            other => return Err(format!("Unknown layer: '{}'", other)),
        }
    }
    if layers.is_empty() {
        layers.push(Layer::Userland);
    }
    Ok(layers)
}

pub fn apply_profile(
    profile: &HardwareProfile,
    layers: &[Layer],
) -> Vec<(Layer, Result<registry::ApplyResult, String>)> {
    let mut results = Vec::new();

    for layer in layers {
        let result = match layer {
            Layer::Firmware => match firmware::install_dxe_module(profile) {
                Ok(()) => Ok(registry::ApplyResult {
                    applied: vec![
                        "SMBIOS profile written to EFI variable (applies on next boot)".into(),
                    ],
                    failed: vec![],
                    skipped: vec![],
                }),
                Err(e) => Err(e),
            },
            Layer::Kernel => match driver_ipc::send_profile_to_driver(profile) {
                Ok(()) => Ok(registry::ApplyResult {
                    applied: vec!["Kernel profile loaded".into()],
                    failed: vec![],
                    skipped: vec![],
                }),
                Err(e) => Err(e),
            },
            Layer::Userland => Ok(registry::apply_registry_layer(profile)),
        };
        results.push((*layer, result));
    }

    results
}

pub fn revert_all() -> Vec<(Layer, Result<registry::ApplyResult, String>)> {
    let mut results = Vec::new();

    match registry::load_backup() {
        Ok(backup) => {
            let result = registry::revert_registry_layer(&backup);
            let fully_reverted = result.failed.is_empty();
            results.push((Layer::Userland, Ok(result)));
            // Once every original value is restored, the backup has served
            // its purpose. Remove it so `status` reports "original state"
            // rather than "profile applied", and so a later apply starts
            // from a clean baseline. Keep it if any key failed to revert,
            // so the revert can be retried.
            if fully_reverted {
                let _ = std::fs::remove_file(registry::backup_path());
            }
        }
        Err(e) => {
            results.push((Layer::Userland, Err(format!("No backup found: {}", e))));
        }
    }

    let driver_status = driver_ipc::check_driver();
    if driver_status.loaded {
        match driver_ipc::clear_driver_profile() {
            Ok(_) => {}
            Err(e) => results.push((Layer::Kernel, Err(e))),
        }
    }

    match firmware::remove_dxe_module() {
        Ok(_) => {}
        Err(_) => {} // silently ignore — no firmware variable is the normal state
    }

    results
}

pub fn status() -> Vec<(Layer, String)> {
    let mut statuses = Vec::new();

    let fw = firmware::check_firmware();
    statuses.push((Layer::Firmware, firmware::format_firmware_status(&fw)));

    let drv = driver_ipc::check_driver();
    statuses.push((Layer::Kernel, driver_ipc::format_driver_status(&drv)));

    let has_backup = registry::load_backup().is_ok();
    statuses.push((
        Layer::Userland,
        format!(
            "Registry backup: {}",
            if has_backup {
                "present (profile applied)"
            } else {
                "none (original state)"
            },
        ),
    ));

    statuses
}
