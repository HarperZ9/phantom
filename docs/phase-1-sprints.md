# Phase 1 — Launch Readiness Sprints

Six sprints to move Phantom from "codebase that compiles" to "product
that can be sold." Two are external-blocked (procurement); the rest
are engineering. Parallelize where the dependency graph allows.

**Assumed team**: 1–2 engineers full-time, plus part-time external
work (cert procurement, legal review). Target: **v1.0.0 GA in
6–10 weeks** from Sprint 22 kickoff, Layer 0/1 deferred.

---

## Dependency graph

```
Sprint 22 ─┐         Sprint 24 ── Sprint 25 ─┐
           │                                  ├─── Sprint 27 ── v1.0.0
Sprint 23 ─┤─────── Sprint 26 ────────────────┘
(cert lead │
 time ~2wk)│
           └────────── (parallelizable)
```

Sprint 22 (Windows validation) and Sprint 23 (cert procurement) can
start **Day 1 in parallel**. Sprint 24 (master seed) can also start
Day 1 — it's a config change, not blocked on anything. Sprint 25
(endpoints) can start once 24 lands. Sprint 26 (MSI) needs 22 done
and 23 delivered. Sprint 27 is the integration rehearsal.

---

## Sprint 22 — Windows CI green + Layer-2 end-to-end on real hardware

**Duration**: 1 week (or 2 if physical Windows setup is greenfield).

**Goal**: The primary target platform actually works. Every CI job
passes on Windows, and the flagship Layer-2 registry spoofing has
been executed end-to-end on a real Windows machine.

### Scope

- Land the remaining Windows compile/link fixes (in flight from
  commits `f7201d3` and `1ff3b8b`).
- Provision a Windows 10 and a Windows 11 VM (Hyper-V, VMware, or
  cloud). Snapshot before any Phantom activity so the runbook can
  reset between runs.
- Author `docs/windows-runbook.md`: minimum steps a developer runs
  to validate a build on a fresh Windows image.
- Run the runbook end-to-end against `main`. Fix anything that
  breaks. Record what actually happens per step.

### Deliverables

- Green CI on `main` for `test (windows-latest)`, `release build
  (windows-latest)`.
- `docs/windows-runbook.md` covering: install prerequisites, build
  from source, execute `phantom audit`, `phantom profile generate`,
  `phantom apply my-profile --layers 2`, verify registry changed,
  `phantom validate`, `phantom revert`, verify registry restored.
- A short integration-test story in `phantom-cli/tests/` that at
  minimum exercises the audit + apply + revert path on Windows (may
  need `#[cfg(windows)]` gating).

### Acceptance

- [ ] All CI jobs green on `main`
- [ ] Runbook executable by a second engineer on a clean VM without
      undocumented steps
- [ ] `phantom apply` changes `HKLM\SOFTWARE\Microsoft\Cryptography
      \MachineGuid` to the profile's value
- [ ] `phantom revert` restores the original value byte-for-byte
- [ ] Registry backup file survives a reboot
- [ ] Uninstall (manual for now) leaves no orphan values

### Risks

- Registry ACLs may block writes without elevation → runbook needs
  UAC / "Run as administrator" step.
- Some SMBIOS registry paths differ across OEMs → collect divergence
  during runbook execution, file follow-up.

---

## Sprint 23 — Release signing infrastructure (procurement + wiring)

**Duration**: 2–4 weeks wall-clock; ~1 week engineering.

**Goal**: Every release binary and MSI Phantom ships is signed with a
production cert, verifiable by end users, and gradually earning
SmartScreen reputation.

### Scope

- Select cert vendor. Recommendation: **Sectigo EV Code Signing**
  (\~\$300/yr, hardware token or cloud HSM). DigiCert also viable.
- File corporate paperwork (D-U-N-S, articles of incorporation if
  not already). This is the long-pole item.
- Choose signing approach:
  - **In-cloud HSM signing** (Sectigo cloud, DigiCert KeyLocker) —
    signs from CI without shipping the key. Preferred.
  - **Hardware token** — cert on a USB dongle, requires a self-
    hosted runner or a signing shim on a physical box. Cheaper but
    operationally heavier.
- Add signing step to `.github/workflows/release.yml` on Windows
  jobs. Sign `phantom-cli.exe`, `phantom-svc.exe`, `phantom-tray.exe`
  and the MSI (once Sprint 26 produces one).
- Publish SHA-256 checksums file (signed via cosign or GPG) alongside
  each release so users on non-Windows platforms can verify.
