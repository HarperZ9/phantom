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

### Changed — Sprint 18: signed IPC + log/panic redaction
- **Every phantom-ipc message is HMAC-signed.** Wire format bumped
  to protocol v2: `[u32 LE total_len] [32-byte HMAC-SHA256]
  [JSON body]`. Signing uses the STATE_PURPOSE subkey. An attacker
  with SYSTEM or root can still take the pipe endpoint, but can no
  longer forge a payload without the master key. A v1 peer talking
  to a v2 peer sees MAC verification fail and disconnects.
- **Log-line and panic-message redaction.** `phantom_license::redact`
  scrubs three shapes that only appear in output when a secret is
  being logged: 19+ dashed base32 groups (license keys), 64-char
  lowercase hex (HMAC state / origin marks / attempt log entries),
  and 32-char lowercase hex (fingerprints). Regex-free single-pass
  scanner. Replaced with fixed placeholders that preserve log-grep.
- **Panic hook installed at startup** in both `phantom-cli` and
  `phantom-svc` `main()`. Redacted panic messages hit stderr and
  crash reporters; the original hook still runs so backtrace
  collectors keep working.

### Added
- `phantom_ipc::PROTOCOL_VERSION` bumped from 1 to 2 (breaking).
- 5 new signed-protocol tests: shorter-than-MAC rejection, payload
  bit-flip rejection, MAC byte-flip rejection, unsigned-frame
  rejection, per-payload determinism.
- 8 new redact tests: plaintext untouched, short hex untouched,
  fingerprint / MAC / license-key scrubbing, short dashed UUID not
  matched, multi-secret line, panic-hook idempotency.

### Changed — Sprint 19: tripwire + honey license keys
- **Anti-tamper tripwire log** at `<data_dir>/.tripwire`. Every entry
  is HMAC-signed under STATE_PURPOSE — an attacker cannot silently
  trim or edit the log without failing MAC verification. Two
  severities:
  - **Low** (LD_PRELOAD, tracer_pid, debugger_env): recorded for
    operator visibility via `phantom tamper-report`; does not lock
    the install. Legitimate profilers trigger these, so acting on
    them would misfire.
  - **High** (state MAC failure, honey-key attempt, clock rewind):
    `LicenseGuard::load()` silently returns Free tier from that
    point on. Cracked installs become functionally-Free installs
    with no visible error. Successful `license activate` with a
    real key clears the tripwire.
- **Honey license keys.** Eight well-formed-looking but never-issued
  strings baked into the binary as bait. Any attempt to activate one
  records a High-severity trip. The failure returned is the same
  `InvalidSignature` as any other bad key — no distinguishing signal.
- **`phantom tamper-report [--clear]`** subcommand: reads the local
  log, optionally clears it, and prints it in text or `--json`. The
  log **never leaves this machine over the network.** The command
  makes that guarantee visible in the output.
- Detector ensemble triggers now also land a Low-severity tripwire
  event via `integrity::full_self_check_with_log()`.

### Design boundaries (deliberate non-goals)
- **Nothing outside `<data_dir>` is ever touched.** No writes to
  user home directories, browser data, other applications, system
  files, or the OS. The tool serves users who chose it for
  anonymity — the anti-tamper posture cannot itself become a way
  to de-anonymize them.
- **Nothing is transmitted over the network.** Ever. Not even
  aggregate counters, not even hashed telemetry. If an operator
  wants to share a tamper report with support, they run
  `phantom tamper-report` themselves and paste the output.
- **No damage to the reverser's machine.** No shellcode payload,
  no privilege-escalation attempt, no destruction of unrelated
  files, no bricking, no rootkits. The cracked install becomes
  Free tier — that is the full extent.

### Added
- `phantom_license::tripwire` module (8 tests: empty/low/high state
  transitions, forgery rejection, dedup, honey-key normalization,
  arbitrary-string negative).

### Added — Sprint 20: license phone-home + investigator verifier
- **`phantom_license::phone_home`** module implementing a fully
  opt-in license callback. No baked-in vendor URL — the operator
  (individual or enterprise) sets `phone_home_url` in the config;
  unset means no calls ever. Payload contains only: opaque license
  serial (HMAC hash, not the key), tier, phantom version, unix
  seconds, tripwire counts, and a **proof-of-possession signature**
  (HMAC-SHA256 keyed by the license key itself over the canonical
  payload). No machine fingerprint, no IP-linkable data, no profile
  content. Transport is `curl` via subprocess so the URL and body
  are auditable in `ps` / audit logs and no HTTPS crate joins the
  supply chain. Rate-limited to at most once per 24h via a signed
  last-call file; fail-open on network errors; fail-closed only on
  explicit `{"revoked": true}` response (which High-trips the
  tripwire).
- **`phantom-verify` binary** (new workspace member) for
  vendor-side investigation of Phantom-generated evidence:
  - `inspect <profile.json>` — dumps origin_mark fields (no
    crypto).
  - `verify <profile.json>` — verifies the HMAC on the mark,
    reports `VALID` / `INVALID` / `UNMARKED` / `TAMPERED` /
    `MALFORMED`. Exit code reflects verdict.
  - `match <profile.json> --key <license-key>` — the primary
    investigation flow. Given a suspect profile and a candidate
    license key, reports whether that key produced this profile.
    Uses the machine hash embedded in the license (signed by the
    master, unforgeable) as the tie-back to the mark's
    `origin_fingerprint_hex`.
  - `serial --key <license-key>` — compute the opaque phone-home
    serial that a given license would present. Correlates a
    phone-home log entry back to a customer record.
  Reads from a file path or `-` (stdin). Text output by default,
  `--json` envelope for automation.

