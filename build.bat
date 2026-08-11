@echo off
setlocal EnableExtensions EnableDelayedExpansion

for %%I in ("%~dp0.") do set "ROOT=%%~fI"
pushd "%ROOT%" >nul || exit /b 1

set "RUN_CHECKS=1"
set "RUN_BUILD=1"
set "RUN_PACKAGE=1"
set "FAIL_MESSAGE="

:parse_args
if "%~1"=="" goto args_done
if /I "%~1"=="--quick" set "RUN_CHECKS=0" & shift & goto parse_args
if /I "%~1"=="--no-check" set "RUN_CHECKS=0" & shift & goto parse_args
if /I "%~1"=="--no-package" set "RUN_PACKAGE=0" & shift & goto parse_args
if /I "%~1"=="--check-only" set "RUN_BUILD=0" & set "RUN_PACKAGE=0" & shift & goto parse_args
if /I "%~1"=="-h" goto usage
if /I "%~1"=="--help" goto usage
set "FAIL_MESSAGE=Unknown option: %~1"
goto usage

:args_done
where cargo >nul 2>nul || (set "FAIL_MESSAGE=Cargo is not available in PATH" & goto fail)
where powershell >nul 2>nul || (set "FAIL_MESSAGE=PowerShell is not available in PATH" & goto fail)

set "VERSION=0.1.0"
set "OS_NAME=windows"
set "BIN_EXT=.exe"

if "%RUN_CHECKS%"=="1" (
    echo [build] Checking formatting
    set "FAIL_MESSAGE=cargo fmt failed"
    cargo fmt --all -- --check || goto fail

    echo [build] Checking workspace types
    set "FAIL_MESSAGE=cargo check failed"
    cargo check --workspace --all-targets --all-features --locked || goto fail

    echo [build] Running Clippy with warnings denied
    set "FAIL_MESSAGE=cargo clippy failed"
    cargo clippy --workspace --all-targets --all-features --locked -- -D warnings || goto fail

    echo [build] Running workspace tests
    set "FAIL_MESSAGE=cargo test failed"
    cargo test --workspace --all-features --locked || goto fail

    echo [done] Validation suite passed
)

if "%RUN_BUILD%"=="0" goto done

echo [build] Building release workspace
set "FAIL_MESSAGE=cargo build failed"
cargo build --workspace --release --all-features --locked || goto fail

if "%RUN_PACKAGE%"=="0" (
    echo [done] Release binaries are in target\release\
    goto done
)

set "DIST_DIR=%ROOT%\target\dist"
set "PACKAGE_NAME=VKey-rs-%VERSION%-%OS_NAME%"
set "PACKAGE_DIR=%DIST_DIR%\%PACKAGE_NAME%"

if /I not "%DIST_DIR%"=="%ROOT%\target\dist" (
    set "FAIL_MESSAGE=Refusing to replace unexpected path: %DIST_DIR%"
    goto fail
)

if exist "%DIST_DIR%" rmdir /s /q "%DIST_DIR%"
mkdir "%PACKAGE_DIR%\bin" "%PACKAGE_DIR%\config" >nul 2>nul || (
    set "FAIL_MESSAGE=Failed to create package directory"
    goto fail
)

for %%B in (VKey-rs VKey-core-test keyboard-debug keyboard-core-debug) do (
    if not exist "%ROOT%\target\release\%%B%BIN_EXT%" (
        set "FAIL_MESSAGE=Missing release binary: %ROOT%\target\release\%%B%BIN_EXT%"
        goto fail
    )
    copy /y "%ROOT%\target\release\%%B%BIN_EXT%" "%PACKAGE_DIR%\bin\" >nul || (
        set "FAIL_MESSAGE=Failed to copy %%B%BIN_EXT%"
        goto fail
    )
)

copy /y "%ROOT%\README.md" "%PACKAGE_DIR%\" >nul || (
    set "FAIL_MESSAGE=Failed to copy README.md"
    goto fail
)

powershell -NoProfile -Command "Copy-Item -Path 'config\*' -Destination '%PACKAGE_DIR%\config' -Recurse -Force" || (
    set "FAIL_MESSAGE=Failed to copy config"
    goto fail
)

for %%A in (vkey_icon_*.png vkey_logo_*.png) do (
    if exist "%ROOT%\%%A" copy /y "%ROOT%\%%A" "%PACKAGE_DIR%\" >nul
)

powershell -NoProfile -Command "$archive = '%DIST_DIR%\%PACKAGE_NAME%.zip'; if (Test-Path $archive) { Remove-Item -Force $archive }; Compress-Archive -Path '%PACKAGE_DIR%\*' -DestinationPath $archive -Force" || (
    set "FAIL_MESSAGE=Failed to create zip archive"
    goto fail
)

echo [done] Package directory: %PACKAGE_DIR%
echo [done] Release archive: %DIST_DIR%\%PACKAGE_NAME%.zip
goto done

:usage
echo Usage: build.bat [OPTIONS]
echo.
echo Build, verify, and package VKey-rs on Windows.
echo.
echo Options:
echo   --quick         Skip fmt, clippy, and tests; build/package release directly
echo   --no-check      Alias for --quick
echo   --no-package    Build release binaries without creating an archive
echo   --check-only    Run the complete validation suite without building a package
echo   -h, --help      Show this help
echo.
echo Artifacts are written to target\dist\.
if defined FAIL_MESSAGE goto fail
goto done

:fail
echo [error] %FAIL_MESSAGE%
popd >nul
endlocal
exit /b 1

:done
popd >nul
endlocal
exit /b 0
