# Windows MSI install / upgrade / uninstall QA runbook

Manual QA for the Phantom Windows installer. Run against `PhantomSetup-v<version>.msi` on a **snapshotted VM** for each release candidate — never against a machine you rely on for anything else, because a bad revert during uninstall can persist an unwanted MachineGuid until the tester restores the snapshot.

Test matrix:

- Windows 10 (22H2), fresh install, x64
- Windows 11 (23H2 or newer), fresh install, x64

Every step below runs on both. Any divergence between the two OSes is a bug — file it, don't paper over it.

## Prerequisites

- VM with a clean Windows install, snapshotted **before** any Phantom test.
- Administrator account.
- The MSI from the release you're validating, downloaded from the GitHub release page the same way a real customer would.

v1.0.0 is not code-signed, so skip any Authenticode/signtool checks; every SmartScreen "Windows protected your PC" click is expected on an unsigned build. When a signing certificate lands, restore the signature-verification steps.

## Section 1 — Fresh install

Snapshot: `clean-vm` (nothing installed).

- [ ] Copy MSI to VM.
- [ ] Right-click → Properties. Confirm the **Digital Signatures** tab lists a valid signature by "Phantom" (or the vendor company name from the cert). If tab absent, this is an unsigned build; note it in the report.
- [ ] Double-click MSI. Accept the UAC prompt.
- [ ] SmartScreen dialog appears. On a fresh cert (< ~2 weeks of reputation) expect "Windows protected your PC" → click **More info** → **Run anyway**. On a warmed cert, expect direct install.
- [ ] EULA screen: text loads (from `eula.rtf`), accept checkbox works, Install button enables.
- [ ] Progress bar completes without error. Install duration should be < 30 seconds on a modern VM.
- [ ] Close installer.

Verify install landed:

