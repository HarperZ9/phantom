# Windows validation runbook

Minimum steps to validate a Phantom build on a fresh Windows image.
The exit gate on Sprint 22 is that a second engineer can execute
this runbook end-to-end without asking follow-up questions and end
with a green summary at the bottom.

## Prereqs, one-time per Windows image

Pick one of the two VM environments. Both work; the choice is
whatever the engineer already has.

**Option A: Hyper-V (Windows host)**
1. `hvc.exe list` to confirm Hyper-V is enabled.
2. Download the Windows 10 (or 11) Enterprise Evaluation ISO from
   Microsoft's Evaluation Center. 90-day license, sufficient for
   Phase 1.
3. Create a Gen 2 VM, 4 vCPU / 8 GB RAM / 60 GB disk, one virtual
   NIC on the external switch.
4. Install Windows. Skip the Microsoft account prompt, use a local
   account named `phantom-dev`.
5. Enable Remote Desktop.
6. **Take a snapshot named `clean`.**

**Option B: cloud VM (Azure)**
1. Create a `Standard_D4s_v3` Windows Server 2022 VM in your
   preferred region.
2. Enable RDP inbound on port 3389.
3. Snapshot the OS disk before proceeding.

## Prereqs, dev tools inside the VM

Once inside the VM:

```powershell
# Install Rust (MSVC toolchain)
Invoke-WebRequest -Uri https://win.rustup.rs -OutFile rustup-init.exe
.\rustup-init.exe -y --default-toolchain stable --default-host x86_64-pc-windows-msvc
$env:PATH += ";$env:USERPROFILE\.cargo\bin"

# Install git
winget install --id Git.Git --silent --accept-package-agreements

# (Optional) VS Build Tools for MSVC, Rust may prompt for this on
# first `cargo build`. If prompted, install the C++ workload only.
```

Verify:
```powershell
rustc --version   # expect: rustc 1.7x or newer
cargo --version
git --version
```

## Building from source

```powershell
git clone https://github.com/HarperZ9/phantom.git
cd phantom
cargo build --release --workspace
```

Expected: no errors. Warnings on the `phantom-driver` C code are
fine; the Rust workspace should complete clean.

The signed release archives from GitHub already contain built
binaries, building from source is for engineers, not for
customer QA.

## Smoke test, Layer 2 registry spoofing

The one behavior that matters for v1: `phantom apply … --layers 2`
changes real registry values and `phantom revert` restores them.

**Before any modification, snapshot the affected registry values:**

```powershell
# Run PowerShell as Administrator for these
$before = @{}
'HKLM:\SOFTWARE\Microsoft\Cryptography',
'HKLM:\SOFTWARE\Microsoft\SQMClient',
'HKLM:\SYSTEM\CurrentControlSet\Control\ComputerName\ComputerName' |
    ForEach-Object {
        $key = $_
        Get-ItemProperty -Path $key | Get-Member -MemberType NoteProperty |
            Where-Object { $_.Name -notmatch '^PS' } |
            ForEach-Object { $before["$key\$($_.Name)"] = (Get-ItemProperty -Path $key -Name $_.Name).($_.Name) }
    }
$before | ConvertTo-Json > pre-phantom-snapshot.json
Write-Host "Snapshotted $($before.Count) values to pre-phantom-snapshot.json"
```

**Audit the current identity Phantom sees:**

```powershell
$env:PHANTOM_DATA_DIR = "$env:TEMP\phantom-runbook"
Remove-Item -Recurse -Force $env:PHANTOM_DATA_DIR -ErrorAction SilentlyContinue

.\target\release\phantom-cli.exe audit
```

Expected: prints tables of SMBIOS, disk, network, GPU, TPM, display,
Windows registry, and boot identifiers currently visible.

**Generate a profile:**

```powershell
.\target\release\phantom-cli.exe profile generate runbook-demo --seed "runbook-seed-42"
.\target\release\phantom-cli.exe profile show runbook-demo
```

Expected: profile summary printed. The `origin_mark` in the saved
JSON at `%TEMP%\phantom-runbook\profiles\runbook-demo.json` shows
tier `Free`, this machine's fingerprint hex, and a MAC.

**Apply Layer 2:**

