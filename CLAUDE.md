# Zeph

A CLI tool for inspecting zarr data stores.

## Tech Stack

- **Rust** (edition 2021)
- **crossterm** — terminal styling, raw mode for interactive widgets
- **clap** — CLI argument parsing
- **anyhow** — error handling
- **zarrs** / **zarrs_object_store** / **zarrs_storage** — zarr format reading
- **object_store** (aws, gcp, azure, http) — cloud storage backends
- **tokio** — async runtime
- **serde** / **serde_json** — serialization

## Architecture

Zeph is a **hybrid scrolling terminal app**, not a full-screen TUI:
- Normal output (welcome box, command results) prints to stdout with crossterm styling and scrolls naturally
- Interactive moments (autocomplete dropdown, key capture) use crossterm raw mode briefly, then return to normal mode
- Always restore terminal state via `RawModeGuard` (Drop impl) and a panic hook

## Build & Run

```
cargo run                    # start zeph in current directory
cargo run -- <path>          # start zeph with a zarr store path
```

## Testing

After making changes, always run the tests to check for regressions:

```
cargo test
```

## Linting & Formatting

CI enforces these checks — run before committing:

```
cargo fmt --check                # check formatting (cargo fmt to fix)
cargo clippy -- -D warnings      # lint
```

## Project Structure

```
src/
  main.rs                    — entry point, CLI parsing, panic hook
  lib.rs                     — public module re-exports (zarr)
  cli.rs                     — clap derive struct
  ui/                        — output rendering (non-interactive)
  commands/                  — command registry and handlers
  repl/                      — interactive input loop
  zarr/                      — zarr store access, metadata parsing, cloud backends
```
