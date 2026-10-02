# Contributing

Pull requests are welcome. This is a hobby project, so reviews may take a while.

## Windows support (experimental)

`setup/setup.ps1` (run by `setup.bat`) and `play.bat` mirror the Linux scripts, but were written
without a Windows machine. Fixes from people who run them are very welcome. A Windows pull request
should say what you tested it on (Windows version, GPU, controller) and must not change the Linux
path. The CI's Windows job checks that the game compiles and the script parses.

## Other changes

- Keep changes small and say how you checked them in game.
- No game files, ever: nothing from WoW or Skate 3, and nothing converted from them.
- The WoW client part is [benilla](https://github.com/samwhosung/benilla). Fixes that aren't
  about skating belong upstream there.
- New code is offered under the project's licence (MIT or Apache 2.0); see [NOTICE](NOTICE).