- Add a `docs/signature-verification.md` page with copy-pasteable
  `signtool verify /pa` and `sha256sum -c` commands.

### Deliverables

- EV cert issued and stored in cloud HSM (or on token in a locked
  drawer, with runbook).
- CI signing step working; nightly build produces a signed test
  binary.
- Published verification instructions.
- SHA256SUMS.txt published per release, GPG-signed by a maintainer
  key.

### Acceptance

- [ ] Downloaded MSI passes `signtool verify /pa`
- [ ] Downloaded binaries pass `signtool verify /pa /v`
- [ ] `sha256sum -c SHA256SUMS.txt` succeeds against downloaded files
- [ ] After ~1–2 weeks of downloads, SmartScreen "unrecognized
      publisher" warning stops appearing on fresh Windows installs
      (reputation-based; will improve over time)

### Risks

- EV vetting can take 2–4 weeks. Start Day 1 of Phase 1.
- Corporate entity not yet formed → cannot receive cert. Legal setup
  is a hidden dependency.
- Kernel driver signing (WHQL) is a separate, much longer process —
  explicitly excluded from Phase 1 per Layer-1 deferral.

---

## Sprint 24 — Master seed rotation, CI-driven

**Duration**: 3–5 days.

**Goal**: The obfuscated master key baked into every release binary
is unique to this vendor, sourced from a CI-managed secret, and
never appears in the repo. The current placeholder seed in
`phantom-license/build.rs` is decommissioned.

### Scope

- Generate a fresh 32-byte seed (`openssl rand -hex 32`). Store as
  GitHub Actions organization secret `PHANTOM_MASTER_SEED`.
- Rewrite `phantom-license/build.rs`:
  - Read seed from `PHANTOM_MASTER_SEED` env var at build time.
  - If unset AND `--release` mode → **fail the build with a clear
    error** ("release builds require PHANTOM_MASTER_SEED"). This is
    the enforcement mechanism.
  - If unset AND `--debug` → fall back to the current placeholder
    with a compiler warning ("dev build, do not distribute").
- Bump `MASTER_KEY_GEN` in the generated file from 1 to 2.
- Update `phantom-license/src/key.rs` — the `derived_signing_key_is_
  pinned` test currently pins the SHA-256 of the derived subkey for
  the placeholder seed; move that pin to a `#[cfg(not(release))]`
  test, add a separate release-build pin that's set once the real
  seed is baked.
- Update `phantom-verify` to consume the same secret at build time
  so the vendor's investigator tool matches the customer-issued keys.
- Runbook: how to rotate the seed. What breaks (every existing
  license becomes invalid) and what doesn't (customer profiles keep
  loading with `Unmarked` verdict). Document that this is a **one-
  time-only, pre-launch rotation** — once real customers hold keys,
  further rotation is a breaking change.

### Deliverables

- `PHANTOM_MASTER_SEED` secret in GitHub Actions
- `phantom-license/build.rs` fails release builds without the secret
- `MASTER_KEY_GEN` bumped to 2 and reflected in `phantom self-check`
- `docs/master-seed-rotation.md` runbook
- CHANGELOG entry documenting the seed rotation

### Acceptance

- [ ] `cargo build --release` on a fresh checkout without the secret
      fails with a clear error message
- [ ] `cargo build --release` with the secret succeeds
- [ ] `cargo build` (dev) still succeeds without the secret
- [ ] `phantom self-check --json` reports `"master_key_generation":
      2` on a release-built binary
- [ ] Test `derived_signing_key_is_pinned` still passes for dev
      builds; a separate release pin exists

### Risks

- Once the secret exists, protecting it becomes an ops
  responsibility. Suggest: only 2–3 humans have access, rotation
  is a public-signal event (like a CA key rotation).
- If the secret leaks, every issued license is forgeable. Treat as a
  Sev-1.

---

## Sprint 25 — License issuance + phone-home endpoints

**Duration**: 2 weeks.

**Goal**: Two minimal HTTP endpoints on Cloudflare Workers (or
equivalent) that close the licensing loop end-to-end. Not a full
customer portal — the smallest thing that makes the tool sellable.

### Scope

**Platform**: Cloudflare Workers + D1 (SQLite at the edge) + KV for
rate limits. Alternatives: Fly.io + Postgres, AWS Lambda + DynamoDB.
Pick one, document why.

**Endpoint 1: `POST /license/request`**

Not user-facing. Internal admin tool the licensing team runs after
receiving a customer payment.

- Input: enrollment JSON produced by `phantom license request`
  (fingerprint, requested tier, platform, master key gen, build).