### Design boundaries (Sprint 20 additions)
- **Phone-home leaks no fingerprint.** Only an opaque HMAC serial
  identifies the install to the endpoint. The endpoint must have
  been previously issued this serial (via the license request
  flow) to know which customer it maps to.
- **`phantom-verify` is vendor-internal.** It carries the same
  obfuscated master key as `phantom-cli`, so anyone who has the
  binary can, in principle, extract the master key and forge marks.
  Distribution should be limited to the vendor and specific
  auditors under NDA. A follow-up sprint will migrate origin marks
  from HMAC-SHA256 to Ed25519 signatures; at that point
  `phantom-verify` can ship publicly with only the public key.
- **No auto-reporting on tamper detection.** Tamper events remain
  local to `<data_dir>/.tripwire`; the phone-home payload includes
  only a count. The user must run `phantom tamper-report` themselves
  to share detail.

### Tests
- 11 new `phone_home` cases: payload purity, deterministic opaque
  serial, no-URL no-call, is-due interval, tampered last-call
  falls back to due, URL stored only as hash, forged log dropped,
  proof roundtrips, empty-proof rejection, field-tamper breaks
  proof, per-time freshness.
- 6 new `phantom-verify` integration tests: inspect prints fields,
  verify prints VALID on marked, TAMPERED on hand-edited content,
  JSON envelope validates, stdin path works, serial is
  deterministic and distinct per key.
- Total workspace: **190 tests** passing (up from 173).

### Changed — Sprint 21: disclosed opt-out telemetry + ToU + response tooling
- **First-run acknowledgment flow.** `phantom license activate` now
  requires acceptance of the Terms of Use AND the Privacy Notice
  before it touches key material. Interactive TTYs are prompted;
  headless installers must pass `--accept-tou` and
  `--acknowledge-privacy-notice`. Rejection or missing flags aborts
  activation with a clear message pointing at `phantom tou` and
  `phantom privacy-notice`.
- **Pinned legal text in `phantom_license::legal`.** Full Privacy
  Notice and Terms of Use as compiled-in constants with monotonic
  version integers (`PRIVACY_NOTICE_VERSION`, `TOU_VERSION`). A
  version bump forces re-acknowledgment on the next `activate`;
  stale acknowledgments do NOT satisfy `phone_home_active()`.
  Tests pin the version numbers and pin specific enforceable
  claims (disable-command mention, no-fingerprint promise,
  revocation + appeal clauses).
- **Opt-out phone-home semantics — with mandatory disclosure.**
  Once the notice is acknowledged, `phone_home_enabled` defaults to
  true and calls fire on the 24h interval. Users disable at any
  time with `phantom config set phone_home_enabled false`. Explicit
  `phone_home_enabled: false` in config beats any acknowledgment.
- **Compile-time default endpoint via `PHANTOM_DEFAULT_PHONE_HOME_URL`
  build env var.** Vendor release builds set the env var at
  `cargo build --release` time; the resulting binary bakes in the
  endpoint and populates `phone_home_url` when the operator
  acknowledges the notice. Dev builds ship without a baked
  endpoint. Operator can always override in the config file.
- **Opportunistic non-blocking call at CLI startup.** Every
  `phantom …` invocation calls `maybe_spawn_phone_home()` at
  `main()`, which consults the config, checks if the call is due,
  and detaches a thread to make the call via `curl`. The user's
  command proceeds without waiting; a `{"revoked": true}` response
  from a prior call trips the tripwire and the next
  `LicenseGuard::load()` downgrades to Free.

### Added
- `phantom privacy-notice` — displays the current Privacy Notice
  and this install's acknowledgment status (accepted version,
  timestamp, whether phone-home is currently active).
- `phantom tou` — same for the Terms of Use.
- `PhantomConfig` fields: `phone_home_url`, `phone_home_enabled`,
  `phone_home_interval_secs`, `privacy_notice_acknowledged_at`,
  `privacy_notice_version_accepted`, `tou_accepted_at`,
  `tou_version_accepted`. All covered by the existing config MAC
  so a hand-edited acknowledgment fails the seal.
- `PhantomConfig::phone_home_active()` — the single gate the
  runtime consults.
- 4 new config tests: acknowledgment-gating, active-after-ack,
  explicit-disable-beats-ack, stale-version-requires-reack.
- Total workspace: **198 tests** passing (up from 190).

### Design principles held
- **Disclosure precedes any call.** The Privacy Notice and ToU are
  shown before activation completes; phone-home cannot fire until
  they've been acknowledged for the current version.
- **Auditability.** `phantom privacy-notice` and `phantom tou`
  reprint the shipping text at any time. Every phone-home call is
  recorded to the signed local log. The endpoint URL is visible in
  `ps` because transport is `curl`.
- **User controls.** `phantom config set phone_home_enabled false`
  disables. No env var required. The change is sealed into the
  MAC'd config so a downstream tamperer cannot silently re-enable.
- **No hidden collection.** The Privacy Notice text lists exactly
  what is sent, and the `phone_home::PhoneHomePayload` struct
  matches that list byte-for-byte. Adding a field to the payload
  without updating the notice text and version fails the pinned
  claim tests.

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
