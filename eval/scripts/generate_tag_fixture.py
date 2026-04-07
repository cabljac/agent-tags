#!/usr/bin/env python3
"""
Generate eval/tasks/<instance_id>.tags.diff by cloning the task repo at base_commit,
injecting @agents headers into each file touched by the ground-truth patch, and
adding AGENTS.md. Intended for bootstrapping eval fixtures; review diffs before commit.

Usage:
  python3 eval/scripts/generate_tag_fixture.py astropy__astropy-13398
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path


def ground_truth_files(patch: str) -> list[str]:
    return list(
        dict.fromkeys(re.findall(r"^diff --git a/(.*?) b/", patch, re.MULTILINE))
    )


def make_tag_name(path: str) -> str:
    """Stable, unique-ish slug from path (alphanumeric, hyphens)."""
    stem = path
    for suf in (".py", ".pyi", ".rst", ".txt"):
        if stem.endswith(suf):
            stem = stem[: -len(suf)]
            break
    parts = stem.split("/")
    slug = "-".join(parts[-4:]).replace("_", "-")
    slug = re.sub(r"[^a-zA-Z0-9_-]+", "-", slug)
    slug = re.sub(r"-+", "-", slug).strip("-").lower()
    return slug or "module"


def find_insert_line(lines: list[str]) -> int:
    """Insert after leading shebang, encoding comment, and # comment / blank block."""
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
    # Skip a module docstring if present
    if i < n:
        s = lines[i].lstrip()
        if s.startswith('"""') or s.startswith("'''"):
            return i
    return i


def build_header_block(
    tag: str, description: str, related: list[tuple[str, str]], is_python: bool
) -> str:
    rel = ", ".join(f"{p}#{t}" for p, t in related)
    if is_python:
        body = f"# @agents({tag})\n# {description}\n# Related: {rel}\n#\n"
    else:
        body = f"# @agents({tag})\n# {description}\n# Related: {rel}\n#\n"
    return body


def clone_repo(repo: str, dest: Path, base_commit: str) -> None:
    if dest.exists():
        subprocess.run(
            ["git", "-C", str(dest), "fetch", "--quiet", "origin"],
            check=False,
        )
    else:
        dest.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(
            [
                "git",
                "clone",
                "--quiet",
                f"https://github.com/{repo}.git",
                str(dest),
            ],
            check=True,
        )
    subprocess.run(
        ["git", "-C", str(dest), "checkout", "--quiet", base_commit],
        check=True,
    )
    subprocess.run(
        ["git", "-C", str(dest), "reset", "--hard", "--quiet", base_commit],
        check=True,
    )
    subprocess.run(
        ["git", "-C", str(dest), "clean", "-fdq"],
        check=True,
    )


AGENTS_MD = """# Agent Context

This codebase uses `@agents` tags — structured comments in source files that describe what each file does and how it relates to others.

## Reading tags

Look for `@agents(name)` in the first few lines of source files. Example:

```python
# @agents(example-module)
# Short description of the module's role.
# Related: path/to/other.py#other-module
```

- **Body**: what the file does
- **Related**: repo-root-relative paths to related files. `#name` fragments point to specific tagged files.
- **Warnings**: lines starting with `Don't`, `Warning:`, `Note:`, or `Avoid:`

## Before editing a file

Read the `@agents` header (if present) to understand the file's role and what other files it coordinates with. Follow `Related:` links to understand the dependency chain.
"""


def inject_into_file(
    root: Path,
    rel: str,
    tag: str,
    description: str,
    related: list[tuple[str, str]],
) -> bool:
    path = root / rel
    if not path.is_file():
        print(f"  skip (not at base commit): {rel}", file=sys.stderr)
        return False
    text = path.read_text(encoding="utf-8", errors="replace")
    lines = text.splitlines(keepends=True)
    is_py = rel.endswith(".py")
    block = build_header_block(tag, description, related, is_py)
    idx = find_insert_line(lines)
    # Avoid double-inject
    if any("@agents(" in line for line in lines[: min(20, len(lines))]):
        return True
    new_lines = lines[:idx] + [block] + lines[idx:]
    path.write_text("".join(new_lines), encoding="utf-8")
    return True


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("instance_id")
    ap.add_argument(
        "--eval-root",
        type=Path,
        default=Path(__file__).resolve().parent.parent,
        help="eval/ directory (contains tasks/)",
    )
    args = ap.parse_args()
    eval_dir: Path = args.eval_root
    task_json = eval_dir / "tasks" / f"{args.instance_id}.json"
    if not task_json.is_file():
        print(f"Missing {task_json}", file=sys.stderr)
        return 1
    data = json.loads(task_json.read_text())
    repo = data["repo"]
    base_commit = data["base_commit"]
    patch = data.get("patch") or ""
    files = ground_truth_files(patch)
    if not files:
        print("No files in patch", file=sys.stderr)
        return 1

    tags: dict[str, str] = {}
    used: set[str] = set()
    for f in files:
        base = make_tag_name(f)
        t = base
        n = 1
        while t in used:
            t = f"{base}-{n}"
            n += 1
        used.add(t)
        tags[f] = t

    tmp = Path(tempfile.gettempdir()) / f"agent-tags-fixture-gen-{args.instance_id}"
    clone_repo(repo, tmp, base_commit)

    (tmp / "AGENTS.md").write_text(AGENTS_MD, encoding="utf-8")

    for f in files:
        others = [(p, tags[p]) for p in files if p != f and (tmp / p).is_file()]
        desc = f"Subsystem context for coordination with related modules. Part of the graph around {f.split('/')[-1]}."
        inject_into_file(tmp, f, tags[f], desc, others)

    subprocess.run(["git", "-C", str(tmp), "add", "-A"], check=True)
    # Combined unstaged+staged diff like eval README
    diff_cached = subprocess.run(
        ["git", "-C", str(tmp), "diff", "--cached"],
        capture_output=True,
        text=True,
        check=True,
    )
    diff_work = subprocess.run(
        ["git", "-C", str(tmp), "diff"],
        capture_output=True,
        text=True,
        check=True,
    )
    out = diff_cached.stdout + diff_work.stdout
    out_path = eval_dir / "tasks" / f"{args.instance_id}.tags.diff"
    out_path.write_text(out, encoding="utf-8")
    print(f"Wrote {out_path} ({len(out)} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
