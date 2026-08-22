# Contributing to Phantom

Thanks for your interest. Phantom is proprietary software; contributions
are accepted under a Contributor License Agreement. This file covers the
technical standards contributors are held to.

## Ground rules

- **Zero external dependencies unless there is no alternative.** Every
  new dependency is a supply-chain risk and a compile-time tax; if the
  standard library or an existing crate in the tree already does it,
  use that.
- **No behavior change disguised as a refactor.** Refactors that alter
  semantics are two commits (or two PRs), never one.
- **Frozen invariants get pinned assertions.** The license signing key,
  the IPC magic bytes, the SMBIOS layout offsets, anything that a
  future refactor would silently break gets a test that reads like a
  contract, not like an implementation check.

## Layout

Workspace crates:

| Crate               | Role |
|---------------------|------|
| `phantom-cli`       | User-facing CLI plus the profile engine and validator |
| `phantom-ipc`       | Named-pipe protocol (message types, wire format, client/server) |
| `phantom-svc`       | Background service (Windows service or standalone) |
| `phantom-license`   | Licensing, machine fingerprinting, integrity checks |
| `phantom-tray`      | System tray UI (Windows) |
| `phantom-driver`    | C kernel filter driver (Layer 1) |
| `phantom-dxe`       | C UEFI DXE application (Layer 0) |
| `phantom-installer` | WiX MSI installer definition |

## Building and testing

```bash
# Full build
cargo build --workspace

# Full test suite (should be 100% green before you push)
cargo test --workspace

# Format and lint
cargo fmt --all
cargo clippy --workspace --all-targets

# One CLI end-to-end sanity check
PHANTOM_DATA_DIR=/tmp/phantom-dev cargo run -p phantom-cli -- --json status
```

CI runs fmt (check-only), clippy, workspace build and test on Linux and
Windows, a release build on both, and `cargo audit`. The audit step is
allowed to fail so it does not block on transient advisories, but every
real finding gets triaged in the PR.

## Tests that touch process-global state

Env vars, the license file on disk, the config file, any test that
mutates something the whole process shares must acquire the shared
mutex from `phantom_cli::profile::env_test_mutex()` before touching it
and remove/restore what it changed at the end. Parallel test threads
otherwise race on the process env table.

## Adding a new command

1. Extend the `Commands` enum in `phantom-cli/src/main.rs`.
2. Add the handler in the `match` block; support `--json` from day one
   by emitting an `Envelope` with a typed payload declared in
   `json_out.rs`.
3. Add tests for the pure logic in the module where it lives. The
   `main.rs` handler is a thin shell, the tests belong in the module.
4. Update the README's usage section and the CHANGELOG's
   `[Unreleased]` block in the same commit.

## Commit style

Prefix commits with the sprint or area, keep the summary under 72
chars, and describe the *why* in the body:

```
Sprint 11: add adversarial license-key fuzz test

Every single-bit flip of a valid key is now exercised in the license
test suite (480 cases per run). Guarantees the HMAC covers every byte
and rules out a silent parse path that would accept a near-miss.
```

Never mention model identifiers or specific AI tooling in commits,
titles, or code comments.

## PR expectations

- CI green on both Linux and Windows
- New user-facing behavior is documented in the README
- The CHANGELOG is updated in the same commit that introduces the
  change
- The diff answers "what did I trust the reviewer to catch", bugs
  they would have to run the code to find go into the PR description
  as risks, not as surprises

## Reporting a vulnerability

Do not open a public issue. Report it privately through GitHub: the
repository's **Security** tab, **Report a vulnerability**. See
[SECURITY.md](SECURITY.md).
