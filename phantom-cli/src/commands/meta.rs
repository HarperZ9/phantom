use crate::commands::util::print_document;
use phantom_cli::json_out::{
    Envelope, SelfCheckBuild, SelfCheckPayload, TamperEventDto, TamperReportPayload, VersionPayload,
};
use phantom_cli::{build_info, config, profile};

pub fn version(json: bool) {
    if json {
        Envelope::ok(
            "version",
            VersionPayload {
                name: build_info::PKG_NAME,
                version: build_info::VERSION,
                git_commit: build_info::GIT_COMMIT,
                target: build_info::BUILD_TARGET,
                profile: build_info::BUILD_PROFILE,
            },
        )
        .print();
    } else {
        println!("{}", build_info::full_version_string());
    }
}

pub fn self_check(json: bool) {
    let detect = phantom_license::integrity::full_self_check();
    let data_dir = profile::data_dir();

    // Time anchor: 'ok' or 'rewound' or 'first-time'
    let anchor_verdict = phantom_license::time_anchor::check_and_advance(&data_dir);
    let anchor_str = match anchor_verdict {
        phantom_license::time_anchor::AnchorVerdict::Ok => "ok",
        phantom_license::time_anchor::AnchorVerdict::ClockRewound { .. } => "rewound",
        phantom_license::time_anchor::AnchorVerdict::NoAnchor => "first-time",
    };

    // A successful LicenseGuard::load() implies state_mac
    // verified AND time anchor was not rewound. We reproduce
    // the check by loading and comparing tiers.
    let guard = phantom_license::LicenseGuard::load();
    let license_state_ok = guard.is_licensed() || !data_dir.join(".license.json").exists();

    let cooldown = phantom_license::rate_limit::required_cooldown_secs(&data_dir);

    let healthy = detect.all_clear && anchor_str != "rewound" && license_state_ok;

    if json {
        Envelope::ok(
            "self-check",
            SelfCheckPayload {
                healthy,
                debugger_detected: !detect.all_clear,
                debugger_detectors_triggered: detect.triggered.clone(),
                time_anchor: anchor_str,
                license_state_verified: license_state_ok,
                activation_cooldown_secs: cooldown,
                master_key_generation: phantom_license::master_key_generation(),
                build: SelfCheckBuild {
                    version: build_info::VERSION,
                    git_commit: build_info::GIT_COMMIT,
                    target: build_info::BUILD_TARGET,
                    profile: build_info::BUILD_PROFILE,
                },
            },
        )
        .print();
    } else {
        println!("\n  Phantom Self-Check\n  {}\n", "=".repeat(50));
        println!(
            "  Overall:            {}",
            if healthy { "HEALTHY" } else { "DEGRADED" }
        );
        if detect.all_clear {
            println!("  Debugger detected:  no");
        } else {
            println!(
                "  Debugger detected:  YES (triggered: {})",
                detect.triggered.join(", ")
            );
        }
        println!("  Time anchor:        {}", anchor_str);
        println!(
            "  License state:      {}",
            if license_state_ok {
                "verified"
            } else {
                "tamper detected"
            }
        );
        println!("  Activation cooldown: {} sec", cooldown);
        println!(
            "  Master key gen:     {}",
            phantom_license::master_key_generation()
        );
        println!(
            "  Build:              {}",
            build_info::full_version_string()
        );
        println!();
        if !healthy {
            std::process::exit(1);
        }
    }
}

pub fn tamper_report(clear: bool, json: bool) {
    // The tripwire log is a strictly local record. This
    // command reads it, optionally clears it, and prints it
    // to stdout. It NEVER transmits over the network — the
    // operator decides whether to share the output with
    // support, and can pipe it through their own redactor
    // first if they wish.
    let data_dir = profile::data_dir();
    let events = phantom_license::tripwire::read_events(&data_dir);
    let tripped = phantom_license::tripwire::is_tripped(&data_dir);

    if json {
        Envelope::ok(
            "tamper report",
            TamperReportPayload {
                tripped,
                events: events
                    .iter()
                    .map(|e| TamperEventDto {
                        unix_secs: e.unix_secs,
                        severity: match e.severity {
                            phantom_license::tripwire::Severity::Low => "low",
                            phantom_license::tripwire::Severity::High => "high",
                        },
                        reason: e.reason.clone(),
                    })
                    .collect(),
                note: "Local file only. Never transmitted over the network.",
            },
        )
        .print();
    } else {
        println!("\n  Phantom Tamper Report\n  {}\n", "=".repeat(50));
        println!(
            "  Overall:  {}",
            if tripped {
                "TRIPPED — install silently downgraded to Free tier"
            } else {
                "clean"
            }
        );
        println!("  Events:   {}", events.len());
        if events.is_empty() {
            println!("  (no tripwire events recorded)");
        } else {
            println!();
            for e in &events {
                let sev = match e.severity {
                    phantom_license::tripwire::Severity::Low => "low ",
                    phantom_license::tripwire::Severity::High => "HIGH",
                };
                println!("    [{}] t={} reason={}", sev, e.unix_secs, e.reason);
            }
        }
        println!();
        println!("  This report never leaves this machine unless you share it.");
        println!("  A successful `phantom license activate <your-key>` also clears it.");
        println!();
    }

    if clear {
        phantom_license::tripwire::clear(&data_dir);
        if !json {
            println!("  Log cleared.\n");
        }
    }
}

pub fn privacy_notice() {
    let cfg = config::load_from_file();
    print_document("PRIVACY NOTICE", phantom_license::legal::PRIVACY_NOTICE);
    println!(
        "  Version shipping in this build: {}",
        phantom_license::legal::PRIVACY_NOTICE_VERSION
    );
    match (
        cfg.privacy_notice_version_accepted,
        cfg.privacy_notice_acknowledged_at,
    ) {
        (Some(v), Some(t)) if v >= phantom_license::legal::PRIVACY_NOTICE_VERSION => {
            println!("  This install: acknowledged version {} at unix {}", v, t);
        }
        (Some(v), _) => {
            println!(
                "  This install: acknowledged version {} (STALE — current is {})",
                v,
                phantom_license::legal::PRIVACY_NOTICE_VERSION
            );
        }
        (None, _) => {
            println!("  This install: NOT YET ACKNOWLEDGED");
        }
    }
    println!(
        "  Phone-home currently: {}",
        if cfg.phone_home_active() {
            "ACTIVE"
        } else {
            "inactive"
        }
    );
    println!();
}

pub fn tou() {
    let cfg = config::load_from_file();
    print_document("TERMS OF USE", phantom_license::legal::TOU);
    println!(
        "  Version shipping in this build: {}",
        phantom_license::legal::TOU_VERSION
    );
    match (cfg.tou_version_accepted, cfg.tou_accepted_at) {
        (Some(v), Some(t)) if v >= phantom_license::legal::TOU_VERSION => {
            println!("  This install: accepted version {} at unix {}", v, t);
        }
        (Some(v), _) => {
            println!(
                "  This install: accepted version {} (STALE — current is {})",
                v,
                phantom_license::legal::TOU_VERSION
            );
        }
        (None, _) => {
            println!("  This install: NOT YET ACCEPTED");
        }
    }
    println!();
}
