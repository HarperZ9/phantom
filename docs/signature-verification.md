# Verifying your download

Every Phantom release publishes three artifacts:

- **Linux tarball**: `phantom-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz`
- **Windows zip**: `phantom-vX.Y.Z-x86_64-pc-windows-msvc.zip`
- **Windows MSI installer**: `PhantomSetup-vX.Y.Z.msi`

Alongside them, `SHA256SUMS.txt` lists the SHA-256 of every artifact in
that release. Verify against it before you run anything: not because we
assume we uploaded something bad to our own release page, but because
what reaches your disk crossed the network, a proxy, and a mirror, any
of which can corrupt or substitute a file.

## Verify with SHA-256

Download `SHA256SUMS.txt` from the same release page as the artifact you
plan to run, and put them in the same directory.

### Windows (PowerShell)

```powershell
Get-FileHash -Algorithm SHA256 PhantomSetup-v1.0.0.msi
```

Compare the printed hash against the matching line in `SHA256SUMS.txt`.
They must match exactly. With the WSL or Git-Bash `sha256sum` tool you
can check every file at once:

```
sha256sum -c SHA256SUMS.txt
```

### Linux / macOS

```
sha256sum -c SHA256SUMS.txt          # Linux
shasum -a 256 -c SHA256SUMS.txt      # macOS
```

If any line reports `FAILED`, delete that download and fetch it again;
do not run it. If every line reports `OK`, you have the exact bytes we
shipped.

## About code signing

**v1.0.0 is not code-signed.** There is no Authenticode signature on the
MSI yet, so:

- `signtool verify /pa PhantomSetup-v1.0.0.msi` reports **no signature**.
  That is expected for this release and does **not** mean the file was
  tampered with. Use the SHA-256 check above to confirm integrity.
- Windows SmartScreen shows "Windows protected your PC" when you run the
  MSI, because an unsigned installer has no download reputation. Click
  **More info**, then **Run anyway**. There is no publisher line to check
  yet, so the SHA-256 match is your integrity proof.

A code-signing certificate is planned. When it lands, signed releases
will carry an Authenticode signature you can verify with `signtool`, and
this page will describe how.

## Reporting a mismatch

If `sha256sum -c` reports `FAILED` on a file downloaded directly from
`https://github.com/HarperZ9/phantom/releases/`, open an issue with:

- The release tag (`v1.0.0`, etc.)
- The filename
- The hash you got locally
- The hash `SHA256SUMS.txt` claims it should be
- Your platform (OS and downloader: browser, `curl`, `wget`, `gh`)

We will investigate before shipping the next release.
