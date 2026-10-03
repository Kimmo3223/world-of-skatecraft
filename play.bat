@echo off
rem Starts the local server (if it is not running) and the game. Run setup.bat first.
rem In game: J summons/dismisses the skateboard (Xbox-style controller), K swaps the skate camera.
cd /d "%~dp0"
if not exist target\release\benilla.exe (
    echo Not built yet: run setup.bat first.
    pause
    exit /b 1
)
if "%SKATECRAFT_SERVER_PROJECT%"=="" set SKATECRAFT_SERVER_PROJECT=world-of-skatecraft
docker compose -p %SKATECRAFT_SERVER_PROJECT% --project-directory server -f server\compose.yaml up -d
set WOW_HOST=localhost
set WOW_SKATE_ASSETS=%~dp0skate-data\assets
set WOW_SKATE_AUDIO=%~dp0skate-audio
target\release\benilla.exe %*
