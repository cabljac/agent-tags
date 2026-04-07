# agent-tags

AI coding agents waste most of their tokens *finding* the right code, not writing it. File localization is the bottleneck — not generation. `@agents` tags fix this by embedding context directly in source code: what a file does, what it relates to, and what constraints apply.

No AST parsing. No infrastructure. Works in any language. Grep-fast.

## The problem

An agent asked to "fix the auth token refresh bug" has to:

1. Guess which files matter (often wrong)
2. Read dozens of files to build context (expensive)
3. Miss non-obvious constraints ("don't cache tokens", "must run before session middleware")
4. Break cross-file dependencies it didn't know about

`@agents` tags short-circuit this. The agent reads structured annotations *in the code itself* and immediately knows what the file does, what it depends on, and what not to break.

## What it looks like

**File header** — describes a file's purpose and relationships:

```ts
/**
 * @agents(auth-module)
 * OAuth PKCE flow for third-party providers.
 * Uses refresh tokens in httpOnly cookies (see cookie-config.ts).
 * Related: src/auth/guard.ts#token-validation, src/types/auth.d.ts
 * Don't add session logic here — see session-manager.ts
 */
```

**Inline tag** — annotates a specific code location:

```ts
// @agents(token-check): Must validate before refresh, not after.
const isValid = checkToken(token);
```

**Range markers** — scopes a region of code:

```ts
// @agents(auth-middleware, start): Validates JWT tokens.
// Related: src/models/user.js#user-model
function validateToken(req, res, next) { /* ... */ }
// @agents(auth-middleware, end)
```

Named tags are stable anchors — other files reference them with `file.ts#tag-name`, and the CLI validates every reference.

## How agents use it

The repo ships an [agent skill](.agents/skills/agent-tags) that teaches compatible agents (Claude Code, Cursor, Copilot, etc.) a simple protocol:

**Before editing a file:**

```bash
git agent-tags context --for src/auth/token.ts --hops 1
```

The agent reads all tags from the file and its neighbors in the reference graph — constraints, relationships, warnings — before making any changes.

**After editing files:**

```bash
git agent-tags check
```

The agent validates that no references are broken and no headers are stale. If something's wrong, it fixes it before committing.

This turns tags into a self-maintaining system: agents read tags for context, then update them as part of every edit.

## Getting started

### Install the CLI

```bash
# From source
git clone https://github.com/cabljac/agent-tags.git
cd agent-tags/git-agent-tags
cargo install --path .
```

This installs `git-agent-tags` as a git subcommand.

### Tag your repo

See what's untagged:

```bash
git agent-tags missing
```

Add headers to important files, run `git agent-tags check` to validate, and install the pre-commit hook to keep things clean:

```bash
git agent-tags hook --install
```

### Give your agent the skill

Copy [`.agents/skills/agent-tags`](.agents/skills/agent-tags) into your repo. Agents that support skills will automatically read tags before editing and validate after.

## CLI reference

```bash
git agent-tags context                          # all tags as markdown
git agent-tags context --for <file>             # scoped to file + neighbors
git agent-tags context --for <file> --hops 2    # deeper graph walk
git agent-tags check                            # broken refs + stale headers
git agent-tags check --deep                     # regex heuristics too
git agent-tags status                           # repo health summary
git agent-tags missing                          # files without headers
git agent-tags suggest                          # suggest Related: from co-changes
git agent-tags graph <file>                     # reference graph for a file
git agent-tags coverage                         # file + line coverage metrics
git agent-tags coverage --json                  # machine-readable coverage
git agent-tags index                            # build SQLite index (.git/agent-tags/tags.db)
git agent-tags index --force                    # rebuild from scratch
git agent-tags index --path                     # print database path
git agent-tags hook --install                   # install pre-commit hook
```

### SQLite index

Build a queryable index of all tags, relationships, and coverage:

```bash
git agent-tags index
# ✓ Index built: 34 files, 8 headers, 12 inline tags, 10 edges
#   Database: .git/agent-tags/tags.db (28 KB)
```

The database includes FTS5 full-text search over tag content, a reference graph via foreign keys, and per-file coverage stats. Query it with the built-in `query` command:

```bash
git agent-tags query search "auth"              # full-text search across all tags
git agent-tags query deps <file>                # what does this file depend on?
git agent-tags query rdeps <file>               # what depends on this file?
git agent-tags query file <file>                # all tags for a specific file
git agent-tags query uncovered                  # files with no tags, by size
git agent-tags query sql "SELECT ..."           # arbitrary read-only SQL
git agent-tags query --json search "auth"       # JSON output for any subcommand
```

## Specification

See [SPEC.md](SPEC.md) for the full tag format: syntax, fields, naming rules, reference format, comment styles, and parsing rules.

## Why not just use AGENTS.md?

`AGENTS.md` is great for repo-wide instructions. But it's a single file — it can't express per-file constraints, cross-file relationships, or scoped warnings. `@agents` tags are the per-file extension: they live where the code lives, travel through git, and get validated automatically.

## Why not tree-sitter / code indexing?

Tree-sitter tells you what the code *is* (functions, classes, symbols). `@agents` tags tell you what the code *means* (purpose, constraints, relationships). They're complementary — but tags require zero infrastructure and are instantly searchable with grep. The SQLite index adds optional structured queries, but it builds in milliseconds from a simple regex scan — no AST parsing needed.

## License

MIT
