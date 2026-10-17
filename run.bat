@echo off
cd /d "%~dp0"
cargo build --release
if errorlevel 1 exit /b 1
start "" "%~dp0target\release\ModDB.exe"
