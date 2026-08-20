@echo off
setlocal

REM ──────────────────────────────────────────────────────────────────
REM  Phantom Installer Build Script
REM
REM  Prerequisites:
REM    1. WiX Toolset v3.x installed (wixtoolset.org)
REM    2. Rust binaries built:  cd phantom && cargo build --release
REM    3. Driver built:         cd phantom-driver && msbuild ...
REM
REM  Usage:
REM    build.cmd             Build PhantomSetup.msi
REM    build.cmd clean       Remove build artifacts
REM ──────────────────────────────────────────────────────────────────

set SCRIPT_DIR=%~dp0
set WXS=%SCRIPT_DIR%phantom.wxs
set EULA=%SCRIPT_DIR%eula.rtf
set OUT_DIR=%SCRIPT_DIR%out
set OBJ=%OUT_DIR%\phantom.wixobj
set MSI=%OUT_DIR%\PhantomSetup.msi

if "%1"=="clean" (
    if exist "%OUT_DIR%" rmdir /s /q "%OUT_DIR%"
    echo Cleaned.
    exit /b 0
)

REM Check for WiX tools
where candle.exe >nul 2>&1
if errorlevel 1 (
    echo ERROR: WiX Toolset not found. Install from https://wixtoolset.org
    echo   or add it to PATH.
    exit /b 1
)

REM Verify build artifacts exist
set BUILD_DIR=%SCRIPT_DIR%..\target\release
if not exist "%BUILD_DIR%\phantom-cli.exe" (
    echo ERROR: phantom-cli.exe not found. Run: cd phantom ^&^& cargo build --release
    exit /b 1
)
if not exist "%BUILD_DIR%\phantom-svc.exe" (
    echo ERROR: phantom-svc.exe not found. Run: cd phantom ^&^& cargo build --release
    exit /b 1
)
if not exist "%BUILD_DIR%\phantom-tray.exe" (
    echo ERROR: phantom-tray.exe not found. Run: cd phantom ^&^& cargo build --release
    exit /b 1
)

REM Create output directory
if not exist "%OUT_DIR%" mkdir "%OUT_DIR%"

echo Compiling WiX source...
candle.exe -nologo -ext WixUtilExtension -ext WixUIExtension -out "%OBJ%" "%WXS%"
if errorlevel 1 (
    echo ERROR: candle.exe failed.
    exit /b 1
)

echo Linking MSI...
light.exe -nologo -ext WixUtilExtension -ext WixUIExtension -out "%MSI%" "%OBJ%"
if errorlevel 1 (
    echo ERROR: light.exe failed.
    exit /b 1
)

echo.
echo Build complete: %MSI%
echo.
echo Install:    msiexec /i "%MSI%"
echo Uninstall:  msiexec /x "%MSI%"
