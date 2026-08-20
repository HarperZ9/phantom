# Uninstalling Phantom

The uninstall is designed to leave your machine exactly as it was
before Phantom was installed. Any identity layers Phantom applied
are reverted **before** the files are removed, so you cannot end up
with a spoofed identity and no tool to fix it.

## 1. Uninstall via Settings

- Open **Settings** → **Apps** → **Installed apps**.
- Find **Phantom** in the list.
- Click the ⋯ menu → **Uninstall**.
- Accept the UAC prompt.

Or from an elevated terminal:

```
msiexec /x "PhantomSetup-v1.0.0.msi"
```

## What happens during uninstall

The installer runs three things in order:

1. `phantom-svc.exe --cleanup` — reverts every active identity
   layer from Phantom's backup files, removes the tray autostart
   entry, and clears the sealed config.
2. Stops and unregisters the **Phantom Privacy Service**.
3. Removes `C:\Program Files\Phantom\` and its contents.

If step 1 fails (for example, the service was already stopped
manually), the uninstall still proceeds so you are not stuck with a
half-installed product. In that case run `phantom revert` yourself
from another install or a portable build before uninstalling next
time.

## 2. Verify the cleanup

After uninstall, open an elevated terminal and check:

```
reg query "HKLM\SOFTWARE\Microsoft\Cryptography" /v MachineGuid
```

The value should be your **original** MachineGuid — the one you saw
in `phantom audit` before you ever applied a profile.

```
sc query PhantomService
```

Should report `The specified service does not exist as an installed
service.`

```
dir "C:\Program Files\Phantom"
```

Should report `File Not Found`.

## Data that persists

By default the uninstall removes program files and the service, but
leaves your **profiles, license record, and configuration** under
`%APPDATA%\Phantom\`. This is deliberate: reinstalling picks your
setup back up exactly where you left it.

To wipe everything, including your license:

```
rmdir /s /q "%APPDATA%\Phantom"
```

Warning: this deletes your license activation record. You will need
to re-activate with your key on the next install. The key itself is
not consumed — it re-activates cleanly on the same machine (same
fingerprint).

## Reporting a bad uninstall

If your MachineGuid is still the spoofed value after uninstall,
that is a Sev-1 bug. Open an issue at
`https://github.com/HarperZ9/phantom/issues` with:

- Windows version (`winver`)
- Phantom version you uninstalled from (`phantom --version` before
  uninstall, if you still have it)
- Current MachineGuid vs. what you expect it to be

We will help you revert manually and diagnose why the automatic
revert did not fire.
