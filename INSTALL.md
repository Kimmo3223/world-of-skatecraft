# Installing World of Skatecraft

`setup/setup.sh` (Linux) or `setup.bat` (Windows, experimental) does all of this for you. This page explains what it does and what it asks, so
you know what to expect. If a step fails, setup stops, says what to fix, and continues from that
step when you run it again.

## 1. Before you start

You need:

| | Where it comes from |
|---|---|
| WoW 1.12.1, English, build 5875 | Your own copy. The folder with `WoW.exe` and `Data/` |
| Skate 3 (Xbox 360), extracted | Your own disc or digital copy, extracted so you have `default.xex` and `data/` side by side. For a disc image use [extract-xiso](https://github.com/XboxDev/extract-xiso): `extract-xiso -x "Skate 3.iso"` |
| An Xbox-style controller | Xbox pads on the `xpad` driver work best; Bluetooth and PlayStation pads work too |
| Docker with the compose plugin | Your distribution's packages |
| ~20 GB free | WoW, the server's map data, the build |

Then run `setup/setup.sh --doctor`. It checks everything below and prints the exact install
command for your distribution if something is missing:

- Rust (via [rustup](https://rustup.rs)), a C compiler, pkg-config, ALSA and libudev development
  files, git, curl, Python 3 with venv.
- Docker installed, running, and usable without `sudo` (you are in the `docker` group; log out and
  back in after adding yourself).

On NixOS the scripts enter the repo's `shell.nix` by themselves; you only need Docker enabled in
your system configuration.

## 2. What setup does

1. **Checks your machine** (the doctor above).
2. **Links your WoW client.** It asks for your WoW folder and checks it has the 1.12.1 data
   files. The game reads it directly; nothing in it is changed.
3. **Converts Skate 3 data.** It asks for your extracted Skate 3 folder, downloads the Skate 3
   Rust Engine's converter at a fixed version, and writes the skater, animations and physics
   settings to `skate-data/`. This takes a few minutes.
4. **Converts Skate 3 sounds** into `skate-audio/` (rolling, pops, landings, grinds). It
   downloads vgmstream to decode them. If this fails, skating still works, just silently.
5. **Sets up the local server** ([vmangos](https://github.com/vmangos/core) through
   [vmangos-deploy](https://github.com/mserajnik/vmangos-deploy)) in `server/`, and extracts
   map data from your WoW client. **The extraction takes several hours** and uses a couple of CPU
   cores; leave it running. It only happens once.
6. **Creates your game account.** It asks for a name and a password. The account is a GM.
7. **Builds the game.** The first build takes several minutes. Cargo downloads the skate engine
   from its authors' repository during this step.

Afterwards, start the game with `./play.sh`.

## 3. Where things are

| Folder | What | Safe to delete? |
|---|---|---|
| `WoW` | a link to your WoW folder | yes, setup asks again |
| `skate-data/`, `skate-audio/` | converted from your Skate 3 | yes, setup converts again |
| `server/` | the server, its database (your characters) and map data | only if you want to start over: characters are lost and the maps re-extract for hours |
| `benilla-config/` | game settings, keybindings, addons (`benilla-config/AddOns/`) | yes, settings reset |
| `.setup/`, `setup.log` | setup's tools and log | yes |

## 4. Everyday use

- Play: `./play.sh` (starts the server if it isn't running).
- Stop the server: `docker compose -p world-of-skatecraft -f server/compose.yaml down`.
- Update: `git pull`, then `setup/setup.sh` again (it only rebuilds).
- New account or new password: `python3 tools/make_account.py <name> <password>`.

## 5. Windows (experimental)

The Windows setup does the same steps as the Linux one, but nobody has run it on a real Windows
PC yet. If you try it, please post how it went in Discussions (see the README).

1. Install the tools. In a terminal (PowerShell or Command Prompt):

   ```bat
   winget install -e --id Git.Git
   winget install -e --id Rustlang.Rustup
   winget install -e --id Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
   winget install -e --id Python.Python.3.12
   winget install -e --id Docker.DockerDesktop
   ```

   Restart Windows afterwards, then start Docker Desktop and let it finish starting (it uses
   WSL 2; it will ask to set that up the first time).
2. Clone to a short path (long paths can break the build) and run setup:

   ```bat
   git clone https://github.com/Kimmo3223/world-of-skatecraft.git C:\skatecraft
   cd C:\skatecraft
   setup.bat
   ```

3. Start the game with `play.bat`.

The folders are the same as on Linux, with `WoW` as a junction (a folder link) instead of a
symlink. Stop the server with `docker compose -p world-of-skatecraft -f server\compose.yaml down`.
