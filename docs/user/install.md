# Installing Phantom on Windows

Phantom ships as a Windows MSI. Installation takes under a minute.

## System requirements

- Windows 10 (22H2) or Windows 11 (any current build), x64
- Administrator account
- ~30 MB of disk space
- No internet connection required. Licensing keys are issued out of
  band, and phone-home is off until you configure it (see the
  [privacy notice](privacy.md)).

## 1. Download

Grab `PhantomSetup-v1.1.0.msi` from the [releases page](https://github.com/HarperZ9/phantom/releases/latest),
along with `SHA256SUMS.txt`. Verify the download before running it (see
[`signature-verification.md`](../signature-verification.md)).

## 2. Run the installer

Double-click `PhantomSetup-v1.1.0.msi` and accept the UAC prompt.

The MSI is not yet code-signed, so Windows SmartScreen displays "Windows
protected your PC". Click **More info**, then **Run anyway**. This
warning stays until a code-signing certificate is in place.

The installer will:

- Copy `phantom.exe`, `phantom-svc.exe`, and `phantom-tray.exe` to
  `C:\Program Files\Phantom\`.
- Register **Phantom Privacy Service** as an auto-start Windows service
  running under `LocalSystem`. The service re-applies your active profile
  across reboots on Pro and Enterprise licenses.
- Create the machine-wide store at `%ProgramData%\Phantom`.
- Add **Phantom** to the Start menu and set the tray app to launch at
  sign-in.

## 3. Verify install

Open a terminal (PowerShell or `cmd`) and run:

```
phantom --version
```

You should see `phantom 1.1.0`. If Windows reports "phantom is not
recognized", close and reopen your terminal; PATH refreshes on new
shells only.

Check the service is running:

```
sc query PhantomService
```

Look for `STATE : 4 RUNNING`.

## Next steps

- [Create your first profile](first-profile.md). Free tier already
  applies a Layer-2 profile, so you can start here.
- [Activate a license](activate.md) for Pro or Enterprise features.

## Privacy

Phantom does not phone home by default. It sends nothing off the machine
until you set a callback URL, and even then the payload is minimal and
signed. See [privacy.md](privacy.md) or run `phantom privacy-notice`.

## Uninstalling

See [uninstall.md](uninstall.md).
