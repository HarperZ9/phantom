# Installing Phantom on Windows

Phantom ships as a signed MSI. Installation takes about a minute on a
modern PC.

## System requirements

- Windows 10 (22H2) or Windows 11 (any current build), x64
- Administrator account
- ~30 MB of disk space
- Internet connection **only** for license activation and the daily
  license check-in. Phantom itself never sends anything else off your
  machine — see the privacy notice below.

## 1. Download

Grab `PhantomSetup-v1.0.0.msi` from the [releases page](https://github.com/HarperZ9/phantom/releases/latest).

Alongside the MSI you will find `SHA256SUMS.txt`. Verify the download
before running it — see [`signature-verification.md`](../signature-verification.md).

## 2. Run the installer

Double-click `PhantomSetup-v1.0.0.msi` and accept the UAC prompt.

If Windows SmartScreen displays "Windows protected your PC", click
**More info** → **Run anyway**. This warning is normal for newly
released software; it fades as our signing certificate accumulates
download reputation.

The installer will:

- Copy `phantom.exe`, `phantom-svc.exe`, and `phantom-tray.exe` to
  `C:\Program Files\Phantom\`.
- Register **Phantom Privacy Service** as an auto-start Windows
  service running under `LocalSystem`. The service is the only
  component that touches the registry keys Phantom manages.
- Add **Phantom** to the Start menu.
- Set the tray app to launch when you sign in.

## 3. Verify install

Open a terminal (PowerShell or `cmd`) and run:

```
phantom --version
```

You should see `phantom 1.0.0` (or newer). If Windows reports
"phantom is not recognized", close and reopen your terminal — the
PATH refreshes on new shells only.

Check the service is running:

```
sc query PhantomService
```

Look for `STATE : 4 RUNNING`.

## Next steps

- [Activate your license](activate.md) — Phantom runs in Free tier
  until you activate; Free tier can inspect but not spoof.
- [Create your first profile](first-profile.md) — the shortest path
  to actually changing your identity.

## Privacy notice

Phantom phones home once every 24 hours to check whether your
license has been revoked. The payload contains only:

- Your license serial (an opaque 8-hex string; not reversible to
  your key or your machine)
- The tier your install believes it holds
- Your Phantom version
- A counter of how many times you have tripped the local integrity
  check

It does not contain your hardware fingerprint, your identity, your
IP-level metadata beyond what Cloudflare records for any request, or
any profile you have applied. Read `phantom privacy-notice` for the
full text.

You can disable the phone-home entirely with `phantom config set
phone_home_enabled false`; your license will continue to work
locally until it expires, and no revocation check will occur.

## Uninstalling

See [uninstall.md](uninstall.md).
