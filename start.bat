@echo off
REM ==============================================================================
REM FMLE by Vanaila - 1-Click Automated Setup & Launch Script (Windows 10 / 11)
REM Real-Time Memory Editor, Scouting Suite & Tactical Toolkit for Football Manager 26
REM ==============================================================================

TITLE FMLE by Vanaila - Football Manager 26 Live Editor

echo ==============================================================================
echo              FMLE by Vanaila - Football Manager 26 Live Editor
echo        100%% Free & Open-Source • Built with love for FM Community
echo ==============================================================================
echo.

cd /d "%~dp0"

REM Check Rust toolchain
where cargo >nul 2>nul
if %ERRORLEVEL% NEQ 0 (
    echo [ERROR] Rust toolchain (cargo) is not detected!
    echo Please install Rust from https://rustup.rs and run this script again.
    pause
    exit /b 1
)

echo [OK] Rust toolchain detected.
echo [INFO] Starting background browser launcher for http://localhost:41789...

start "" cmd /c "timeout /t 3 /nobreak >nul && start http://localhost:41789"

echo [INFO] Compiling and starting FMLE release daemon on port 41789...
cargo run -p fmlev-cli --release -- serve --port 41789

pause
