# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run Commands

```bash
cargo build                    # Debug build
cargo build --release          # Release build
cargo run                      # Run the game (reads from stdin for player actions)
RUST_LOG=debug cargo run       # Run with debug logging
RUST_LOG=info cargo run        # Run with info logging
cargo test                     # Run tests (no tests exist yet)
cargo clippy                   # Lint
```

## Project Overview

A Rust-based collectible card game (CCG) engine. Card definitions are written in Lua and loaded at runtime via `mlua`. The game runs as a two-player CLI application with stdin-based input.

## Architecture

### Module Dependency Flow

```
main.rs → LuaApi + card_loader + desk_loader → Game::new() → Game::run()
```

- **game.rs** (~1050 lines) — Core game loop and all phase logic. Contains `Game`, `GameState`, and `Zone` types. This is the largest and most critical file.
- **card.rs** — `Card` (instance) and `CardInfo` (definition/template) with `CardInfoBuilder` for fluent construction.
- **effect.rs** — Effect system: `Effect`, `DoEffect`, `Action`, `WindowsTag`. Effects are resolved via a `VecDeque<DoEffect>` queue.
- **lua_api.rs** — Bridge between Lua card scripts and Rust. `LuaApi` holds all loaded `CardInfo` definitions.
- **card_loader.rs** — Reads `cards/*.lua` files and registers them into `LuaApi`.
- **desk_loader.rs** — Reads `desks/*` files (plain text, one card ID per line) to build deck lists.
- **command_reader.rs** — `ReadPlayerActions` trait and `CommandReader` for stdin-based player input.
- **player_actions.rs** — `PlayerAction` enum defining all possible player choices.
- **targeting.rs** — `Targeting` enum for effect target resolution (player or card).
- **window_event.rs** — Game event triggers (placement, attack, turn start, etc.).
- **game_diff.rs** / **choice_req.rs** / **choice_res.rs** — Scaffolding for future networking/replay.
- **common.rs** — Shared type aliases (`EntryId`, etc.).

### Game Phase Loop

Each turn cycles through 7 phases: Start → Draw → Reuse → Main → Fight → Main2 → End. Phase logic is centralized in `game.rs`.

### Card Definition (Lua)

Cards are defined in `cards/` as Lua scripts using `define_card()`. Example:
```lua
define_card("S000-A-001", function(card)
    card:name("测试卡001")
    card:cost(2)
    card:ack(100)
    card:reg_effect("e1", function(effect)
        effect:window("set")   -- triggers on placement
        effect:draw(1)         -- draw 1 card
    end)
end)
```

Decks are defined in `desks/` as plain text files with one card ID per line (40 cards).

### Key Game Concepts

- **Zones**: Each player has 4 front zones (combat) and 4 back zones, plus hand, deck, cost area, and grave.
- **Cost system**: Playing a card costs N hand cards moved to the cost area. `RealPoint` can substitute. Cost-area cards remain usable.
- **Combat**: Front-zone cards attack opponent's front-zone cards (or player directly). All combat outcomes grant 1 RealPoint.
- **Reuse phase**: Recover cards from cost area back to hand. Recovery count = opponent's highest-cost card on field.

## Code Conventions

- Comments and domain terms are in Chinese.
- Commit messages follow: `dev(feature)-description` or `doc description`.
- Rust 2024 edition.
- Logging via `log` crate macros (`info!`, `debug!`, etc.) with `env_logger`.
