"""Shared helpers for applying curated @agents blocks and exporting .tags.diff."""
from __future__ import annotations

import subprocess
import sys
from pathlib import Path

Entry = tuple[str, str, list[str], list[tuple[str, str]], str | None]


def find_insert_line(lines: list[str]) -> int:
    i = 0
    n = len(lines)
    if i < n and lines[i].startswith("#!"):
        i += 1
    while i < n and lines[i].strip() == "":
        i += 1
    if i < n and "coding" in lines[i] and lines[i].lstrip().startswith("#"):
        i += 1
    while i < n and (lines[i].strip() == "" or lines[i].lstrip().startswith("#")):
        i += 1
    if i < n:
        s = lines[i].lstrip()
        if s.startswith('"""') or s.startswith("'''"):
            # SPEC: file headers must be in the first ~30 lines — insert *before*
            # a leading module docstring instead of after a long RST docstring.
            return i
    return i


def format_block(
    name: str, body: list[str], related: list[tuple[str, str]], warning: str | None
) -> str:
    rel = ", ".join(f"{p}#{t}" for p, t in related)
    out = [f"# @agents({name})"]
    for line in body:
        out.append(f"# {line}")
    out.append(f"# Related: {rel}")
    if warning:
        out.append(f"# Warning: {warning}")
    out.append("#")
    return "\n".join(out) + "\n"


def apply_entries(root: Path, agents_md: str, spec: list[Entry]) -> None:
    (root / "AGENTS.md").write_text(agents_md, encoding="utf-8")
    for rel, name, body, related, warn in spec:
        path = root / rel
        if not path.is_file():
            print(f"Missing {path}", file=sys.stderr)
            raise FileNotFoundError(rel)
        text = path.read_text(encoding="utf-8")
        lines = text.splitlines(keepends=True)
        if any("@agents(" in line for line in lines[:40]):
            print(f"Skip (already tagged): {rel}")
            continue
        block = format_block(name, body, related, warn)
        idx = find_insert_line(lines)
        path.write_text("".join(lines[:idx] + [block] + lines[idx:]), encoding="utf-8")
        print(f"Tagged {rel}")


def export_diff(repo: Path, out_path: Path) -> None:
    subprocess.run(["git", "-C", str(repo), "add", "-A"], check=True)
    cached = subprocess.run(
        ["git", "-C", str(repo), "diff", "--cached"],
        capture_output=True,
        text=True,
        check=True,
    )
    unstaged = subprocess.run(
        ["git", "-C", str(repo), "diff"],
        capture_output=True,
        text=True,
        check=True,
    )
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(cached.stdout + unstaged.stdout, encoding="utf-8")
    print(f"Wrote {out_path}")
