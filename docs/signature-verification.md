# Verifying your download

Every Phantom release publishes three signed artifacts:

- **Linux tarball** — `phantom-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz`
- **Windows zip** — `phantom-vX.Y.Z-x86_64-pc-windows-msvc.zip`
- **Windows MSI installer** — `PhantomSetup-vX.Y.Z.msi`

Alongside them, `SHA256SUMS.txt` lists the SHA-256 of every artifact
in that release. The Windows MSI and the Windows PE binaries inside
it are additionally signed with an EV code-signing certificate
issued to the Phantom vendor.

Verify before you run. Not because we are asking you to trust that
we uploaded something malicious to our own release page, but because
what reaches your disk went through the network and a proxy and a
mirror and any of those can corrupt or substitute a file.

## 1. Verify with SHA-256 (any platform)

Download `SHA256SUMS.txt` from the same release page as the artifact
you plan to run. Place them side by side in the same directory. Then:

### Windows (PowerShell)

```powershell
Get-FileHash -Algorithm SHA256 PhantomSetup-v1.0.0.msi
```

Compare the printed hash against the matching line in
`SHA256SUMS.txt`. They must match exactly.

Or, with the WSL / Git-Bash `sha256sum` tool:

```
sha256sum -c SHA256SUMS.txt
```

Reports `OK` for each file whose local hash matches.

### Linux / macOS

```
sha256sum -c SHA256SUMS.txt          # Linux
shasum -a 256 -c SHA256SUMS.txt      # macOS
```

If any line reports `FAILED`, delete that download and fetch it
again — do not run it. If every line reports `OK`, you have the
bytes we shipped.

## 2. Verify the Windows signature

The MSI and every EXE inside it are Authenticode-signed. Windows
enforces this automatically at install time; you can also verify by
hand:

```powershell
signtool verify /pa /v PhantomSetup-v1.0.0.msi
```

Expected output ends with `Successfully verified`. The signer chain
should terminate in a certificate issued by a well-known CA
(Sectigo or DigiCert), issued to a subject that names Phantom's
vendor entity.

If `signtool` reports the signature is missing or invalid, the file
has been tampered with **or** the signature block was stripped by a
proxy. Delete and re-download.

`signtool.exe` ships with the Windows 10/11 SDK; if it is not on
your PATH, look under `C:\Program Files (x86)\Windows Kits\10\bin\
<sdk-version>\x64\signtool.exe`.

## 3. What SmartScreen tells you

When you double-click the MSI, Windows SmartScreen may display
"Windows protected your PC". This is Windows' reputation check
against Microsoft's telemetry — a valid signature is necessary but
not sufficient for SmartScreen to give a green light; the certificate
also needs download-count reputation, which accumulates over the
first few weeks after each cert renewal.

If SmartScreen warns you:

1. Click **More info**.
2. Verify the "Publisher" line names the Phantom vendor. If it does
   not, do not click Run anyway — the file is not what it claims.
3. Click **Run anyway**.

## Reporting a mismatch

If `sha256sum -c` reports `FAILED` on a file downloaded directly
from `https://github.com/HarperZ9/phantom/releases/`, please open
an issue and include:

- The release tag you were downloading (`v1.0.0`, etc.)
- The filename
- The hash you got locally
- The hash `SHA256SUMS.txt` claims it should be
- Your platform (OS + downloader used: browser, `curl`, `wget`, `gh`)

We will investigate before shipping the next release.
