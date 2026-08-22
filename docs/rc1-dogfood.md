# v1.0.0-rc1 dogfood checklist

The end-to-end integration rehearsal for v1.0.0. Executed by one
engineer against a fresh Windows VM, using the same downloads a
paying customer would use. Every step is copyable; every checkbox
must pass green before the tag flips from `v1.0.0-rc1` to `v1.0.0`.

Not a code checklist — this exercises release artifacts that were
already built by CI, endpoints already deployed, license issued
against the production seed. If anything below requires code
changes, file a follow-up and bump to `-rc2`.

## Prerequisites

- Fresh Windows 10 22H2 or Windows 11 23H2 VM, snapshotted before
  Phantom touches anything.
- Administrator account.
- A test customer id and fingerprint you can burn (not a real
  customer's).
- Access to the vendor seat: `phantom-vendor` binary built against
  the production master seed, `wrangler` logged in.
- Access to the endpoints deployment (Cloudflare dashboard for
  logs).
- The GitHub release page for `v1.0.0-rc1` open in a browser.

## Section 1 — Download and verify

- [ ] Open the [v1.0.0-rc1 release page](https://github.com/HarperZ9/phantom/releases/tag/v1.0.0-rc1).
- [ ] Download `PhantomSetup-v1.0.0-rc1.msi` and `SHA256SUMS.txt`.
- [ ] Compare hashes per [`signature-verification.md`](signature-verification.md).
      Every hash matches.
- [ ] `signtool verify /pa /v PhantomSetup-v1.0.0-rc1.msi` reports
      `Successfully verified` and names the Phantom vendor.

If Sprint 23 (cert) has not yet delivered and the artifact is
unsigned, note it here and skip the signtool line. Every SmartScreen
"Windows protected your PC" warning in the sections below is expected.

## Section 2 — Install (fresh)

- [ ] Double-click MSI. UAC accepted.
- [ ] SmartScreen: publisher line reads Phantom vendor. Run anyway.
- [ ] EULA renders. Accept. Install completes < 30 sec.
- [ ] `C:\Program Files\Phantom\` contains phantom.exe, phantom-svc.exe,
      phantom-tray.exe.
- [ ] `sc query PhantomService` → STATE 4 RUNNING, START_TYPE 2 AUTO_START.
- [ ] Reg query `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run` →
      `PhantomTray` points at the installed tray exe.
- [ ] Log out, log back in. Tray icon visible.
- [ ] `phantom --version` → `phantom 1.0.0` (the `-rc1` suffix lives on
      the git tag and artifact filenames, not in the binary's version
      string; it reads `1.0.0` for both rc1 and the final tag).
- [ ] `phantom self-check --json` → `master_key_generation: 2`
      (production seed, not DEV placeholder). This is a Sev-1 gate:
      if it reports 1, the build did not consume PHANTOM_MASTER_SEED.

## Section 3 — Request a license

- [ ] `phantom license request` prints an enrollment JSON blob.
- [ ] Copy the blob out of the VM (paste into an email to yourself,
      or through the VM host clipboard).
- [ ] Note: `fingerprint_hex` and `master_key_generation: 2`.

## Section 4 — Issue the license (vendor seat)

On the vendor workstation, not on the VM:

- [ ] `phantom-vendor issue --tier pro --fingerprint <from step 3> \
      --expires-days 30` prints a key + serial.
- [ ] `wrangler d1 execute phantom-licenses --command "INSERT INTO
      licenses (serial, customer_id, tier, fingerprint_hex,
      issued_epoch_days, expires_epoch_days, issued_at, last_seen_at,
      revoked_at, note) VALUES ('<serial>','dogfood-rc1','Pro',
      '<fp>', $(( $(date +%s) / 86400 )), <expires_days>,
      $(date +%s), NULL, NULL, 'v1.0.0-rc1 dogfood');"` succeeds.
- [ ] Verify with `wrangler d1 execute phantom-licenses --command
      "SELECT * FROM licenses WHERE serial = '<serial>';"`.
- [ ] Send the key to yourself (same channel as step 3, reversed).

## Section 5 — Activate

Back on the VM:

- [ ] `phantom license activate <key>` prompts for ToU + privacy
      notice.
- [ ] Type `y` at each prompt (the ToU prompt defaults to No, so an
      empty line declines). For an unattended run, pass
      `--accept-tou --acknowledge-privacy-notice` instead of answering.
- [ ] `phantom license status` → tier Pro, correct serial, expiry
      matches what you issued.

## Section 6 — Generate + apply a profile

- [ ] `phantom audit` — capture the current MachineGuid in a note.
      This is the baseline.
- [ ] `phantom profile generate dogfood-rc1`.
- [ ] `phantom apply dogfood-rc1 --layers 2` — completes, prints the
      list of registry paths it wrote.
- [ ] `phantom validate` — all keys green.
- [ ] `reg query "HKLM\SOFTWARE\Microsoft\Cryptography" /v MachineGuid`
      shows the spoofed value, not the baseline.

## Section 7 — Reboot persistence

- [ ] Reboot the VM.
- [ ] After login: `sc query PhantomService` → RUNNING.
- [ ] `phantom status` → dogfood-rc1 still active.
- [ ] MachineGuid still the spoofed value, not the baseline.

## Section 8 — Revert

- [ ] `phantom revert`.
- [ ] `reg query "HKLM\SOFTWARE\Microsoft\Cryptography" /v MachineGuid`
      → baseline value from Section 6.
- [ ] `phantom status` → Unprotected.

## Section 9 — Phone-home

Phone-home is operator-configured and opt-in. First point the install
at the endpoint, then trigger a call. There is no `--force-phone-home`
flag; the call fires on any `phantom` invocation once it is due.

- [ ] `phantom config set phone_home_url <endpoint>/license/callback`.
- [ ] Make a call due now (instead of waiting 24h): either
      `phantom config set phone_home_interval_secs 0`, or delete
      `%ProgramData%\Phantom\.phone_home_last`.
- [ ] Run any command, e.g. `phantom license status`. The process
      lingers briefly at the end while the call completes.
- [ ] On the vendor seat: `wrangler d1 execute phantom-licenses
      --command "SELECT serial, last_seen_at FROM licenses WHERE
      serial = '<serial>';"` — `last_seen_at` is now populated.
- [ ] `wrangler tail` shows `POST /license/callback 200` with no
      `proof_invalid` or `unknown_serial` in the log.

## Section 10 — Phone-home opt-out

- [ ] `phantom config set phone_home_enabled false`.
- [ ] Make a call due again (as in Section 9) and run a command. No
      new `last_seen_at` update in D1 — the tool respected the setting.
- [ ] `phantom config set phone_home_enabled true` restores default.

## Section 11 — Revocation

On the vendor seat:

- [ ] `wrangler d1 execute phantom-licenses --command "UPDATE
      licenses SET revoked_at = $(date +%s),
      note = 'dogfood test revoke' WHERE serial = '<serial>';"`.
- [ ] Confirm update.

Back on the VM:

- [ ] Make a call due (as in Section 9) and run `phantom license
      status`. The phone-home returns revoked and records a
      High-severity tripwire.
- [ ] The NEXT `phantom` invocation reports Tier: Free (the downgrade
      applies on the load after the revocation is recorded).
- [ ] `phantom apply <profile> --layers 2` still succeeds — Free tier
      keeps Layer 2 (registry) spoofing.
- [ ] `phantom apply <profile> --layers 1` (or `0`) refuses:
      "license tier does not permit this operation. Upgrade to Pro or
      Enterprise for Layer 0/1 access." Revocation costs the Pro-only
      layers and the higher profile limit, not Layer 2.

## Section 12 — Uninstall

- [ ] Apply a fresh profile before uninstalling so we test the
      cleanup path: `phantom profile generate uninst-test` then
      `phantom apply uninst-test --layers 2`.
- [ ] Uninstall via Settings → Apps.
- [ ] After uninstall: `reg query "HKLM\SOFTWARE\Microsoft\
      Cryptography" /v MachineGuid` → baseline value.
      **Sev-1 gate**: if MachineGuid is still the spoofed
      `uninst-test` value, the pre-uninstall CleanupAction failed
      and this rc is not shippable.
- [ ] `sc query PhantomService` → service missing.
- [ ] `C:\Program Files\Phantom\` gone.

## Sign-off

- [ ] All 12 sections passed. Any Sev-1 or Sev-2 defect is filed and
      either fixed for `-rc2` or explicitly deferred with a rationale.
- [ ] Tag `v1.0.0` at the same commit as `v1.0.0-rc1` if nothing
      changed, or at the head of the fix commits if it did.
- [ ] `docs/user/*.md` reads correctly against the shipped tool
      (spot-check: command names, output formats, paths).
- [ ] Release notes drafted (see `CHANGELOG.md` `[Unreleased]` →
      promote to `[1.0.0]`).

## Bug filing template

For each failure, open an issue with:

- Section number + checkbox
- Windows version + build
- Phantom version (`phantom --version`)
- What you ran, what you expected, what you got (paste both, don't
  paraphrase)
- Sev level: 1 (blocks GA), 2 (must fix within a week of GA), 3
  (known-issues doc)
- Whether reverting the VM to the pre-Phantom snapshot recovered
  the baseline

## Related

- Product QA of the MSI proper: `msi-install-runbook.md`
  (overlap intentional — that runbook is per-artifact QA, this one
  is customer-flow rehearsal).
- Layer 2 end-to-end alone: `windows-runbook.md`.
- Endpoint side of steps 4, 9, 11: `issuance-workflow.md`.
