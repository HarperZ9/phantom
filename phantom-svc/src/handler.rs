use crate::state::{ServiceConfig, ServiceState};
use phantom_cli::{apply, profile};
use phantom_ipc::message::{ErrorCode, ProfileInfo, Request, Response, ServiceStatus};
use phantom_ipc::RequestHandler;

pub struct PhantomHandler {
    pub state: ServiceState,
}

impl PhantomHandler {
    pub fn new() -> Self {
        let mut handler = PhantomHandler {
            state: ServiceState::new(),
        };
        handler.restore_configured_profile();
        handler
    }

    /// Re-apply a profile the operator previously activated, so spoofing
    /// survives a reboot. This ONLY restores an explicitly-configured
    /// profile (`auto_apply` is set by `set_protected` after a real
    /// `apply`); with no such config the service stays unprotected.
    ///
    /// It deliberately does NOT generate or apply a profile on its own.
    /// An earlier version, on finding an empty profile store, minted a
    /// random "default" profile and applied it — spoofing the machine's
    /// identity at service start with no license check and no user
    /// consent, and polluting the registry backup so uninstall could not
    /// restore the true original. Identity changes must always originate
    /// from an explicit, licensed, consented operator `apply`.
    fn restore_configured_profile(&mut self) {
        if let Some(config) = ServiceConfig::load() {
            if config.auto_apply {
                if let Some(ref name) = config.active_profile {
                    self.do_protect(name, &config.layers);
                }
            }
        }
    }

    fn get_status(&self) -> ServiceStatus {
        let driver_connected = {
            let drv = apply::driver_ipc::check_driver();
            drv.loaded
        };
        let firmware_detected = {
            let fw = apply::firmware::check_firmware();
            fw.dxe_module_installed
        };

        ServiceStatus {
            protected: self.state.protected,
            active_profile: self.state.active_profile.clone(),
            active_layers: self.state.active_layers.clone(),
            uptime_secs: self.state.uptime_secs(),
            driver_connected,
            firmware_detected,
            identifier_count: self.state.identifier_count,
        }
    }

    fn do_protect(&mut self, profile_name: &str, layers: &[u8]) -> Response {
        let prof = match profile::load_profile(profile_name) {
            Ok(p) => p,
            Err(e) => {
                return Response::Error {
                    code: ErrorCode::ProfileNotFound,
                    message: format!("cannot load profile '{}': {}", profile_name, e),
                };
            }
        };

        let parsed_layers: Vec<apply::Layer> = layers
            .iter()
            .filter_map(|&l| match l {
                0 => Some(apply::Layer::Firmware),
                1 => Some(apply::Layer::Kernel),
                2 => Some(apply::Layer::Userland),
                _ => None,
            })
            .collect();

        if parsed_layers.is_empty() {
            return Response::Error {
                code: ErrorCode::InvalidRequest,
                message: "no valid layers specified".into(),
            };
        }

        let results = apply::apply_profile(&prof, &parsed_layers);

        let mut total_applied = 0;
        let mut errors = Vec::new();

        for (layer, result) in &results {
            match result {
                Ok(r) => {
                    total_applied += r.applied.len();
                    for (item, err) in &r.failed {
                        errors.push(format!("{}: {} - {}", layer.name(), item, err));
                    }
                }
                Err(e) => {
                    errors.push(format!("{}: {}", layer.name(), e));
                }
            }
        }

        if !errors.is_empty() && total_applied == 0 {
            return Response::Error {
                code: ErrorCode::DriverError,
                message: errors.join("; "),
            };
        }

        let applied_layer_nums: Vec<u8> = parsed_layers
            .iter()
            .map(|l| match l {
                apply::Layer::Firmware => 0,
                apply::Layer::Kernel => 1,
                apply::Layer::Userland => 2,
            })
            .collect();

        self.state.set_protected(
            profile_name.to_string(),
            applied_layer_nums.clone(),
            total_applied,
        );

        Response::Applied {
            layers_applied: applied_layer_nums,
            identifiers: total_applied,
        }
    }

    fn do_unprotect(&mut self) -> Response {
        let results = apply::revert_all();

        let mut warnings = Vec::new();
        for (layer, result) in &results {
            if let Err(e) = result {
                warnings.push(format!("{}: {}", layer.name(), e));
            }
        }

        self.state.set_unprotected();

        Response::Reverted { warnings }
    }

