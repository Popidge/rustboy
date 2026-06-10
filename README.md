# Rustboy

Rustboy is a Game Boy emulator written in Rust. It is currently in development and not yet complete. The emulator is capable of running some simple games, but many features are still missing.

This is a learning project for me to improve my Rust skills, agentic development workflows and understand the inner workings of the Game Boy. I am not planning to make this a complete emulator, but rather a fun project to learn from. If you are interested in a more complete emulator, I recommend checking out other projects like [SameBoy](https://sameboy.github.io/) or [BGB](https://bgb.bircd.org/).

## Status

It plays Tetris perfectly with no sound (APU supportcoming soon!). It probably also happily plays other DMG games.

## Agentic Development

Rustboy is built with an agentic-first development workflow in mind - strong in-repo record-keeping via the docs folder, clear signposting via AGENTS.md, and tests that aim to "close the loop" - easily runnable and interpreted by agents. 

AI handles writing most of the code, but the project is heavily driven by me - I didn't just write "codex make me a gameboy emulator in rust, make no mistakes" and walk away!

Most of my work in this will be done via OpenAI's Codex (currently using GPT-5.5)

For more info, see testing strategy - /docs/testing_strategy.md

## Learning-focused architecture, powered by Rust

The architecture of Rustboy is designed to be educational. The code is organized in a way that makes it easy to understand how the Game Boy works. The emulator is divided into several modules, each responsible for a specific part of the Game Boy's functionality. 

Additionally, I make intentional use of Rust's featureset, modelling hardware components as individually-owned structs, and using Rust's ownership and borrow system to enforce the Game Boy's memory model. This is a learning exercise for me to understand how Rust's ownership system can be used to model hardware components.

For more info, see the architecture (/docs/architecture.md) and style guide (/docs/styleguide.md)

## Test ROMs

These are not included in the repo, but can be found at https://github.com/c-sp/game-boy-test-roms - thanks to c-sp (https://github.com/c-sp) for the excellent compilation. 

These serve as a "second compiler", and with the right agent-use harness and tooling, allow for goal-driven loop development targetting specific test ROM suite passage.

## Workflow

I'm currently following the roadmap in /docs/roadmap.md - this is a living document that will be updated as I progress. This often involves implementing the basic shape of a feature, committing in milestones, then using the test ROMs to drive the implementation to completion.

## License

The code and documentation in this repository are licensed under the MIT License. Licenses for crate dependencies are specific to thier respective crates.