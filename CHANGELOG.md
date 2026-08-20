# Changelog

All notable changes to Phantom are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[SemVer](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Adversarial license-key fuzz test: every single-bit flip of a valid key
  is confirmed to fail HMAC verification (480 cases per run)
- Truncated-key and noisy-input license validation coverage

### Changed — Sprint 12: anti-reversal hardening
- **Master signing key is now XOR-obfuscated at build time.** `build.rs`
  scrambles the seed into per-byte-position XOR bytes and emits only the
  obfuscated array. The runtime `keys::master_key()` unscrambles into a
  stack buffer with an `#[inline(never)]` derivation to prevent
  constant-folding the plaintext back into `.rodata`. Verified: `strings`
  on the release binary reports **0** hits for the old plaintext key.
- **Domain-separated key derivation.** Every subsystem now takes a
  purpose-specific subkey (`LICENSE_PURPOSE`, `INTEGRITY_PURPOSE`,
  `STATE_PURPOSE`, `TIME_ANCHOR_PURPOSE`) via HMAC-SHA256(master,
  purpose). Recovering one subkey does not expose the others.
- **Time-anchor / clock-rollback defense.** `LicenseGuard::load()` now
  writes a HMAC'd monotone-forward anchor to `<data_dir>/.time_anchor`.
  A wall clock rewound by more than 24 hours (`GRACE_SECS`) causes the
  guard to refuse any stored license, blocking the "set the clock back"
  bypass of license expiration.
- **License-key signing pin migrated** to the SHA-256 of the derived
  subkey (no longer pins the plaintext string, which would defeat the
  obfuscation).

### Added — machine fingerprint diversity (Linux)
- `product_serial` DMI reader
- `/proc/cpuinfo` vendor + model line
- Sorted MAC addresses for every non-virtual interface (skips `lo`,
  `docker*`, `veth*`, `br-*`)

### Security
- The plaintext `SIGNING_KEY` constant is gone. The old key was
  `phantom-license-hmac-v1-key` — every pre-Sprint-12 license was
  signed with it and is now invalid. No customer licenses were issued
  under it, so no field migration is required.

### Changed — Sprint 13: state integrity + build info
- **License state file is HMAC-signed.** `<data_dir>/.license.json`
  now carries a `state_mac` field over `(key, activated_at)` using the
  STATE_PURPOSE subkey. Rewriting `activated_at` (to age the record
  forward past the time anchor grace, or backward to earn free days)
  or swapping in an unrelated key both fail the MAC. Legacy records
  without the field load once for migration and are re-signed.
- **Build metadata baked in.** `phantom-cli/build.rs` reads the git
  short SHA, target triple, and cargo profile at compile time and
  exposes them via the new `phantom version` subcommand and the
  `--json version` payload. A `-dirty` suffix flags builds from a
  worktree with uncommitted changes.

### Added
- `phantom version` and `phantom --json version`
- `phantom_cli::build_info` module surfacing all compile-time metadata

### Changed — Sprint 14: profile watermarking + memory hygiene
- **Every generated profile carries a signed `origin_mark`.** The mark
  covers `SHA-256(canonical_profile) || origin_fingerprint || tier ||
  issued_epoch_days`, signed with the STATE_PURPOSE subkey. Import
  policy:
  - **Unmarked** — legacy profile, loads with a note
  - **Local** — verified, generated on this machine, loads silently
  - **Foreign** — verified, generated elsewhere; **loads only for Pro
    and Enterprise tiers**, Free tier refuses
  - **ContentTampered** — the mark's covered hash disagrees with the
    profile bytes (someone hand-edited a marked file); rejected
  - **Invalid** — mark present but MAC forged; rejected
  - **Malformed** — structural corruption in the mark; rejected
- **Master key stack buffers are volatile-zeroed** after each subkey
  derivation. `volatile_zero()` uses `ptr::write_volatile` + a Release
  fence so the writes are not dead-code-eliminated the way a plain
  `= [0; N]` on a stack-local would be.

### Added
- `phantom_license::watermark` module with `sign`, `sign_bytes`,
  `verify`, `Verdict`, `OriginMark` public surface (9 tests)
- `phantom_cli::profile::sign_profile`, `check_origin`, `ImportVerdict`
  helpers (used by `save_profile` and the CLI import handler)

### Changed — Sprint 15: rate-limit + self-check
- **Activation is rate-limited.** `LicenseGuard::activate()` consults
  a HMAC-signed attempt log at `<data_dir>/.activation_attempts` before
  exercising any key material. First 5 failed attempts within a rolling
  1-hour window are free; each further failure earns an exponentially-
  growing backoff (30s → 60s → 120s → ..., capped at 1h). Successful
  activation clears the log; forged log entries fail MAC verification
  and are discarded on load. Blocks feed-the-key-generator brute force.
- **New `LicenseError::RateLimited(secs)` variant** surfaces the
  required wait time to the CLI, which prints a human-readable message.

### Added
- `phantom self-check` subcommand: reports debugger detection, time
  anchor state, license-state verification, activation cooldown,
  master key generation, and full build info. Exits 1 when any check
  fails. `--json` gives a machine-readable `SelfCheckPayload`.
- `phantom_license::rate_limit` module (7 tests: no-history, free
  window, sixth-attempt trigger, backoff cap, clear-on-success,
  forged-entry rejection, MAC roundtrip).
- `phantom_license::master_key_generation()` public accessor so
  operators can confirm the master-key generation their binary was
  built against without exposing key material.

### Changed — Sprint 16: detector ensemble + process hardening
- **Anti-debugger detection is an ensemble now.** `full_self_check()`
  returns a `DetectionVerdict { all_clear, triggered }` listing every
  detector that fired. Patching out one detector no longer disables
  the whole check.
  - Linux: `tracer_pid` (`/proc/self/status`), `ld_preload`
    (environment shim/interposition), `debugger_env`
    (`LD_AUDIT`, `MALLOC_TRACE`, `GDB_PYTHON`, `FRIDA_AGENT`,
    `PIN_INSTRUMENT`, ...)
  - Windows: `is_debugger_present`, `check_remote_debugger`
- **Integrity fanout.** `LicenseGuard::check_layer()` and
  `check_service()` now re-run `self_check()` on every call.
  A partial patch that silences `activate()` no longer opens the
  gate for Pro/Enterprise-only operations.
- **Process hardening at startup.** `phantom_license::integrity::
  harden_process()` runs from `phantom-cli` and `phantom-svc`
  `main()`. On Linux: `prctl(PR_SET_DUMPABLE, 0)` — no core dumps
  (closes the "crash the tool, grep the dump for the master key"
  attack) and blocks foreign-UID ptrace. No-op on Windows for now.
- `phantom self-check` output (text + JSON) now names the detectors
  that fired, not just a boolean.

### Added
- `PHANTOM_DISABLE_INTEGRITY` env var: bypasses the detector ensemble
  for operator debugging. Documented but not advertised — production
  users have no reason to set it.

### Changed — Sprint 17: config MAC + ptrace lockdown + enrollment helper
- **Config file is HMAC-signed.** `config.json` gained a `config_mac`
  field covering the canonical serialization (with `config_mac`
  cleared). Editing `data_dir` (redirecting license loading to a
  writable path) or `license_key` (planting a foreign key) fails the
  MAC and the file is dropped in favor of defaults + env. Legacy
  files without the field load once for migration and are re-signed
  on next save. Verified end-to-end: hand-edited `pipe_name`
  reverts to the default in resolved output.
- **`prctl(PR_SET_PTRACER, 0)` on Linux.** Combined with the
  existing `PR_SET_DUMPABLE=0`, this revokes the Yama LSM's same-UID
  ptrace exemption — `gdb -p <pid>` now fails immediately from the
  same user, not just from foreign UIDs.
- **`PHANTOM_DISABLE_INTEGRITY` now also skips hardening**, so
  operators debugging locally can still attach a debugger.

### Added
- `phantom license request [--tier free|pro|enterprise]` prints a
  self-contained enrollment block (fingerprint, requested tier,
  current tier, platform, build info, master key generation) that the
  licensing team turns into a machine-bound key. Human-readable text
  or `--json` `LicenseRequestPayload`.
- `phantom_license::state_mac_hex()` and `verify_state_mac_hex()`
  public helpers for CLI-side tamper seals under the STATE_PURPOSE
  subkey.

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
