@echo off
setlocal EnableExtensions EnableDelayedExpansion

for %%I in ("%~dp0.") do set "ROOT=%%~fI"
pushd "%ROOT%" >nul || exit /b 1

set "COMMAND=run"
set "ARGS="
set "FAIL_MESSAGE="

if not "%~1"=="" (
    set "COMMAND=%~1"
    shift
)

:collect_args
if "%~1"=="" goto args_done
if defined ARGS (
    set "ARGS=!ARGS! %~1"
) else (
    set "ARGS=%~1"
)
shift
goto collect_args

:args_done
where cargo >nul 2>nul || (set "FAIL_MESSAGE=Cargo is not available in PATH" & goto fail)

if /I "%COMMAND%"=="run" goto run
if /I "%COMMAND%"=="headless" goto headless
if /I "%COMMAND%"=="core" goto core
if /I "%COMMAND%"=="test-cli" goto core
if /I "%COMMAND%"=="kbd-debug" goto kbd_debug
if /I "%COMMAND%"=="core-debug" goto core_debug
if /I "%COMMAND%"=="test" goto test
if /I "%COMMAND%"=="check" goto check
if /I "%COMMAND%"=="all" goto all
if /I "%COMMAND%"=="package" goto package
if /I "%COMMAND%"=="help" goto usage
if /I "%COMMAND%"=="-h" goto usage
if /I "%COMMAND%"=="--help" goto usage

set "FAIL_MESSAGE=Unknown command: %COMMAND%"
goto usage

:run
if not defined RUST_BACKTRACE set "RUST_BACKTRACE=1"
if not defined RUST_LOG set "RUST_LOG=debug"
if not defined ARGS set "ARGS=--debug-input"
echo [dev] Starting VKey-rs %ARGS%
set "FAIL_MESSAGE=cargo run failed"
cargo run -p VKey-rs -- %ARGS% || goto fail
goto done

:headless
if not defined RUST_BACKTRACE set "RUST_BACKTRACE=1"
if not defined RUST_LOG set "RUST_LOG=debug"
if defined ARGS (
    set "ARGS=--headless !ARGS!"
) else (
    set "ARGS=--headless"
)
echo [dev] Starting VKey-rs in headless mode
set "FAIL_MESSAGE=cargo run failed"
cargo run -p VKey-rs -- %ARGS% || goto fail
goto done

:core
echo [dev] Running the platform-independent Vietnamese core CLI
set "FAIL_MESSAGE=cargo run failed"
cargo run -p VKey-core-test -- %ARGS% || goto fail
goto done

:kbd_debug
echo [dev] Running keyboard backend diagnostics
set "FAIL_MESSAGE=cargo run failed"
cargo run -p keyboard-debug -- %ARGS% || goto fail
goto done

:core_debug
echo [dev] Running keyboard/core integration diagnostics
set "FAIL_MESSAGE=cargo run failed"
cargo run -p keyboard-core-debug -- %ARGS% || goto fail
goto done

:test
set "FAIL_MESSAGE=cargo test failed"
cargo test --workspace --all-features --locked %ARGS% || goto fail
goto done

:check
echo [dev] Checking formatting
set "FAIL_MESSAGE=cargo fmt failed"
cargo fmt --all -- --check || goto fail
echo [dev] Checking workspace types
set "FAIL_MESSAGE=cargo check failed"
cargo check --workspace --all-targets --all-features --locked || goto fail
echo [dev] Running Clippy with warnings denied
set "FAIL_MESSAGE=cargo clippy failed"
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings || goto fail
goto done

:all
echo [dev] Checking formatting
set "FAIL_MESSAGE=cargo fmt failed"
cargo fmt --all -- --check || goto fail
echo [dev] Checking workspace types
set "FAIL_MESSAGE=cargo check failed"
cargo check --workspace --all-targets --all-features --locked || goto fail
echo [dev] Running Clippy with warnings denied
set "FAIL_MESSAGE=cargo clippy failed"
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings || goto fail
echo [dev] Running workspace tests
set "FAIL_MESSAGE=cargo test failed"
cargo test --workspace --all-features --locked || goto fail
echo [dev] Building debug workspace
set "FAIL_MESSAGE=cargo build failed"
cargo build --workspace --all-features --locked || goto fail
goto done

:package
call "%ROOT%\build.bat" %ARGS% || goto fail
goto done

:usage
echo Usage: dev.bat [COMMAND] [ARGS...]
echo.
echo Commands:
echo   run [args]       Run the GUI + keyboard service (default: --debug-input)
echo   headless [args]  Run only the keyboard service
echo   core [args]      Run VKey-core-test, e.g. dev.bat core "tieengs Vieejt"
echo   kbd-debug        Run the platform keyboard-event diagnostic
echo   core-debug       Run keyboard ^> Vietnamese core diagnostics
echo   test [args]      Run workspace tests
echo   check            Run fmt, check, and Clippy
echo   all              Run the complete local validation suite and debug build
echo   package [args]   Delegate to build.bat, e.g. dev.bat package --quick
echo   help             Show this help
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
