#!/usr/bin/env node

const { spawnSync } = require("child_process");

function getBinaryPath() {
  const platform = process.platform;
  const arch = process.arch;
  const os = platform === "win32" ? "win32" : platform;
  const ext = platform === "win32" ? ".exe" : "";

  try {
    return require.resolve(
      `agent-tags-${os}-${arch}/bin/git-agent-tags${ext}`
    );
  } catch {
    // Fallback: check if git-agent-tags is on PATH (cargo install)
    try {
      const { execFileSync } = require("child_process");
      return execFileSync("which", ["git-agent-tags"], {
        encoding: "utf-8",
      }).trim();
    } catch {}

    console.error(
      `\nagent-tags: no binary found for ${os}-${arch}.\n\n` +
        `Install from source:\n\n` +
        `  git clone https://github.com/cabljac/agent-tags.git\n` +
        `  cd agent-tags/git-agent-tags\n` +
        `  cargo install --path .\n\n` +
        `Docs: https://agenttags.dev\n`
    );
    process.exit(1);
  }
}

const result = spawnSync(getBinaryPath(), process.argv.slice(2), {
  stdio: "inherit",
});
process.exit(result.status ?? 0);
