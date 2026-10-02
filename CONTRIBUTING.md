# Contributing

Pull requests are welcome. This is a hobby project, so reviews may take a while.

## Windows support (wanted)

World of Skatecraft only has a Linux setup today. Most pieces already work on Windows; what is
missing is a `setup/setup.ps1` (and a `play.bat`) that does what `setup/setup.sh` does:

- benilla builds on Windows (MSVC build tools from the Rust installer).
- The skate engine reads Xbox controllers through XInput on Windows.
- The server runs in Docker Desktop; vmangos-deploy supports Windows hosts.
- `tools/skate_audio.py`, `tools/eb_extract.py` and `tools/make_account.py` are plain Python;
  vgmstream has a Windows build.
- The Skate 3 converter is the same Python as on Linux; the native RefPack decoder builds with
  `rustc` into a real `.dll`.

A Windows pull request should say what you tested it on (Windows version, GPU, controller) and
must not change the Linux path.

## Other changes

- Keep changes small and say how you checked them in game.
- No game files, ever: nothing from WoW or Skate 3, and nothing converted from them.
- The WoW client part is [benilla](https://github.com/samwhosung/benilla). Fixes that aren't
  about skating belong upstream there.
- New code is offered under the project's licence (MIT or Apache 2.0); see [NOTICE](NOTICE).
