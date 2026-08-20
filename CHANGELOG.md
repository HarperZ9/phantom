# Changelog

All notable changes to Phantom are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[SemVer](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Adversarial license-key fuzz test: every single-bit flip of a valid key
  is confirmed to fail HMAC verification (480 cases per run)
- Truncated-key and noisy-input license validation coverage
- Pinned assertion on the license signing key so refactors that would
  invalidate all issued keys fail loudly in CI

## [0.5.0] — Sprint 10

### Added
- GitHub Actions CI: rustfmt, clippy, cross-platform build+test on
  Ubuntu and Windows, cargo-audit for security advisories
- Release workflow: tagged pushes build Linux and Windows binary
  archives and attach them to a GitHub Release
- `phantom config` subcommand tree: `show`, `path`, `init`, `set`
- Global `--json` flag emitting a stable `{ok, command, data | error}`
  envelope for `status`, `license status`, `config show`, `profile list`
- JSON config file at `$PHANTOM_CONFIG` or `<data_dir>/config.json` with
  precedence env > file > defaults
- `PHANTOM_TELEMETRY` environment variable (opt-in flag; disabled by
  default; wired into the resolved config, no network calls yet)

### Changed
- Workspace-wide rustfmt normalization
- README documents the resolution order, config subcommands, and JSON
  envelope contract

## [0.4.x] — Sprints 6-9

### Added
- Sprint 6: security hardening (SDDL pipe ACL, CSPRNG seed generation,
  release profile with LTO, panic=abort, strip)
- Sprint 7: structured logging via `tracing` with daily file rotation
  under `<data_dir>/logs/`, log level from `PHANTOM_LOG_LEVEL`
- Sprint 8: DRM/licensing system — HMAC-SHA256 keys, machine
  fingerprint binding, tier-gated layer access (Free/Pro/Enterprise),
  binary integrity self-check, constant-time comparison, debugger
  detection
- Sprint 9: enterprise configuration via `PHANTOM_DATA_DIR`,
  `PHANTOM_PIPE_NAME`, centralized deployment support

### Changed
- License changed from MIT to Phantom Proprietary License
- All crate `license` metadata updated

## [0.3.x] — Sprints 3-5

### Added
- Sprint 3: system tray application (shield icon, status popup, toast
  notifications, context menu, auto-start)
- Sprint 4: WiX MSI installer, first-run auto-generation, config
  persistence, profile quick-switch
- Sprint 5: system identifier readers — SMBIOS parser, disk/network/GPU
  /display/TPM registry readers, source key alignment, real
  identifier_count, revert warning propagation

## [0.2.x] — Sprints 1-2

### Added
- Sprint 1: profile engine with vendor-accurate generation, profile
  management (save/load/export/import), hardware audit, cross-source
  validation
- Sprint 2: named-pipe IPC protocol, background service, CLI service
  commands, Layer 2 registry spoofing, Layer 1 kernel driver
  scaffolding, Layer 0 DXE firmware module scaffolding
