# agent-tags

Structured annotations for AI coding agents. Embed context directly in source code so agents know what matters before touching a line.

**Docs:** [agenttags.dev](https://agenttags.dev)
**GitHub:** [cabljac/agent-tags](https://github.com/cabljac/agent-tags)

## Install

```bash
npm install -g agent-tags
```

This installs the `git-agent-tags` binary for your platform. Use it as a git subcommand or directly:

```bash
git agent-tags context --for src/auth/token.ts
git agent-tags check
git agent-tags coverage

# or via npx
npx agent-tags context --for src/auth/token.ts
```

## What are agent-tags?

`@agents` tags are structured comments that give AI agents context before modifying code:

```ts
/**
 * @agents(auth-module)
 * OAuth PKCE flow for third-party providers.
 * Related: src/auth/guard.ts#token-validation
 * Don't add session logic here — see session-manager.ts
 */
```

No AST parsing. Any language. Grep-fast.

## License

MIT
