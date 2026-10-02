# World of Skatecraft

Skate 3 skateboarding in World of Warcraft 1.12.1. Press **J** and your character drops onto a
board, with Skate 3's own physics: flick-it tricks, grinds on any ledge, powerslides and bails,
anywhere in Azeroth. Your own character does the tricks, in your own gear.

Built on [benilla](https://github.com/samwhosung/benilla), a from-scratch WoW 1.12.1 client in
Rust, and the [Skate 3 Rust Engine](https://github.com/SK8-ENGINE/skate-3-rust-engine). It runs
fully offline against a local server.

> **Unofficial fan project.** Not affiliated with Blizzard Entertainment or Electronic Arts.
> **No game files are included.** You need your own copies of both games.

> **Vibe coded, just for laughs.** This whole thing was put together with an AI coding assistant
> (Claude Code) for fun, on top of other people's serious work (see Credits). Expect rough edges,
> weird bugs and a skater who doesn't always know where his sword goes. Don't take it seriously.

## What you need

- **Linux**, or **Windows (experimental, testers wanted)**: the Windows setup is new and untested
  on real machines; please report how it goes (see Help).
- **World of Warcraft 1.12.1**, English client (build 5875).
- **Skate 3 for Xbox 360, extracted**: the folder with `default.xex` and `data/` (an ISO file
  doesn't work; extract it first).
- **An Xbox-style controller.**
- **Docker**, about **20 GB** free disk, and a few hours once for the server's map extraction.

## Install

Linux:

```sh
git clone https://github.com/Kimmo3223/world-of-skatecraft.git
cd world-of-skatecraft
setup/setup.sh
```

Windows (experimental): install [Git](https://git-scm.com), then in a terminal:

```bat
git clone https://github.com/Kimmo3223/world-of-skatecraft.git C:\skatecraft
cd C:\skatecraft
setup.bat
```

Setup checks your machine and tells you exactly what to install if something is missing, asks
for your WoW and Skate 3 folders, converts what it needs from them, sets up the local server,
creates your game account and builds the game. Step-by-step details: [INSTALL.md](INSTALL.md).

## Play

```sh
./play.sh        # Linux
play.bat         # Windows
```

Log in with the account you made during setup and create a character.

| Key | |
|---|---|
| **J** | hop on / off the board |
| **K** | switch between the WoW camera and the Skate 3 camera |
| controller | skate (Skate 3 controls) |

Your account is a GM, so chat commands like `.tele stormwind`, `.levelup 59` and
`.additem <id>` work.

## Help

This is a hobby project with **best-effort help only**. If something breaks:

1. Run the doctor (`setup/setup.sh --doctor`, or on Windows
   `powershell -ExecutionPolicy Bypass -File setup\setup.ps1 -Doctor`) and read
   [TROUBLESHOOTING.md](TROUBLESHOOTING.md).
2. Still stuck? Ask in [Discussions](../../discussions) (Q&A) and attach your `setup.log` and the
   doctor output. Questions without them may go unanswered.

**Windows testers:** whether it works or not, a post in Discussions with your Windows version,
GPU, controller and `setup.log` helps a lot.

## Credits

- **[benilla](https://github.com/samwhosung/benilla)** by samwhosung and contributors: the WoW
  1.12.1 client this is built on. Its own README is in [docs/BENILLA.md](docs/BENILLA.md).
- **[Skate 3 Rust Engine](https://github.com/SK8-ENGINE/skate-3-rust-engine)** by SK8-ENGINE /
  Chasm: the skateboarding, built on **dumbad**'s years of Skate 3 reverse engineering
  ([DumbadsSkate3ModdingTools](https://github.com/Ethanw05/DumbadsSkate3ModdingTools)).
- **[2010 Rust Rewrite Mashup](https://github.com/chasmlol/2010-rust-rewrite-mashup)** by chasmlol,
  on [IW4L](https://github.com/vladtrc/iw4L) by vladtrc: the Skate 3 mode for MW2 this one
  follows.
- **[vmangos](https://github.com/vmangos/core)** and
  **[vmangos-deploy](https://github.com/mserajnik/vmangos-deploy)** (mserajnik): the local server.
  **[vgmstream](https://vgmstream.org)**: decodes the Skate 3 sounds during setup.

## Legal

The code here is benilla's licence, MIT or Apache 2.0 at your option, except where a file says
otherwise; see [NOTICE](NOTICE). The skate engine is downloaded from its authors' repository at
build time and is not part of this repository. No World of Warcraft or Skate 3 content is
included or distributed: the game reads your WoW install, and setup converts your own Skate 3
files on your machine. World of Warcraft is a trademark of Blizzard Entertainment, Inc.; Skate is
a trademark of Electronic Arts Inc.
