# agent-tags

A comment-based annotation system for source code. `@agents` tags give AI coding agents and humans the context they need before touching code — what a file does, what it relates to, and what constraints apply.

No AST parsing. Works in any language.

## What it looks like

**File header** (top of file):
```ts
/**
 * @agents(auth-module)
 * OAuth PKCE flow for third-party providers.
 * Uses refresh tokens in httpOnly cookies (see cookie-config.ts).
 * Related: src/auth/guard.ts#token-validation, src/types/auth.d.ts
 * Don't add session logic here — see session-manager.ts
 */
```

**Named inline tag** (anywhere in a file):
```ts
// @agents(token-validation): Must validate before refresh, not after.
const isValid = checkToken(token);
```

**Range markers** (scoped region):
```ts
// @agents(auth-middleware, start): Validates JWT tokens.
// Related: src/models/user.js#user-model
function validateToken(req, res, next) { /* ... */ }
// @agents(auth-middleware, end)
```

**Unnamed inline tag**:
```python
# @agents: Must run BEFORE the mutation callback, not after.
```

Named tags are stable anchors — other files reference them with `file.ts#tag-name` in `Related:`, and the reference is validated. Range markers (`start`/`end`) scope staleness detection to the code between them — the markers move with the code through rebases and merges.

## Specification

See [SPEC.md](SPEC.md) for the full tag format: syntax, fields, naming rules, reference format, comment styles, and parsing rules.

## Tooling

### git-agent-tags

A Rust CLI that installs as a git subcommand. Parses `@agents` tags, builds a reference graph, detects stale headers, and outputs context for AI agents.

```bash
# Print all tags (or pipe to an agent)
git agent-tags context

# Scoped context: only tags reachable from a file
git agent-tags context --for src/auth/token.ts

# Check for stale headers and broken references
git agent-tags check

# Pre-commit hook: block on broken refs, warn on staleness
git agent-tags hook --install
```

See [git-agent-tags/README.md](git-agent-tags/README.md) for install instructions and full command reference.

## Dogfooding

This repo uses `@agents` tags on its own source files and runs the pre-commit hook (`git agent-tags hook --install`) to block commits with broken references. It also ships an [Agent Skill](.agents/skills/agent-tags/) so compatible agents (Claude Code, Cursor, Copilot, etc.) can read, write, and maintain tags automatically.
