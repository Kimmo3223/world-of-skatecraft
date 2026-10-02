# Troubleshooting

Start with `setup/setup.sh --doctor`. Setup also writes everything it did to `setup.log`.

## Setup

**"MISSING ..." in the doctor.** Run the `fix:` line printed under it, open a new terminal, run
setup again.

**"Docker running and usable by you" is missing.** Start Docker
(`sudo systemctl enable --now docker`) and add yourself to the `docker` group
(`sudo usermod -aG docker $USER`), then **log out and back in**. On NixOS add your user to
`extraGroups = [ "docker" ]`.

**"Data has no dbc.MPQ".** Point setup at the WoW folder itself (the one containing `Data/`), and
use an English 1.12.1 client. To pick a different folder later: `rm WoW` and run setup again.

**"no default.xex with a data folder beside it".** Skate 3 has to be extracted; an `.iso` doesn't
work. Point setup at the folder that contains both `default.xex` and `data/`. To pick a different
folder later: `rm .setup/skate3-path` and run setup again.

**The Skate 3 conversion failed.** Usually an incomplete extraction: re-extract the game. To redo
the conversion: `rm -rf skate-data` and run setup again.

**Map extraction seems stuck.** It takes hours, and the last two maps (the continents) take most
of it. If a CPU core or two is busy (`top`), it's working.

**"the world server did not come up".** Look at `docker compose -p world-of-skatecraft -f server/compose.yaml logs mangosd`.
If the map extraction was interrupted, delete `server/storage/mangosd/extracted-data/` and run
setup again.

**The build failed.** Read the first `error:` line. Missing `alsa` or `libudev` means the
development packages from the doctor's fix line aren't installed.

## In game

**The login screen has no graphics, or the game says "no WoW install found".** The `WoW` link is
wrong: `ls WoW/Data` should list `.MPQ` files. Fix with `rm WoW` and run setup again.

**Can't log in.** Is the server up? `docker compose -p world-of-skatecraft -f server/compose.yaml ps` should show
`mangosd` and `realmd` as healthy. Reset your password with
`python3 tools/make_account.py <name> <newpassword>`.

**J does nothing.** The log (the terminal you started `./play.sh` from) says why on the line
starting with `skate:`. "no skate-data/assets" means the Skate 3 conversion is missing; run setup
again.

**The board doesn't move.** The engine needs an Xbox-style controller: the log says
`skate: no controller found` if it sees none. Check `ls /dev/input/by-id/*event-joystick`. If
Steam is running with Steam Input, close Steam or turn Steam Input off for the pad.

**No skating sounds.** `skate-audio/` is missing or the sound conversion failed: delete
`skate-audio/` and run setup again.
