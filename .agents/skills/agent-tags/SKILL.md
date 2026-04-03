---
name: agent-tags
description: Write and maintain @agents tags in source code. Use PROACTIVELY — ALWAYS read @agents tags before editing any file in a tagged repo. Use when editing code in a repo with git-agent-tags installed, when asked to add code annotations, when preparing code for other AI agents, or when @agents tags exist in files being modified.
compatibility: Requires git-agent-tags CLI (cargo install). Works in any git repo.
---

# @agents Tags

`@agents` tags are structured comments that give AI agents and humans context before modifying code. They live in source code comments — no AST parsing, any language.

This skill teaches you to read existing tags for context, write new ones, and keep them valid.

## Before editing a file

Read scoped context for the file you're about to modify:

```bash
git agent-tags context --for <file> --hops 1
```

This outputs all `@agents` tags from the file and its immediate neighbors in the reference graph. Read this output and use it to understand constraints, related files, and warnings before making changes.

If `git-agent-tags` is not installed, fall back to scanning the first 30 lines of the file for an `@agents` block and reading it manually.

## After editing files

For each file you meaningfully changed:

1. If the file has an existing `@agents` header, review it — update the body, `Related:` links, and warnings if your changes affect them.
2. If the file has no header and your changes are substantial (new file, major refactor, non-obvious logic), add one.
3. For tricky code locations, add inline `@agents:` tags to explain constraints a future editor would get wrong without the note.

**Always validate before committing:**

```bash
git agent-tags check
```

This is not optional. Fix any broken references or stale warnings before committing. If `check` reports a broken fragment reference, either fix the `Related:` path or update/add the named tag in the target file.

## Tag syntax

### File headers

Place in the first 30 lines inside a comment block. Use the file's native comment style.

```ts
/**
 * @agents
 * OAuth PKCE flow for third-party providers.
 * Related: src/auth/guard.ts#token-validation, src/types/auth.d.ts
 * See: https://datatracker.ietf.org/doc/html/rfc7636
 * Don't add session logic here — see session-manager.ts
 */
```

```python
# @agents
# Retry logic for transient API failures.
# Related: src/api/client.py, src/config/timeouts.py
# Warning: backoff multiplier must stay under 30s or health checks fail.
```

One header per file maximum. The marker is `@agents` (no colon).

### Named headers

```ts
/** @agents(auth-module)
 * OAuth PKCE flow.
 * Related: src/auth/guard.ts#token-validation
 */
```

Names create stable anchors other files can reference via `path/file.ts#auth-module`. Names: alphanumeric, hyphens, underscores only.

### Inline tags

Use `@agents:` (with colon) anywhere in a file. Annotates a specific code location.

```ts
// @agents: Must validate before refresh, not after.
const isValid = checkToken(token);
```

Named inline tags:

```ts
// @agents(token-check): Must validate before refresh.
const isValid = checkToken(token);
```

Continuation: subsequent contiguous comment lines are part of the same tag. A blank line or code ends it.

### Range markers

Use `start` and `end` to define a scoped region:

```ts
// @agents(auth-middleware, start): Validates JWT tokens.
// Related: src/models/user.js#user-model
function validateToken(req, res, next) { /* ... */ }
// @agents(auth-middleware, end)
```

Staleness is scoped to code between the markers. Every `start` must have a matching `end`.

### Fields (file headers only)

| Field | Syntax | Purpose |
|---|---|---|
| Body | Free text | What the file does and why |
| Related | `Related: path/a.ts, path/b.ts` | Repo-root-relative paths to related files |
| See | `See: https://...` | External docs or specs |
| Warnings | `Don't`, `Warning:`, `Note:`, `Avoid:` | Constraints for editors |

Inline tags have free-form text only — no structured fields.

### References

- Paths in `Related:` are always repo-root-relative (not `./relative`)
- Fragment references target named tags: `src/auth/guard.ts#token-validation`
- Fragments are validated — broken ones are errors
- URLs in `See:` are not validated as file references

## Comment styles

Use the file's language:

| Style | Languages |
|---|---|
| `/** */`, `/* */`, `//` | TypeScript, JavaScript, Java, Go, Rust, C, C++, Swift |
| `#` | Python, Ruby, Shell, YAML, TOML |
| `--` | Lua, Haskell |
| `"""` | Python docstrings |

## When to write or update tags

**Always update tags when:**
- You change a function's signature or behavior that other files depend on
- You add a new file that coordinates with existing tagged files
- You add, remove, or rename a file mentioned in a `Related:` link
- You change architectural boundaries (move code between files, extract modules)

**Skip tag updates when:**
- Minor bug fixes that don't change interfaces or relationships
- Cosmetic changes (formatting, renaming local variables)
- Test-only changes

## What makes a good tag

**Do write tags when:**
- The file has non-obvious constraints ("must run before X", "don't use Y here")
- The file coordinates with specific other files that aren't obvious from imports
- There's a "why" that isn't evident from the code itself
- A future editor would likely make a mistake without the context

**Don't write tags when:**
- The code is self-explanatory
- The information is already in the function/variable names
- You're just restating what the code does

**Good tags** are terse, specific, and actionable:
```ts
// @agents: Retry count must stay ≤ 3 or the circuit breaker trips.
```

**Bad tags** are vague or obvious:
```ts
// @agents: This function handles retries.
```

## CLI reference

```bash
git agent-tags context                          # all tags as markdown
git agent-tags context --for <file>             # scoped to file + neighbors
git agent-tags context --for <file> --hops 2    # deeper graph walk
git agent-tags check                            # broken refs + stale headers
git agent-tags check --deep                     # also regex heuristics
git agent-tags status                           # repo health summary
git agent-tags broken                           # broken references only
git agent-tags missing                          # files without headers
git agent-tags suggest                          # suggest Related: from co-changes
git agent-tags graph <file>                     # reference graph for a file
git agent-tags hook --install                   # install pre-commit hook
```

## Gotchas

- The `@agents` marker (no colon) is for file headers. `@agents:` (with colon) is for inline tags. Mixing them up causes parse failures.
- File headers must be in the first 30 lines. A header on line 31 is invisible to the parser.
- `Related:` paths are repo-root-relative. Never use `./` or `../` relative paths.
- Fragment references (`file.ts#name`) are validated. If the target file doesn't have a tag with that name, `git agent-tags check` will report it as broken.
- A file can have at most one header but any number of inline tags.
- Tag names are case-sensitive. `token-check` and `Token-Check` are different names.

For the full specification, read [references/SPEC.md](references/SPEC.md).
