@echo off
rem World of Skatecraft setup (Windows, experimental). See INSTALL.md.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0setup\setup.ps1" %*
pause