    fn do_list_profiles(&self) -> Response {
        match profile::list_profiles() {
            Ok(names) => {
                let mut list = Vec::new();
                for name in &names {
                    match profile::load_profile(name) {
                        Ok(p) => list.push(ProfileInfo {
                            name: p.metadata.name.clone(),
                            seed: p.metadata.seed.clone(),
                            identifier_count: p.identifier_count(),
                        }),
                        Err(_) => list.push(ProfileInfo {
                            name: name.clone(),
                            seed: String::new(),
                            identifier_count: 0,
                        }),
                    }
                }
                Response::Profiles { list }
            }
            Err(e) => Response::Error {
                code: ErrorCode::InternalError,
                message: format!("cannot list profiles: {}", e),
            },
        }
    }

    fn do_get_profile(&self, name: &str) -> Response {
        match profile::load_profile(name) {
            Ok(p) => match serde_json::to_value(&p) {
                Ok(v) => Response::Profile { data: v },
                Err(e) => Response::Error {
                    code: ErrorCode::InternalError,
                    message: format!("serialization error: {}", e),
                },
            },
            Err(e) => Response::Error {
                code: ErrorCode::ProfileNotFound,
                message: format!("profile '{}': {}", name, e),
            },
        }
    }

    fn do_generate(&mut self, name: &str, seed: &str) -> Response {
        let prof = profile::engine::generate_profile(seed, name);
        let count = prof.identifier_count();

        match profile::save_profile(&prof) {
            Ok(_) => Response::Generated {
                name: name.to_string(),
                identifiers: count,
            },
            Err(e) => Response::Error {
                code: ErrorCode::InternalError,
                message: format!("cannot save profile: {}", e),
            },
        }
    }

    fn do_delete(&mut self, name: &str) -> Response {
        let dir = profile::profiles_dir();
        let path = dir.join(format!("{}.json", name));

        if !path.exists() {
            return Response::Error {
                code: ErrorCode::ProfileNotFound,
                message: format!("profile '{}' not found", name),
            };
        }

        match std::fs::remove_file(&path) {
            Ok(_) => {
                if self.state.active_profile.as_deref() == Some(name) {
                    self.state.set_unprotected();
                }
                Response::Deleted {
                    name: name.to_string(),
                }
            }
            Err(e) => Response::Error {
                code: ErrorCode::InternalError,
                message: format!("cannot delete profile: {}", e),
            },
        }
    }
}

