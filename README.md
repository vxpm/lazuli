<h1>
  <img align="left" width="32" height="32" style="margin-bottom: -8px" alt="lazuli" src="./resources/logo_256.png" />
  lazuli
</h1>

A work-in-progress GameCube emulator :)

- [Status](#status)
- [Building](#building)
- [Usage](#usage)
- [Contributing](#contributing)
- [Random Q&A](#random-qa)
- [Licensing](#licensing)

# Status

`lazuli` is still very much a toy, but it's able to boot multiple commercial games and lots of
homebrew. Here's a very small list of games that are frequently tested and go in game with decent graphics:

- Super Mario Sunshine
- Luigi's Mansion
- The Legend of Zelda: The Wind Waker
- Crash Bandicoot: The Wrath of Cortex
- WarioWare, Inc.: Mega Party Game$!

It's worth noting that these are _not_ the only games that work. Other games might or might not work,
most are untested.

On a more technical note, here's what `lazuli` currently offers:

- `cranelift` based PowerPC JIT compiler
- `cranelift` based JIT vertex parser compiler
- DSP LLE interpreter
- `wgpu` based renderer backend
- `cpal` based audio backend
- `wesl` based shader generator/compiler
- IPL HLE used to boot games without an IPL ROM
- A very simple UI for debugging

Performance is okayish. The biggest bottleneck _by far_ is the DSP LLE interpreter. A JIT is planned,
but not before most bugs in the interpreter are fixed (audio works fine in some games, and is completely
broken in others - mostly ones that use ADPCM).

# Building

To build lazuli, you'll need the latest nightly rust toolchain (which can be obtained through `rustup`)
and the `just` command runner.

First, run `just ipl-hle build` to build the ipl-hle binary, which is embedded into the lazuli executable.
This should generate `ipl-hle.dol` inside a `local/` directory in the workspace.

Then, build the main lazuli app by executing `cargo build` (with any optional flags you might want,
such as `--release`). This should produce an `app` executable inside `target/chosen_profile`.

# Usage

## Running a game

Once you have a `lazuli` executable (either by building it or by grabbing one of the nightly releases),
you can run it in the terminal with a path to the ROM you want to run (supports `.iso` and `.rvz`):

```sh
lazuli --rom path/to/gamecube/game.iso
```

You do not need an IPL ROM (the "bios") to run games, as game loading is HLEd by `lazuli`. However,
some games might use IPL's embedded font (in which case the game might not even boot without it).
To pass an IPL:

```sh
lazuli --ipl path/to/ipl.bin --rom path/to/gamecube/game.iso
```

You can also pass `--ipl-lle` to skip the high-level emulation of the IPL (IPL-HLE) and instead
use the provided IPL ROM to boot. This will take you to the system menu, from where you can boot 
the game.

For more CLI options, `--help` is your friend.

## Inputs

Both gamepads and keyboard input are supported. When a gamepad is detected, it is automatically set
as the active input source - otherwise, the keyboard will be used. Mappings cannot be customized yet.

Keyboard Mappings:
- Left Analog: W A S D
- Right Analog (C): H J K L
- A B: B N
- X Y: C V
- Z: R
- Start: Space
- D-Pad: Arrows
- Left Trigger: Q/T
- Right Trigger: E/Y

## Debugging

The UI has many features that are useful for debugging. With it, you can set breakpoints, watch memory
variables, analyze call stacks and more. To open windows, click the `view` button in the top-left corner
of the screen (it's in the top bar).

# Contributing

Contributions are very welcome! You do not need to be an expert on the GameCube's internals to contribute,
there's multiple other ways you could help:

- Improving UI
- Optimizing performance
- Fixing bugs
- Documenting stuff
- And more!

If you're interested, **please** read [the contribution guidelines](./CONTRIBUTING.md) before getting
started.

# Random Q&A

Here's some random questions and their answers. I'd call this a FAQ but no one has ever asked these
questions so I'm not sure it would be appropriate :p

## Is there any reason I should use this over Dolphin?

No, not yet. Dolphin is a thousand times more mature and what you should use if you want to actually
play games.

## Is this a reimplementation of Dolphin in Rust?

No, this is built from the ground up. No dolphin code is reused/stolen/whatever.

## Does this support Wii?

Not yet. It's a long-term goal, since the Wii is very similar to the GameCube. There's currently no
infrastructure for it, though.

## What is `hemisphere`?

The old name of this project. I renamed it to `lazuli` because it's cute.

# Licensing

Most of the emulator is licensed under GPLv3, but some library crates are licensed under MIT instead.
Check the `license` property of the `Cargo.toml` of each crate to verify it's license. The license
text can be found under the `licenses/` directory.
