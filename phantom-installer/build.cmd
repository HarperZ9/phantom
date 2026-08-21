@echo off
setlocal enabledelayedexpansion

REM ==================================================================
REM  Phantom Installer Build Script
REM
REM  Prerequisites:
REM    1. WiX Toolset v3.x installed (wixtoolset.org) and on PATH.
REM    2. Rust binaries built:  cargo build --release --workspace
REM
REM  Usage:
REM    build.cmd                  Build PhantomSetup-<version>.msi
REM    build.cmd clean            Remove build artifacts
REM
REM  Optional environment:
REM    PHANTOM_MSI_VERSION        Overrides the version in the MSI
REM                               filename. Default: 0.6.0. Must
REM                               match the ProductVersion in the .wxs.
REM ==================================================================

set SCRIPT_DIR=%~dp0
set WXS=%SCRIPT_DIR%phantom.wxs
set OUT_DIR=%SCRIPT_DIR%out
set OBJ=%OUT_DIR%\phantom.wixobj

if "%PHANTOM_MSI_VERSION%"=="" set PHANTOM_MSI_VERSION=1.0.0
set MSI=%OUT_DIR%\PhantomSetup-v%PHANTOM_MSI_VERSION%.msi

if "%1"=="clean" (
    if exist "%OUT_DIR%" rmdir /s /q "%OUT_DIR%"
    echo Cleaned.
    exit /b 0
)

REM ---- WiX presence check ------------------------------------------

where candle.exe >nul 2>&1
if errorlevel 1 (
    echo ERROR: WiX Toolset not found on PATH.
    echo   Install v3.x from https://wixtoolset.org
    echo   or on CI:  choco install wixtoolset -y
    exit /b 1
)

REM ---- Release binary presence check -------------------------------

set BUILD_DIR=%SCRIPT_DIR%..\target\release
for %%B in (phantom-cli.exe phantom-svc.exe phantom-tray.exe) do (
    if not exist "%BUILD_DIR%\%%B" (
        echo ERROR: %%B not found in %BUILD_DIR%.
        echo   Run: cargo build --release --workspace
        exit /b 1
    )
)

REM ---- Build --------------------------------------------------------

if not exist "%OUT_DIR%" mkdir "%OUT_DIR%"

echo Compiling WiX source (candle.exe)...
REM  -dProductVersion overrides the ?define in phantom.wxs so a CI
REM  build for tag v0.7.0 embeds 0.7.0 as the MSI ProductVersion,
REM  without having to edit the .wxs on every version bump.
candle.exe -nologo -ext WixUtilExtension -ext WixUIExtension -arch x64 ^
    -dProductVersion=%PHANTOM_MSI_VERSION% ^
    -dBuildDir="%BUILD_DIR%" ^
    -out "%OBJ%" "%WXS%"
if errorlevel 1 (
    echo ERROR: candle.exe failed.
    exit /b 1
)

echo Linking MSI (light.exe)...
light.exe -nologo -ext WixUtilExtension -ext WixUIExtension ^
    -cultures:en-us -out "%MSI%" "%OBJ%"
if errorlevel 1 (
    echo ERROR: light.exe failed.
    exit /b 1
)

echo.
echo Build complete: %MSI%
echo.
echo   Install:    msiexec /i "%MSI%"
echo   Uninstall:  msiexec /x "%MSI%"
echo   Sign:       call sign.cmd "%MSI%"