```powershell
.\target\release\phantom-cli.exe apply runbook-demo --layers 2
```

Expected: prints per-value results for MachineGuid, HwProfileGuid,
MachineId, ProductId, and InstallDate. No `FAILED`. ComputerName is
deliberately not spoofed at Layer 2 (a half-rename breaks reboot and
WMI); it is deferred to a full rename implementation.

**Verify the registry actually changed:**

```powershell
(Get-ItemProperty -Path 'HKLM:\SOFTWARE\Microsoft\Cryptography').MachineGuid
```

Expected: matches `os.machine_guid` from the profile JSON, NOT
the value from `pre-phantom-snapshot.json`.

**Validate cross-source consistency:**

```powershell
.\target\release\phantom-cli.exe validate runbook-demo
```

Expected: `is_consistent: true`. Any inconsistency here means one
of the registry writes silently failed, or the reader is reading
a different key than the writer wrote to.

**Revert:**

```powershell
.\target\release\phantom-cli.exe revert
```

Expected: prints restored values.

**Confirm original registry values are back:**

```powershell
(Get-ItemProperty -Path 'HKLM:\SOFTWARE\Microsoft\Cryptography').MachineGuid
```

Expected: matches `pre-phantom-snapshot.json['HKLM:\SOFTWARE\Microsoft\Cryptography\MachineGuid']`.

## Smoke test, license + phone-home + tripwire

**License activation flow (interactive):**

```powershell
# In a live TTY (not piped through anything)
.\target\release\phantom-cli.exe license activate PHNTM-TEST-KEY-XXXX-YYYY-ZZZZ
```

Expected: displays the ToU, prompts `[y/N]`; displays the Privacy
Notice, prompts `[Y/n]`. Then produces "License activated: Free
tier" or an InvalidSignature error (since we used a fake key).
Either is fine, what matters is the disclosures printed.

**License request enrollment block:**

```powershell
.\target\release\phantom-cli.exe license request --tier pro
```

Expected: prints machine fingerprint hex, requested tier, platform,
build info. This block is what a customer would send to the
licensing team to receive a real key.

**Honey key trap (must NOT distinguish itself):**

```powershell
.\target\release\phantom-cli.exe license activate "PHANTOM-MASTER-UNLOCK-ENTERPRISE-TIER-PERPETUAL-XXXXXXXXXXXXXXXXXX"
```

Expected: prints "Activation failed: license key signature
verification failed", the SAME error a random invalid key would
produce. Then:

```powershell
.\target\release\phantom-cli.exe tamper-report
```

Expected: shows `TRIPPED, install silently downgraded to Free
tier` and `HIGH honey_key_attempt`. This confirms the trap fires
silently to the user but is visible to the operator running the
tamper report.

**Self-check:**

```powershell
.\target\release\phantom-cli.exe --json self-check
```

Expected: JSON with `healthy: true` (before the honey key trap
fired) or `healthy: false` (after). `debugger_detectors_triggered`
should be `[]` on a normal VM without profilers attached.

## Cleanup

```powershell
Remove-Item -Recurse -Force $env:PHANTOM_DATA_DIR
Remove-Item pre-phantom-snapshot.json
# Restore VM snapshot back to `clean` for the next run
```

## Runbook exit criteria

Runbook is passed when a second engineer can complete every command
above and mark this checklist:

- [ ] `cargo build --release --workspace` finishes without errors
- [ ] `phantom audit` prints identifier tables for at least SMBIOS
      and OS registry
- [ ] `phantom profile generate` writes a JSON with a valid
      `origin_mark`
- [ ] `phantom apply … --layers 2` changes `MachineGuid` to the
      profile value
- [ ] `phantom validate runbook-demo` reports consistent
- [ ] `phantom revert` restores `MachineGuid` byte-for-byte
- [ ] `phantom license activate` in TTY shows ToU + Privacy Notice
      and prompts
- [ ] Honey key attempt prints the same error as a random invalid
      key; `tamper-report` shows the trip
- [ ] `phantom --json self-check` returns valid JSON with
      `healthy: true` on a clean VM

Failure modes seen during runbook execution get filed as bugs
against the current sprint, not swept under the rug. The whole
point of this sprint is that we discover what's actually broken.
