//! Boot-time reapply of the operator's active profile.
//!
//! On Linux a spoofed MAC does not survive a reboot: the NIC comes up on
//! its hardware MAC. The systemd unit runs `--reapply` at boot so the
//! active profile's MAC is restored. machine-id and hostname persist on
//! their own; reapplying them is idempotent.
//!
//! This reapplies ONLY a profile the operator explicitly applied
//! (`auto_apply` is set by `phantom apply`). With no such record it does
//! nothing: the service never mints or applies a profile on its own. The
//! reapply leans on the backup machinery's "preserve the true original
//! across a re-apply" rule, so revert still restores the hardware MAC.

use crate::state::ServiceConfig;
use phantom_cli::{apply, profile};

pub struct ReapplySummary {
    pub profile: String,
    pub identifiers: usize,
    pub errors: Vec<String>,
}

/// Map the stored numeric layer tags back to apply layers, dropping any
/// unknown tag. Pure, so it is testable without disk.
fn layers_from_tags(tags: &[u8]) -> Vec<apply::Layer> {
    tags.iter()
        .filter_map(|&t| match t {
            0 => Some(apply::Layer::Firmware),
            1 => Some(apply::Layer::Kernel),
            2 => Some(apply::Layer::Userland),
            _ => None,
        })
        .collect()
}

/// Reapply the configured profile if the operator has protected one.
/// Returns `None` when nothing is configured (the service stays passive).
pub fn reapply_active_profile() -> Option<Result<ReapplySummary, String>> {
    let config = ServiceConfig::load()?;
    let (name, tags) = config.planned()?;
    Some(do_reapply(name, tags))
}

fn do_reapply(name: &str, tags: &[u8]) -> Result<ReapplySummary, String> {
    let prof = profile::load_profile(name)
        .map_err(|e| format!("cannot load profile '{}': {}", name, e))?;

    let layers = layers_from_tags(tags);
    if layers.is_empty() {
        return Err("no valid layers in the active-profile record".into());
    }

    let results = apply::apply_profile(&prof, &layers);

    let mut identifiers = 0;
    let mut errors = Vec::new();
    for (layer, result) in &results {
        match result {
            Ok(r) => {
                identifiers += r.applied.len();
                for (item, err) in &r.failed {
                    errors.push(format!("{}: {} - {}", layer.name(), item, err));
                }
            }
            Err(e) => errors.push(format!("{}: {}", layer.name(), e)),
        }
    }

    Ok(ReapplySummary {
        profile: name.to_string(),
        identifiers,
        errors,
    })
}

/// The `--reapply` entry point. Prints a short summary and exits non-zero
/// if the reapply reported errors, so `systemctl status phantom` shows the
/// failure.
pub fn run_reapply() {
    match reapply_active_profile() {
        None => {
            println!("Phantom: no active profile to reapply.");
            tracing::info!("reapply: no active profile configured");
        }
        Some(Ok(summary)) => {
            if summary.errors.is_empty() {
                println!(
                    "Phantom: reapplied '{}' ({} identifiers).",
                    summary.profile, summary.identifiers
                );
                tracing::info!(
                    profile = %summary.profile,
                    identifiers = summary.identifiers,
                    "reapply complete"
                );
            } else {
                eprintln!(
                    "Phantom: reapplied '{}' with {} error(s):",
                    summary.profile,
                    summary.errors.len()
                );
                for e in &summary.errors {
                    eprintln!("  {}", e);
                }
                tracing::error!(
                    profile = %summary.profile,
                    errors = summary.errors.len(),
                    "reapply had errors"
                );
                std::process::exit(1);
            }
        }
        Some(Err(e)) => {
            eprintln!("Phantom: reapply failed: {}", e);
            tracing::error!(error = %e, "reapply failed");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_from_tags_maps_known() {
        assert_eq!(layers_from_tags(&[2]), vec![apply::Layer::Userland]);
        assert_eq!(
            layers_from_tags(&[0, 1, 2]),
            vec![
                apply::Layer::Firmware,
                apply::Layer::Kernel,
                apply::Layer::Userland
            ]
        );
    }

    #[test]
    fn layers_from_tags_drops_unknown() {
        assert_eq!(layers_from_tags(&[2, 9, 7]), vec![apply::Layer::Userland]);
        assert!(layers_from_tags(&[9]).is_empty());
        assert!(layers_from_tags(&[]).is_empty());
    }
}
