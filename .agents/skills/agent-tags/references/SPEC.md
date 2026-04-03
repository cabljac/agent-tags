# @agents Tag Specification

Version 0.1

## Overview

`@agents` is a comment-based annotation system for source code. Tags provide structured context about files and code locations — what the code does, what it relates to, and what constraints apply — so that AI coding agents and human readers can understand intent before making changes.

Tags are language-agnostic. They live inside comments and require no AST parsing.

## Tag forms

There are two forms: **file headers** and **inline tags**.

### File headers

A file header is a comment block in the first 30 lines of a file containing the marker `@agents` (without a colon). It describes the file as a whole.

```ts
/**
 * @agents
 * OAuth PKCE flow for third-party providers.
 * Related: src/auth/guard.ts, src/types/auth.d.ts
 * Don't add session logic here — see session-manager.ts
 */
```

A file may have at most one header.

### Inline tags

An inline tag is a comment containing `@agents:` (with a colon) anywhere in the file. It annotates a specific location.

```ts
// @agents: Query key must include documentRef.path, not just id.
const queryKey = [ref.firestore, ref.path];
```

Subsequent contiguous comment lines are captured as continuation text:

```ts
// @agents: First line of the annotation.
// This continues the same tag.
// So does this.
```

A blank line or non-comment line ends the continuation. A new `@agents:` on the next line starts a separate tag.

A file may have any number of inline tags.

## Named tags

Either form can include a name in parentheses:

```ts
/** @agents(auth-module)          — named file header */
// @agents(token-check): ...      — named inline tag
```

Names create stable anchors that other files can reference via fragment syntax (see References below). Names are optional — unnamed tags work identically but cannot be targeted by fragment references.

### Name rules

A tag name must:
- Contain only alphanumeric characters, hyphens (`-`), and underscores (`_`)
- Be non-empty

Names are case-sensitive. `token-check` and `Token-Check` are different names.

### Range markers

A named inline tag can use `start` and `end` markers to define a region of code:

```ts
// @agents(auth-middleware, start): Validates JWT tokens, attaches user context.
// Related: src/models/user.js#user-model
// Don't cache tokens. Must run before any route handler.
function validateToken(req, res, next) {
  const token = req.headers.authorization?.split(' ')[1];
  if (!token) return next(new AuthError('No token provided'));
  try {
    req.user = jwt.verify(token, process.env.JWT_SECRET);
    next();
  } catch (err) { next(new AuthError('Invalid token')); }
}
// @agents(auth-middleware, end)
```

The `start` marker opens a named region. It uses inline tag syntax (`@agents(name, start):` with a colon) and may include body text, `Related:`, and warnings. The `end` marker closes the region (`@agents(name, end)` — no colon, no body).

Staleness detection is scoped to the code between the markers. If the code inside the region changes without the start marker being updated, it is flagged as stale. The markers move with the code through rebases, cherry-picks, and merges.

Every `start` must have a matching `end` with the same name in the same file. Unpaired markers are validation errors. Range markers are for inline tags only — file headers do not use them.

## Fields

File header bodies support the following structured fields. All are optional.

| Field | Syntax | Purpose |
|---|---|---|
| Body | Free text lines | What the file does and why |
| Related | `Related: path/a.ts, path/b.ts` | Files this one depends on or coordinates with |
| See | `See: https://...` | External docs, specs, or resources |
| Warnings | Lines starting with `Don't`, `Warning:`, `Note:`, or `Avoid:` | Constraints for anyone editing |

Any line that doesn't match a field prefix is treated as body text.

Inline tags have free-form text only — they do not support `Related:`, `See:`, or warning fields.

## References

### File references

Paths in `Related:` are repo-root-relative:

```
Related: src/auth/guard.ts, src/types/auth.d.ts
```

Not `./guard.ts` or `../types/auth.d.ts`.

### Fragment references

A `#name` fragment appended to a path targets a specific named tag within that file:

```
Related: src/auth/guard.ts#token-validation
```

This means "the tag named `token-validation` in `src/auth/guard.ts`." The fragment is validated — if the target file has no tag with that name, the reference is considered broken.

References without fragments point to the file as a whole.

### URLs

`See:` fields may contain URLs. URLs (starting with `http://` or `https://`) are not validated as file references.

```
See: https://datatracker.ietf.org/doc/html/rfc7636
```

## Comment styles

Tags are recognized inside any of these comment syntaxes:

| Style | Languages |
|---|---|
| `/** */`, `/* */`, `//` | TypeScript, JavaScript, Java, Kotlin, Scala, Go, Rust, C, C++, C#, Swift |
| `#` | Python, Ruby, Shell, Bash, YAML, TOML, Elixir |
| `--` | Lua, Haskell |
| `"""` / `'''` | Python docstrings |

For languages not listed, `#` style is assumed.

## Parsing rules

1. **File headers**: scan the first 30 lines for the marker `@agents` or `@agents(name)` inside a comment. If found, the entire enclosing comment block is the header.

2. **Inline tags**: scan the entire file for `@agents:`, `@agents(name):`, `@agents(name, start):`, or `@agents(name, end)` inside comments. Lines within the file header range are excluded from inline scanning.

3. **Comment stripping**: the parser strips comment prefixes (`//`, `#`, `--`, `*`) to extract the inner text. No language-specific AST parsing is performed.

4. **Multiple tags per file**: a file may have zero or one file header and zero or more inline tags.

## Output format

When tags are rendered (e.g. by `git agent-tags context`), each tag becomes a Markdown section:

```markdown
## path/to/file.ts#tag-name
Body text here.
Related: other/file.ts#other-tag

## path/to/file.ts:42#inline-name
Inline annotation text.
```

- File headers: `## <path>` or `## <path>#<name>`
- Inline tags: `## <path>:<line>` or `## <path>:<line>#<name>`
- Tags are sorted by file path, then line number
