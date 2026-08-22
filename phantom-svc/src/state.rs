use std::time::Instant;

/// The on-disk active-profile record. Defined once in phantom-cli (where
/// both the CLI `apply` and this service write it) and re-exported here so
/// there is a single shape and a single path, not two that can drift.
pub use phantom_cli::apply::ActiveConfig as ServiceConfig;

pub struct ServiceState {
    pub protected: bool,
    pub active_profile: Option<String>,
    pub active_layers: Vec<u8>,
    pub identifier_count: usize,
    pub start_time: Instant,
    pub shutdown_requested: bool,
}

impl ServiceState {
    pub fn new() -> Self {
        ServiceState {
            protected: false,
            active_profile: None,
            active_layers: Vec::new(),
            identifier_count: 0,
            start_time: Instant::now(),
            shutdown_requested: false,
        }
    }

    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    pub fn set_protected(
        &mut self,
        profile_name: String,
        layers: Vec<u8>,
        identifier_count: usize,
    ) {
        self.protected = true;
        self.active_profile = Some(profile_name.clone());
        self.active_layers = layers.clone();
        self.identifier_count = identifier_count;
        let _ = ServiceConfig {
            active_profile: Some(profile_name),
            auto_apply: true,
            layers,
        }
        .save();
    }

    pub fn set_unprotected(&mut self) {
        self.protected = false;
        self.active_profile = None;
        self.active_layers.clear();
        self.identifier_count = 0;
        let _ = ServiceConfig {
            active_profile: None,
            auto_apply: false,
            layers: Vec::new(),
        }
        .save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state_is_unprotected() {
        let state = ServiceState::new();
        assert!(!state.protected);
        assert!(state.active_profile.is_none());
        assert!(state.active_layers.is_empty());
        assert!(!state.shutdown_requested);
    }

    #[test]
    fn protect_and_unprotect() {
        let mut state = ServiceState::new();

        state.set_protected("test-profile".into(), vec![1, 2], 20);
        assert!(state.protected);
        assert_eq!(state.active_profile.as_deref(), Some("test-profile"));
        assert_eq!(state.active_layers, vec![1, 2]);

        state.set_unprotected();
        assert!(!state.protected);
        assert!(state.active_profile.is_none());
        assert!(state.active_layers.is_empty());
    }

    #[test]
    fn uptime_is_non_negative() {
        let state = ServiceState::new();
        assert!(state.uptime_secs() < 2);
    }

    #[test]
    fn config_round_trip() {
        let config = ServiceConfig {
            active_profile: Some("test".into()),
            auto_apply: true,
            layers: vec![1, 2],
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: ServiceConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.active_profile.as_deref(), Some("test"));
        assert!(parsed.auto_apply);
        assert_eq!(parsed.layers, vec![1, 2]);
    }

    #[test]
    fn config_default_is_inactive() {
        let config = ServiceConfig::default();
        assert!(config.active_profile.is_none());
        assert!(!config.auto_apply);
        assert!(config.layers.is_empty());
    }
}
