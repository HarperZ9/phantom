@echo off
setlocal

REM ==================================================================
REM  Phantom code-signing helper.
REM
REM  Signs a Windows PE binary or MSI with the vendor's EV cert. Called
REM  from CI (release.yml) once per release artifact.
REM
REM  Usage:
REM    sign.cmd <path-to-msi-or-exe>
REM
REM  Required environment:
REM    PHANTOM_SIGNING_CERT_B64   Base64 of the .pfx file.
REM    PHANTOM_SIGNING_CERT_PASS  .pfx password.
REM
REM  Optional environment:
REM    PHANTOM_SIGNING_TIMESTAMP_URL
REM      RFC 3161 timestamp authority. Default: DigiCert.
REM    PHANTOM_SIGNING_DESCRIPTION
REM      Human-readable /d description embedded in the signature.
REM
REM  Behavior when secrets are absent:
REM    Prints a warning and exits 0. This lets nightly / PR builds
REM    produce an unsigned MSI without failing the pipeline; only tag
REM    pushes are expected to have the secrets set.
REM ==================================================================

if "%~1"=="" (
    echo Usage: sign.cmd ^<path-to-msi-or-exe^>
    exit /b 2
)
set TARGET=%~1

if "%PHANTOM_SIGNING_CERT_B64%"=="" (
    echo [sign.cmd] PHANTOM_SIGNING_CERT_B64 not set - skipping signing of "%TARGET%".
    echo             ^(This is expected for unsigned dev/CI builds.^)
    exit /b 0
)
if "%PHANTOM_SIGNING_CERT_PASS%"=="" (
    echo [sign.cmd] PHANTOM_SIGNING_CERT_B64 is set but PHANTOM_SIGNING_CERT_PASS is not.
    echo             Refusing to sign - fix the secret pair.
    exit /b 1
)

if "%PHANTOM_SIGNING_TIMESTAMP_URL%"=="" (
    set PHANTOM_SIGNING_TIMESTAMP_URL=http://timestamp.digicert.com
)
if "%PHANTOM_SIGNING_DESCRIPTION%"=="" (
    set PHANTOM_SIGNING_DESCRIPTION=Phantom - hardware identity privacy tool
)

REM ---- Materialize the pfx from the base64 secret ------------------

set PFX=%TEMP%\phantom-signing.pfx
powershell -NoProfile -Command ^
    "$b64 = $env:PHANTOM_SIGNING_CERT_B64;" ^
    "[IO.File]::WriteAllBytes('%PFX%', [Convert]::FromBase64String($b64))"
if errorlevel 1 (
    echo [sign.cmd] Failed to decode PHANTOM_SIGNING_CERT_B64.
    exit /b 1
)

REM ---- Locate signtool ---------------------------------------------
REM  On windows-latest, signtool ships with the Windows SDK. We try a
REM  small set of known locations before failing.

set SIGNTOOL=
for %%D in (
    "C:\Program Files (x86)\Windows Kits\10\bin\10.0.22621.0\x64\signtool.exe"
    "C:\Program Files (x86)\Windows Kits\10\bin\10.0.22000.0\x64\signtool.exe"
    "C:\Program Files (x86)\Windows Kits\10\bin\10.0.19041.0\x64\signtool.exe"
    "C:\Program Files (x86)\Windows Kits\10\bin\x64\signtool.exe"
) do (
    if exist %%D set SIGNTOOL=%%D
)
if "%SIGNTOOL%"=="" (
    where signtool.exe >nul 2>&1
    if not errorlevel 1 set SIGNTOOL=signtool.exe
)
if "%SIGNTOOL%"=="" (
    echo [sign.cmd] signtool.exe not found. Install the Windows SDK.
    del /q "%PFX%" 2>nul
    exit /b 1
)

REM ---- Sign + verify ------------------------------------------------

echo [sign.cmd] Signing "%TARGET%" using %SIGNTOOL%
%SIGNTOOL% sign ^
    /fd SHA256 ^
    /td SHA256 ^
    /tr "%PHANTOM_SIGNING_TIMESTAMP_URL%" ^
    /f "%PFX%" ^
    /p "%PHANTOM_SIGNING_CERT_PASS%" ^
    /d "%PHANTOM_SIGNING_DESCRIPTION%" ^
    "%TARGET%"
set SIGN_RC=%ERRORLEVEL%

del /q "%PFX%" 2>nul

if not %SIGN_RC%==0 (
    echo [sign.cmd] signtool sign failed with exit %SIGN_RC%.
    exit /b %SIGN_RC%
)

echo [sign.cmd] Verifying signature...
%SIGNTOOL% verify /pa /v "%TARGET%"
exit /b %ERRORLEVEL%
