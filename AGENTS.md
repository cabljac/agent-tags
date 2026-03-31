# Agent Tags

A comment-based annotation system (`@agents` tags) for source code. Language-agnostic, no AST parsing. Tags live in comments and provide structured context about files and code locations for AI coding agents and humans.

## Repository structure

- `SPEC.md` — full specification for the `@agents` tag format (syntax, fields, naming, references, parsing rules)
- `README.md` — project overview and usage examples
- `git-agent-tags/` — Rust CLI tool that installs as a `git agent-tags` subcommand

## The CLI tool (`git-agent-tags/`)

Rust project built with Cargo. Parses `@agents` tags from source files, builds a reference graph, detects stale headers, and outputs context.

Key source files:
- `src/main.rs` — CLI entry point and subcommand dispatch
- `src/parser.rs` — tag parsing (file headers and inline tags)
- `src/graph.rs` — reference graph construction
- `src/check.rs` — validation (broken references, stale headers)
- `src/cache.rs` — index caching in `.git/agent-headers/`
- `src/config.rs` — configuration
- `src/git.rs` — git integration helpers

Build and test: `cd git-agent-tags && cargo build && cargo test`

## Spec compliance

All parsing and output behavior must match `SPEC.md`. When in doubt, the spec is the source of truth. Key rules:
- File headers must appear in the first 30 lines
- Inline tags use `@agents:` (with colon), headers use `@agents` (without)
- `Related:` paths are repo-root-relative
- Fragment references (`file.ts#tag-name`) are validated
- Output format is Markdown sections sorted by path then line number