- [ ] `C:\Program Files\Phantom\` exists and contains `phantom.exe`, `phantom-svc.exe`, `phantom-tray.exe` — and nothing else.
- [ ] `sc query PhantomService` reports `STATE : 4 RUNNING` and `START_TYPE : 2 AUTO_START`.
- [ ] `reg query "HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run" /v PhantomTray` shows a REG_SZ pointing at `C:\Program Files\Phantom\phantom-tray.exe`.
- [ ] Start Menu → Phantom → shortcut launches the tray.
- [ ] Log out and back in. Tray icon appears in the notification area (may be hidden under the ^ chevron — that's Windows default, not a bug).

Signature verification (only on signed builds):

```powershell
signtool verify /pa /v "C:\Program Files\Phantom\phantom-svc.exe"
signtool verify /pa /v "C:\Program Files\Phantom\phantom-cli.exe"
signtool verify /pa /v "C:\Program Files\Phantom\phantom-tray.exe"
```

Each must report `Successfully verified` and name the vendor as the signer.

## Section 2 — Basic operation

Runs from the fresh install above. This is not a full Layer-2 acceptance run (that's the Sprint 22 runbook, `docs/windows-runbook.md`) — just enough to confirm the installer wired everything correctly.

- [ ] `phantom.exe --version` prints the release version.
- [ ] `phantom.exe self-check --json` runs. `master_key_generation` in the output is `2` (real seed, not the DEV placeholder). If it's `1`, the MSI was built without the production seed and MUST NOT ship.
- [ ] `phantom.exe audit` runs and prints the machine's current identity.
- [ ] Right-click the tray icon → menu shows Status / About / Exit and no crashes.

## Section 3 — Uninstall

- [ ] Settings → Apps → Installed apps → find "Phantom" → click Uninstall.
- [ ] UAC prompt accepted.
- [ ] Uninstall progress completes without error.

Verify clean removal:

- [ ] `C:\Program Files\Phantom\` no longer exists.
- [ ] `sc query PhantomService` returns `The specified service does not exist as an installed service.` — service unregistered.
- [ ] `reg query "HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run" /v PhantomTray` returns `ERROR: The system was unable to find the specified registry key or value.` — tray autostart removed.
- [ ] Start Menu → no Phantom entry.
- [ ] `reg query "HKLM\SOFTWARE\Microsoft\Cryptography" /v MachineGuid` returns the **original** MachineGuid (compare against a value captured before any Phantom activity). If Phantom had applied a layer that was not reverted before uninstall, this reads as evidence of an incomplete cleanup — that's a bug.

Snapshot state for next section: revert to `clean-vm`.

## Section 4 — Upgrade (in-place)

Snapshot: `clean-vm`.

- [ ] Install `PhantomSetup-v0.6.0.msi` (the previous release; or any older one available). Follow §1 abbreviated — just get it installed.
- [ ] Activate a Pro license using a test key: `phantom.exe license activate <test-key>`. Accept ToU + privacy notice.
- [ ] Generate and apply a profile: `phantom.exe profile generate qa-upgrade`, then `phantom.exe apply qa-upgrade --layers 2`.
- [ ] Confirm license status: `phantom.exe license status` reports Pro.

Now upgrade:

- [ ] Double-click the new `PhantomSetup-v<newer>.msi`.
- [ ] Installer proceeds without asking to uninstall the old version first (MajorUpgrade should handle it transparently).
- [ ] Install completes.

Verify state preserved through the upgrade:

- [ ] `phantom.exe --version` reports the new version.
- [ ] `phantom.exe license status` **still** reports Pro. If it dropped to Free, the config MAC (Sprint 15) failed to validate against the new binary — investigate whether master_key_generation changed inadvertently.
- [ ] `phantom.exe config get privacy_notice_ack` is still `true`. Privacy notice / ToU acknowledgments must survive a binary swap.
- [ ] `phantom.exe status` reports the qa-upgrade profile is still active. The MachineGuid in the registry has NOT reverted — the applied layer persists across the upgrade.

## Section 5 — Reboot persistence

Runs from the upgraded install above.

- [ ] `phantom.exe status` — note the active profile and current MachineGuid.
- [ ] Reboot the VM.
- [ ] After boot: `sc query PhantomService` reports RUNNING (auto-start).
- [ ] Tray icon appears in notification area after login.
- [ ] `reg query "HKLM\SOFTWARE\Microsoft\Cryptography" /v MachineGuid` returns the same spoofed value observed before the reboot — the applied layer survived the reboot.
- [ ] `phantom.exe status` reports the profile as still active.

## Section 6 — Cleanup-during-uninstall (the important one)

Runs from Section 5 state. This validates the pre-uninstall `phantom-svc.exe --cleanup` custom action.

- [ ] Confirm a profile is currently applied: `phantom.exe status` shows Protected + a profile name.
- [ ] Capture the current MachineGuid: `reg query "HKLM\SOFTWARE\Microsoft\Cryptography" /v MachineGuid`.
- [ ] Uninstall via Settings → Apps.

After uninstall:

- [ ] MachineGuid has reverted to the original clean-VM value. If it's still the spoofed value, the cleanup custom action failed — check whether the service was already stopped when it ran (custom action is sequenced `Before="StopServices"`, but a service crash beforehand can leave it dead). This is a **Sev-1 shipping blocker** — a customer who uninstalls Phantom must be returned to their original hardware identity.
- [ ] `C:\Program Files\Phantom\` gone.
- [ ] Tray Run key gone.

Snapshot: revert to `clean-vm`.

## Section 7 — Downgrade refusal

- [ ] Install a **newer** MSI on a clean VM.
- [ ] Attempt to install an **older** MSI on top.
- [ ] Expect: dialog `A newer version of Phantom is already installed.` and installer exits without modifying anything.
- [ ] `phantom.exe --version` still reports the newer version.

## Section 8 — Cancel-mid-install

- [ ] Start an install on a clean VM. On the progress bar screen, click Cancel.
- [ ] Installer rolls back cleanly. No leftover files under `C:\Program Files\Phantom\`, no `PhantomService` registered, no Run key.
- [ ] Re-running the MSI installs cleanly.

## Reporting

For each release candidate, file the run as a comment on the release-candidate issue with:

- Windows version (10 22H2 / 11 23H2 / etc.)
- MSI filename + SHA-256
- Signed? Yes / No + signer name
- Which sections passed / failed
- For any fail: which checkbox, what actually happened, screenshot if UI

A Sev-1 fail in Section 6 blocks the release. A Sev-2 fail (SmartScreen still hot after weeks, cosmetic UI glitch) can ship with a known-issues note.

## Notes for future revisions

- **Layer 1 driver (deferred).** When Layer 1 lands, the driver install/remove custom actions come back to the .wxs and this runbook grows a section for `pnputil` install verification and Code Integrity / Test Signing checks.
- **WiX v3 EOL.** Migrate to WiX v4 or WiX v5 post-v1.0. The MSI table on disk is the same; only the descriptor syntax changes.
