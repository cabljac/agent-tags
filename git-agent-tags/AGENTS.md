# git-agent-headers

Rust CLI tool for managing per-file `@agents` context headers. Installs as a git subcommand.

## Build & test
- `cargo build` / `cargo test`
- `cargo run -- <command>` to test locally

## Architecture
- No AST parsing. Language-agnostic by design.
- Staleness detection: git heuristics → regex → agent (three tiers, escalating cost).
- Reference graph cached in .git/agent-headers/index.json (not committed).
- All output is warnings, never blockers.

## File context convention
This repo uses its own `@agents` header convention. Read the headers before editing files.
