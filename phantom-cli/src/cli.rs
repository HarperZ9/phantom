use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "phantom",
    about = "Hardware identity privacy tool for machines you own or are authorized to test",
    long_about = "Phantom generates realistic, internally-consistent hardware identity profiles \
                  and applies them across multiple system layers, giving users control over \
                  what their machine reports to software.\n\n\
                  For use on machines you own or are expressly authorized to test. \
                  Not for evading fraud controls, circumventing lawful court orders, \
                  or misrepresenting a device you do not control. \
                  Run `phantom tou` for the full Terms of Use.",
    version
)]
pub struct Cli {
    /// Emit machine-readable JSON to stdout instead of the pretty text UI.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Read and report all hardware identifiers currently visible on this machine
    Audit,

    /// Manage hardware identity profiles
    Profile {
        #[command(subcommand)]
        action: ProfileAction,
    },

    /// Apply a profile to the system
    Apply {
        /// Profile name to apply
        name: String,

        /// Comma-separated layers to apply (0=firmware, 1=kernel, 2=userland)
        #[arg(long, default_value = "2")]
        layers: String,
    },

    /// Validate that active spoofing is consistent across all identifier sources
    Validate {
        /// Profile name to validate against
        name: String,
    },

    /// Restore original hardware identifiers
    Revert,

    /// Show current spoofing status for each layer
    Status,

    /// Communicate with the Phantom background service
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },

    /// Manage license activation
    License {
        #[command(subcommand)]
        action: LicenseAction,
    },

    /// Show or edit Phantom configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Control local model-disclosure boundary policies and gateway
    Boundary {
        #[command(subcommand)]
        action: BoundaryAction,
    },

    /// Print detailed version and build information
    Version,

    /// Report anti-tamper / integrity self-check status
    SelfCheck,

    /// Show the local tamper-tripwire log (never leaves this machine)
    TamperReport {
        /// Clear the log after displaying it. Same effect as running
        /// `license activate <valid-key>` — the log resets when the
        /// operator proves they hold a real license.
        #[arg(long)]
        clear: bool,
    },

    /// Show the Phantom Privacy Notice describing what phone-home
    /// sends. Also reports whether this install has acknowledged it.
    PrivacyNotice,

    /// Show the Phantom Terms of Use and this install's acceptance
    /// status.
    Tou,
}

impl Commands {
    pub fn allows_phone_home(&self) -> bool {
        !matches!(self, Commands::Boundary { .. })
    }
}

#[derive(Subcommand)]
pub enum ProfileAction {
    /// Generate a new hardware identity profile
    Generate {
        /// Profile name
        name: String,

        /// Seed string for deterministic generation (same seed = same profile)
        #[arg(long)]
        seed: Option<String>,

        /// Print profile to stdout without saving
        #[arg(long)]
        dry_run: bool,
    },

    /// Display a saved profile
    Show {
        /// Profile name
        name: String,
    },

    /// List all saved profiles
    List,

    /// Export a profile as JSON to stdout
    Export {
        /// Profile name
        name: String,
    },

    /// Import a profile from a JSON file
    Import {
        /// Path to JSON file
        path: String,
    },

    /// Delete a saved profile
    Delete {
        /// Profile name
        name: String,
    },
}

#[derive(Subcommand)]
pub enum ServiceAction {
    /// Check if the Phantom service is running
    Status,

    /// Ask the service to apply a profile (protected mode)
    Protect {
        /// Profile name
        name: String,

        /// Comma-separated layers (0=firmware, 1=kernel, 2=userland)
        #[arg(long, default_value = "2")]
        layers: String,
    },

    /// Ask the service to revert all spoofing (unprotected mode)
    Unprotect,

    /// Ping the service
    Ping,
}

#[derive(Subcommand)]
pub enum LicenseAction {
    /// Show current license status
    Status,

    /// Activate a license key
    Activate {
        /// License key string
        key: String,
        /// Non-interactive acceptance of the current Terms of Use.
        /// Required in headless environments (SCCM, containers,
        /// unattended installers). In an interactive TTY the tool
        /// prompts.
        #[arg(long)]
        accept_tou: bool,
        /// Non-interactive acknowledgment of the current Privacy
        /// Notice (the phone-home disclosure). Required for
        /// unattended activation; interactive TTY prompts.
        #[arg(long)]
        acknowledge_privacy_notice: bool,
    },

    /// Deactivate the current license
    Deactivate,

    /// Show this machine's hardware fingerprint (for license binding)
    Fingerprint,

    /// Print an enrollment request the licensing team can turn into a
    /// key. Includes machine fingerprint, requested tier, build info,
    /// and current tier so the request stands on its own.
    Request {
        /// Tier to request: free, pro, enterprise
        #[arg(long, default_value = "pro")]
        tier: String,
    },
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Show the resolved runtime configuration (env > file > defaults)
    Show,

    /// Print the config file path this session would read/write
    Path,

    /// Write a default config file to disk (fails if one already exists)
    Init,

    /// Set a single config key and save. Keys: 'data_dir', 'pipe_name',
    /// 'log_level', 'license_key', 'telemetry_enabled', 'phone_home_url',
    /// 'phone_home_enabled', 'phone_home_interval_secs'.
    Set {
        /// Config key name
        key: String,
        /// Value to store
        value: String,
    },
}

#[derive(Subcommand)]
pub enum BoundaryAction {
    /// Compute a session-bound keyed content id for a request body
    ContentId {
        /// Boundary session id the approval belongs to
        #[arg(long)]
        session: String,
        /// Request body file, or '-' for stdin
        #[arg(long)]
        content: PathBuf,
        /// Environment variable containing the operator policy HMAC key
        #[arg(long, default_value = "PHANTOM_BOUNDARY_POLICY_KEY")]
        key_env: String,
    },

    /// Seal an operator-authored boundary policy with a local HMAC key
    Seal {
        /// Policy JSON with policy_mac empty or absent
        #[arg(long)]
        policy: PathBuf,
        /// Output path for the sealed policy. Prints to stdout when omitted.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Environment variable containing the operator policy HMAC key
        #[arg(long, default_value = "PHANTOM_BOUNDARY_POLICY_KEY")]
        key_env: String,
    },

    /// Check one request body against a sealed operator policy
    Check {
        /// Sealed policy JSON
        #[arg(long)]
        policy: PathBuf,
        /// Boundary session id for this request
        #[arg(long)]
        session: String,
        /// Request body file, or '-' for stdin
        #[arg(long)]
        content: PathBuf,
        /// Operation label to check
        #[arg(long, default_value = "chat.completions.create")]
        operation: String,
        /// Environment variable containing the operator policy HMAC key
        #[arg(long, default_value = "PHANTOM_BOUNDARY_POLICY_KEY")]
        key_env: String,
    },

    /// Serve the local loopback gateway for POST /v1/chat/completions
    Serve {
        /// Sealed policy JSON
        #[arg(long)]
        policy: PathBuf,
        /// Numeric loopback address and port to bind, for example 127.0.0.1:43117
        #[arg(long)]
        listen: SocketAddr,
        /// Receipt JSONL path. Defaults to <data_dir>/logs/boundary-receipts.jsonl.
        #[arg(long)]
        receipt_log: Option<PathBuf>,
        /// Environment variable containing the operator policy HMAC key
        #[arg(long, default_value = "PHANTOM_BOUNDARY_POLICY_KEY")]
        key_env: String,
        /// Environment variable containing the gateway bearer token
        #[arg(long, default_value = "PHANTOM_BOUNDARY_AUTH")]
        auth_token_env: String,
    },
}