impl RequestHandler for PhantomHandler {
    fn handle(&mut self, request: Request) -> Response {
        let request_type = format!("{:?}", std::mem::discriminant(&request));
        tracing::debug!(request = %request_type, "handling IPC request");

        let response = match request {
            Request::Ping => Response::Pong {
                version: phantom_ipc::PROTOCOL_VERSION,
            },
            Request::GetStatus => Response::Status(self.get_status()),
            Request::Protect {
                profile_name,
                layers,
            } => {
                tracing::info!(profile = %profile_name, ?layers, "protect requested");
                self.do_protect(&profile_name, &layers)
            }
            Request::Unprotect => {
                tracing::info!("unprotect requested");
                self.do_unprotect()
            }
            Request::ListProfiles => self.do_list_profiles(),
            Request::GetProfile { name } => self.do_get_profile(&name),
            Request::GenerateProfile { name, seed } => {
                tracing::info!(name = %name, "generating profile");
                self.do_generate(&name, &seed)
            }
            Request::DeleteProfile { name } => {
                tracing::info!(name = %name, "deleting profile");
                self.do_delete(&name)
            }
            Request::Shutdown => {
                tracing::warn!("shutdown requested via IPC");
                self.state.shutdown_requested = true;
                Response::Ok {
                    message: "service shutting down".into(),
                }
            }
        };

        if let Response::Error {
            ref code,
            ref message,
        } = response
        {
            tracing::warn!(code = %code, message = %message, "request failed");
        }

        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handler_ping() {
        let mut handler = PhantomHandler::new();
        let resp = handler.handle(Request::Ping);
        match resp {
            Response::Pong { version } => assert_eq!(version, phantom_ipc::PROTOCOL_VERSION),
            _ => panic!("expected Pong"),
        }
    }

    #[test]
    fn handler_status_starts_unprotected() {
        let mut handler = PhantomHandler::new();
        let resp = handler.handle(Request::GetStatus);
        match resp {
            Response::Status(s) => {
                assert!(!s.protected);
                assert!(s.active_profile.is_none());
                assert!(s.active_layers.is_empty());
            }
            _ => panic!("expected Status"),
        }
    }

    /// Regression for rc1 dogfood Bug B: a fresh service must never spoof
    /// the machine on its own. The old `restore_or_first_run`, on finding
    /// an empty profile store, minted a random "default" profile and
    /// applied it at startup with no license and no consent. Point the
    /// store at an empty, unique dir so no stale config triggers a
    /// restore, then confirm a new handler sits unprotected and created
    /// no auto-generated "default" profile.
    #[test]
    fn fresh_handler_does_not_auto_apply() {
        let dir =
            std::env::temp_dir().join(format!("phantom-svc-noautoapply-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::env::set_var("PHANTOM_DATA_DIR", &dir);

        let handler = PhantomHandler::new();
        assert!(
            !handler.state.protected,
            "service must not self-protect at startup"
        );
        assert!(handler.state.active_profile.is_none());
        assert!(handler.state.active_layers.is_empty());

        let default_profile = profile::profiles_dir().join("default.json");
        assert!(
            !default_profile.exists(),
            "service must not auto-generate a 'default' profile"
        );

        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("PHANTOM_DATA_DIR");
    }

    #[test]
    fn handler_shutdown_sets_flag() {
        let mut handler = PhantomHandler::new();
        let resp = handler.handle(Request::Shutdown);
        assert!(matches!(resp, Response::Ok { .. }));
        assert!(handler.state.shutdown_requested);
    }

    #[test]
    fn handler_protect_missing_profile() {
        let mut handler = PhantomHandler::new();
        let resp = handler.handle(Request::Protect {
            profile_name: "nonexistent-profile-xyz".into(),
            layers: vec![2],
        });
        assert!(matches!(
            resp,
            Response::Error {
                code: ErrorCode::ProfileNotFound,
                ..
            }
        ));
    }

    #[test]
    fn handler_protect_invalid_layers() {
        let mut handler = PhantomHandler::new();
        let resp = handler.handle(Request::Protect {
            profile_name: "test".into(),
            layers: vec![99],
        });
        match resp {
            Response::Error { code, .. } => {
                assert!(matches!(
                    code,
                    ErrorCode::InvalidRequest | ErrorCode::ProfileNotFound
                ));
            }
            _ => panic!("expected Error"),
        }
    }

    #[test]
    fn handler_get_missing_profile() {
        let mut handler = PhantomHandler::new();
        let resp = handler.handle(Request::GetProfile {
            name: "nonexistent-xyz".into(),
        });
        assert!(matches!(
            resp,
            Response::Error {
                code: ErrorCode::ProfileNotFound,
                ..
            }
        ));
    }

    #[test]
    fn handler_delete_missing_profile() {
        let mut handler = PhantomHandler::new();
        let resp = handler.handle(Request::DeleteProfile {
            name: "nonexistent-xyz".into(),
        });
        assert!(matches!(
            resp,
            Response::Error {
                code: ErrorCode::ProfileNotFound,
                ..
            }
        ));
    }

    #[test]
    fn handler_generate_and_list() {
        let mut handler = PhantomHandler::new();

        let resp = handler.handle(Request::GenerateProfile {
            name: "svc-test-gen".into(),
            seed: "svc-seed".into(),
        });
        match resp {
            Response::Generated { name, identifiers } => {
                assert_eq!(name, "svc-test-gen");
                assert!(identifiers > 20);
            }
            _ => panic!("expected Generated, got {:?}", resp),
        }

        let resp = handler.handle(Request::ListProfiles);
        match resp {
            Response::Profiles { list } => {
                assert!(list.iter().any(|p| p.name == "svc-test-gen"));
            }
            _ => panic!("expected Profiles"),
        }

        // Cleanup
        handler.handle(Request::DeleteProfile {
            name: "svc-test-gen".into(),
        });
    }
}