- Server-side: verify the request signature, look up the customer's
  paid tier, call the vendor-side `generate_license_key` (same
  crypto as `phantom-license/src/key.rs`) with the correct tier and
  expiration, store `(serial, customer_id, tier, issued_at,
  fingerprint)` in D1.
- Output: the license key, plus the serial for the customer record.

**Endpoint 2: `POST /license/callback`**

The endpoint `phantom` phone-home calls into.

- Input: `PhoneHomePayload` JSON.
- Server-side: look up `payload.license_serial` in D1. If not found →
  return `{"revoked": true}` (it's a fake serial or a revoked one).
  If found → verify `payload.proof` using the stored license key.
  Proof invalid → `{"revoked": true}` (someone forged a serial).
  Proof valid + row not marked revoked → `{"revoked": false, "ok":
  true}`. Update `last_seen_at` in D1.
- Rate limit: 10 calls per hour per serial via KV.

**Vendor-side admin tool** (CLI, not a UI):

- `admin issue --customer-id X --tier pro --expires 2027-01-01` →
  reads the customer's enrollment from a queue or file, calls the
  issuance codepath, prints the key to hand off.
- `admin revoke --serial <8-hex>` → marks the row revoked. Next
  phone-home returns `{"revoked": true}` and the install downgrades
  to Free (Sprint 19 tripwire path).

### Deliverables

- Cloudflare Worker (or chosen platform) with the two endpoints
- D1 schema: `licenses(serial PRIMARY KEY, customer_id, tier,
  issued_at, expires_at, fingerprint_hex, revoked_at NULLABLE,
  last_seen_at NULLABLE)`
- Admin CLI in a new `phantom-vendor-tools` (private repo)
- Deployment via `wrangler deploy` or equivalent; production URL
  in a Cloudflare-owned custom domain (e.g. `api.phantom.dev`)
- `docs/api.md` documenting the request/response schemas
- `docs/issuance-workflow.md` for the internal team

### Acceptance

- [ ] End-to-end: `phantom license request --tier pro` → admin
      issues key → `phantom license activate <key>` succeeds
- [ ] `phantom` on a clean install calls `/license/callback`;
      endpoint returns `{"revoked": false}`; `phantom-verify serial
      --key` produces the same serial the endpoint saw
- [ ] Admin `revoke` → next phone-home returns revoked → next
      `phantom` invocation reports Free tier
- [ ] Rate limit fires at the 11th call in an hour
- [ ] Endpoint 200s within 300ms p95 from a US-East client

### Risks

- Cloudflare D1 is beta; if it churns, migration effort. Fallback:
  Postgres on Fly.
- Vendor-side `generate_license_key` needs the master seed. Store it
  as a Cloudflare Worker secret. **Never** commit a copy anywhere.

---

## Sprint 26 — Windows MSI installer + install/uninstall validation

**Duration**: 2 weeks.

**Goal**: A signed MSI that installs Phantom cleanly on Windows 10
and 11, registers the service, sets up the tray, and uninstalls
without leaving orphans.

**Prereqs**: Sprint 22 (Windows CI + Layer-2 validated), Sprint 23
(cert available).

### Scope

- The `phantom-installer/phantom.wxs` WiX descriptor already exists.
  Add a CI job on Windows that:
  - Installs WiX Toolset v3
  - Runs `phantom-installer/build.cmd`
  - Signs the resulting MSI with the cert from Sprint 23
  - Uploads the signed MSI as a release artifact
- Test install on Windows 10 VM:
  - MSI installs to `%ProgramFiles%\Phantom` without UAC weirdness
  - `PhantomService` registered as auto-start with SYSTEM account
  - Tray app installed for login autostart
  - Registry writable by service account
- Test install on Windows 11 VM: same coverage
- Test uninstall:
  - Service stopped and unregistered
  - Files removed
  - Any applied identity layers reverted (uninstall must call
    `phantom revert` before removing the CLI)
- Test upgrade (0.6.0 → 0.6.1):
  - License activation state preserved (config MAC survives
    binary swap)
  - Privacy notice / ToU acknowledgments preserved
  - Active profile preserved
- Add a `docs/msi-install-runbook.md` for QA

### Deliverables

- CI job producing a signed `PhantomSetup-v1.0.0.msi`
- Documented install/uninstall/upgrade behavior
- Runbook for manual QA on both OS versions

### Acceptance

- [ ] MSI installs on fresh Win 10 VM in < 30 seconds, no errors
- [ ] Same on fresh Win 11 VM
- [ ] `sc query PhantomService` shows auto-start after install
- [ ] `phantom license activate <key>` works from a fresh install
- [ ] Uninstall via `Add/Remove Programs` completes cleanly, all
      identity spoofing reverted, no orphan registry values
- [ ] Upgrade preserves config, license, and acknowledgments
- [ ] Signed MSI: SmartScreen "run anyway" acceptable during
      reputation warmup

### Risks

- WiX v3 is EOL; migration to WiX v4 or WiX v5 may be needed
  eventually. Punt to post-v1.
- The Layer-1 driver install path in the WiX descriptor is a stub.
  Layer 1 is deferred; either remove the driver section from the
  MSI or leave it stubbed with a "coming soon" note in docs.

---

## Sprint 27 — v1.0.0-rc1 → v1.0.0 GA rehearsal

**Duration**: 1 week.

**Goal**: Cut a release candidate, dogfood the full customer flow
against production endpoints, fix what breaks, ship GA.

### Scope

- Tag `v1.0.0-rc1`
- Release CI produces signed archives + signed MSI
- Manual dogfood on a fresh VM:
  1. Download the MSI from the GitHub release page
  2. Verify signature manually
  3. Install
  4. Run `phantom license request --tier pro` → forward to admin
  5. Admin issues key via Sprint 25 endpoint
  6. `phantom license activate <key>` → accept ToU + notice
  7. Confirm `phantom license status` reports Pro
  8. `phantom profile generate demo`
  9. `phantom apply demo --layers 2`
  10. Reboot; confirm identity persists
  11. `phantom revert`
  12. Wait 24h; confirm phone-home fired (check endpoint log +
      local `phantom-svc` phone-home log)
  13. `phantom config set phone_home_enabled false` → confirm no
      calls after
  14. Admin `revoke` → confirm next `phantom` invocation drops to Free
  15. Uninstall → confirm clean
- File bugs from dogfood; fix or defer
- Tag `v1.0.0` when the flow is smooth end-to-end

### Deliverables

- `v1.0.0` release published with signed artifacts
- Public release notes
- One page each of user-facing docs: install, activate, first
  profile, uninstall
- Support inbox live: `support@<domain>`

### Acceptance

- [ ] Full flow from download to revoke works without operator
      intervention outside the documented steps
- [ ] No Sev-1 or Sev-2 bugs open at cut time
- [ ] Release page includes signed checksums + verification
      instructions

### Risks

- Every previous sprint has slipped by weeks; assume rc1 → GA
  takes a bug-fix cycle. Budget an extra week.

---

## What Phase 1 explicitly does NOT include

To keep scope real:

- **Layer 1 kernel driver.** WHQL attestation is 6–12 weeks with
  Microsoft. Ship v1 as Layer 2 only. Advertise Layer 1 as "v2 —
  in submission with Microsoft."
- **Layer 0 UEFI/DXE.** Niche (requires Secure Boot off), untested
  on physical hardware. Defer to v2/v3.
- **Ed25519 migration for origin marks.** Would let `phantom-verify`
  ship publicly. Nice-to-have; Phase 2.
- **Public marketing site.** That's the deliverable Phase 1 exists
  to *justify* — build it after v1.0.0 GA.
- **Payment integration.** For v1.0.0, licenses can be issued by
  emailing an invoice and having the admin CLI issue on paid receipt.
  Automate via Stripe in Phase 2.
- **AV vendor whitelisting.** Reputation-based; starts naturally
  once the signed MSI is in the wild.
- **Second-language SDKs / library form.** v1 is a CLI + service +
  tray. Anything else is Phase 3.

## Effort estimate

| Sprint | Wall-clock | Engineer-days | External |
|---|---|---|---|
| 22 | 1–2 wk | 5–10 | — |
| 23 | 2–4 wk | 3–5 | Cert vendor (2–4 wk lead) |
| 24 | 3–5 d | 3–5 | — |
| 25 | 2 wk | 8–12 | Cloudflare / DNS setup |
| 26 | 2 wk | 8–12 | — |
| 27 | 1 wk | 3–5 | — |
| **Total** | **6–10 wk** | **30–50** | Cert + legal review |

Assumes 22, 23, 24 run in parallel from Day 1; 25 starts after 24;
26 starts after 22 + 23 land; 27 gates on 25 + 26.

## What "shipped" looks like at Phase 1 exit

A downloadable, signed Windows MSI at `https://phantom.dev/download`,
paired with a functioning license issuance workflow, that installs a
tool a paying customer can activate and use to spoof Layer-2
hardware identity on their own machine. Not a marketing site yet —
that's Phase 2. Just: the product exists, works, and can take money.
