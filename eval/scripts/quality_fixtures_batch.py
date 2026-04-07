#!/usr/bin/env python3
"""
Export high-quality eval/tasks/<task_id>.tags.diff for multiple SWE-bench tasks.
Requires network for first-time clones under /tmp/quality-<task_id>.

Usage (from repo root):
  python3 eval/scripts/quality_fixtures_batch.py pytest-dev__pytest-5840
  python3 eval/scripts/quality_fixtures_batch.py --all
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
EVAL_DIR = SCRIPT_DIR.parent
sys.path.insert(0, str(SCRIPT_DIR))
from quality_fixture_lib import Entry, apply_entries, export_diff
from quality_specs_extended import EXTENDED_SPECS


def T(
    rel: str,
    tag: str,
    body: str,
    *related_flat: str,
) -> Entry:
    pairs = list(zip(related_flat[0::2], related_flat[1::2]))
    return (rel, tag, body.split("|"), pairs, None)


def ensure_repo(dest: Path, repo: str, commit: str) -> None:
    url = f"https://github.com/{repo}.git"
    if not dest.is_dir():
        subprocess.run(["git", "clone", "--quiet", url, str(dest)], check=True)
    subprocess.run(
        ["git", "-C", str(dest), "fetch", "--quiet", "origin", commit, "--depth", "1"],
        check=True,
    )
    subprocess.run(
        ["git", "-C", str(dest), "checkout", "--quiet", "FETCH_HEAD"], check=True
    )
    subprocess.run(
        ["git", "-C", str(dest), "reset", "--hard", "--quiet", "FETCH_HEAD"],
        check=True,
    )
    subprocess.run(["git", "-C", str(dest), "clean", "-fdq"], check=True)


def agents_md(repo_hint: str, extra: str) -> str:
    return f"""# Agent Context

This codebase uses `@agents` tags — structured comments in source files that describe what each file does and how it relates to others.

{extra}

## Reading tags

Look for `@agents(name)` in the first few lines of source files.

- **Body**: what the file does
- **Related**: repo-root-relative paths. `#name` fragments point to named headers.
- **Warnings**: `Don't`, `Warning:`, `Note:`, or `Avoid:`

## Before editing

Read headers and follow `Related:` to see coordination points across {repo_hint}.
"""


# --- pytest-dev__pytest-5840 ---
PYTEST_AGENTS = agents_md(
    "pytest",
    "Pytest lives under `src/_pytest/`; configuration is split across `config/` "
    "and runtime collection uses `pathlib`, `main`, and `python` modules.",
)
PYTEST_SPEC: list[Entry] = [
    T(
        "src/_pytest/config/__init__.py",
        "pytest-config",
        "Config container, plugin registration, and session-scoped option values.|Entry point used by _pytest.main after CLI parsing.",
        "src/_pytest/config/argparsing.py",
        "config-argparse",
        "src/_pytest/config/findpaths.py",
        "config-findpaths",
        "src/_pytest/main.py",
        "pytest-main",
        "src/_pytest/pathlib.py",
        "pytest-pathlib",
    ),
    T(
        "src/_pytest/config/argparsing.py",
        "config-argparse",
        "Parses CLI args into argparse.Namespace and builds initial Config.|Feeds findpaths and plugin loading.",
        "src/_pytest/config/__init__.py",
        "pytest-config",
        "src/_pytest/config/findpaths.py",
        "config-findpaths",
        "src/_pytest/helpconfig.py",
        "pytest-helpconfig",
    ),
    T(
        "src/_pytest/config/findpaths.py",
        "config-findpaths",
        "Discovers `conftest.py` and test paths from args and filesystem layout.|Uses pathlib helpers for importable path resolution.",
        "src/_pytest/pathlib.py",
        "pytest-pathlib",
        "src/_pytest/config/__init__.py",
        "pytest-config",
        "src/_pytest/main.py",
        "pytest-main",
    ),
    T(
        "src/_pytest/pathlib.py",
        "pytest-pathlib",
        "Filesystem and importlib helpers for turning paths into modules.|Shared by collection, config discovery, and tmpdir.",
        "src/_pytest/config/findpaths.py",
        "config-findpaths",
        "src/_pytest/python.py",
        "pytest-python",
        "src/_pytest/main.py",
        "pytest-main",
    ),
    T(
        "src/_pytest/main.py",
        "pytest-main",
        "Session lifecycle: create Config, perform collection, run test items.|Hosts Session collector and interacts with runner and terminal.",
        "src/_pytest/config/__init__.py",
        "pytest-config",
        "src/_pytest/python.py",
        "pytest-python",
        "src/_pytest/runner.py",
        "pytest-runner",
        "src/_pytest/nodes.py",
        "pytest-nodes",
    ),
    T(
        "src/_pytest/python.py",
        "pytest-python",
        "Python test collection: Module, Function items, fixtures integration.|Imports use pathlib-backed module loading.",
        "src/_pytest/pathlib.py",
        "pytest-pathlib",
        "src/_pytest/main.py",
        "pytest-main",
        "src/_pytest/nodes.py",
        "pytest-nodes",
        "src/_pytest/fixtures.py",
        "pytest-fixtures",
    ),
    T(
        "src/_pytest/nodes.py",
        "pytest-nodes",
        "Collector and Item base classes forming the session tree.|FSCollector ties filesystem nodes to paths.",
        "src/_pytest/main.py",
        "pytest-main",
        "src/_pytest/python.py",
        "pytest-python",
        "src/_pytest/runner.py",
        "pytest-runner",
    ),
    T(
        "src/_pytest/runner.py",
        "pytest-runner",
        "Runs setup/call/teardown for each test item and records outcomes.|Coordinates with capture and fixtures.",
        "src/_pytest/main.py",
        "pytest-main",
        "src/_pytest/python.py",
        "pytest-python",
        "src/_pytest/capture.py",
        "pytest-capture",
    ),
    T(
        "src/_pytest/hookspec.py",
        "pytest-hookspec",
        "Declarative hook specifications plugins implement.|Config wires hook callers across the session.",
        "src/_pytest/config/__init__.py",
        "pytest-config",
        "src/_pytest/main.py",
        "pytest-main",
        "src/_pytest/terminal.py",
        "pytest-terminal",
    ),
    T(
        "src/_pytest/terminal.py",
        "pytest-terminal",
        "Console reporting: progress, failures, and summary lines.|Consumes hook outcomes from runner and reports.",
        "src/_pytest/hookspec.py",
        "pytest-hookspec",
        "src/_pytest/main.py",
        "pytest-main",
        "src/_pytest/runner.py",
        "pytest-runner",
    ),
    T(
        "src/_pytest/capture.py",
        "pytest-capture",
        "Stdin/stdout/stderr capture during tests.|Used by runner for failure output.",
        "src/_pytest/runner.py",
        "pytest-runner",
        "src/_pytest/terminal.py",
        "pytest-terminal",
    ),
    T(
        "src/_pytest/fixtures.py",
        "pytest-fixtures",
        "Fixture definition, dependency resolution, and scoping.|Integrates with python item setup.",
        "src/_pytest/python.py",
        "pytest-python",
        "src/_pytest/nodes.py",
        "pytest-nodes",
        "src/_pytest/runner.py",
        "pytest-runner",
    ),
    T(
        "src/_pytest/compat.py",
        "pytest-compat",
        "Version shims for Python and legacy pytest APIs.|Imported widely across collection code.",
        "src/_pytest/pathlib.py",
        "pytest-pathlib",
        "src/_pytest/python.py",
        "pytest-python",
    ),
    T(
        "src/_pytest/helpconfig.py",
        "pytest-helpconfig",
        "Help and version printing paths for CLI.|Uses same argparse objects as config-argparse.",
        "src/_pytest/config/argparsing.py",
        "config-argparse",
        "src/_pytest/config/__init__.py",
        "pytest-config",
    ),
    T(
        "src/_pytest/_code/code.py",
        "pytest-code",
        "Code and frame introspection for tracebacks and assertion rewriting.|Supports terminal and debugging plugins.",
        "src/_pytest/runner.py",
        "pytest-runner",
        "src/_pytest/terminal.py",
        "pytest-terminal",
    ),
    T(
        "src/_pytest/__init__.py",
        "pytest-internal-init",
        "Package marker for pytest implementation modules.|Public pytest API is exposed via setuptools entrypoints elsewhere.",
        "src/_pytest/main.py",
        "pytest-main",
        "src/_pytest/config/__init__.py",
        "pytest-config",
    ),
]

# --- sympy__sympy-14248 ---
SYMPY_AGENTS = agents_md(
    "SymPy",
    "Printing stack: `sympy/printing/printer.py` dispatches; `latex`, `str`, and "
    "`pretty` backends share `conventions` and `precedence`.",
)
SYMPY_SPEC: list[Entry] = [
    T(
        "sympy/printing/__init__.py",
        "printing-init",
        "Re-exports printers and registers default printing methods.|Consumers pick strrepr vs pretty vs latex.",
        "sympy/printing/printer.py",
        "printer-base",
        "sympy/printing/latex.py",
        "printing-latex",
        "sympy/printing/str.py",
        "printing-str",
    ),
    T(
        "sympy/printing/printer.py",
        "printer-base",
        "Generic dispatch mechanism selecting printer methods per expression type.|All concrete printers subclass Printer.",
        "sympy/printing/defaults.py",
        "printing-defaults",
        "sympy/printing/conventions.py",
        "printing-conventions",
        "sympy/printing/precedence.py",
        "printing-precedence",
    ),
    T(
        "sympy/printing/latex.py",
        "printing-latex",
        "LaTeX output for Expr trees; uses precedence and function bracing rules.|Delegates unknown types to Printer.",
        "sympy/printing/printer.py",
        "printer-base",
        "sympy/printing/conventions.py",
        "printing-conventions",
        "sympy/printing/precedence.py",
        "printing-precedence",
    ),
    T(
        "sympy/printing/str.py",
        "printing-str",
        "Plain string representation for interactive use.|Shares precedence tables with latex and pretty.",
        "sympy/printing/printer.py",
        "printer-base",
        "sympy/printing/defaults.py",
        "printing-defaults",
        "sympy/printing/precedence.py",
        "printing-precedence",
    ),
    T(
        "sympy/printing/pretty/pretty.py",
        "pretty-printer",
        "ASCII/Unicode pretty printer implementation.|Builds layout via stringpict helpers.",
        "sympy/printing/printer.py",
        "printer-base",
        "sympy/printing/pretty/stringpict.py",
        "stringpict",
        "sympy/printing/pretty/pretty_symbology.py",
        "pretty-symbology",
    ),
    T(
        "sympy/printing/pretty/stringpict.py",
        "stringpict",
        "Low-level box drawing and horizontal/vertical composition for pretty output.|Used exclusively by pretty printer.",
        "sympy/printing/pretty/pretty.py",
        "pretty-printer",
        "sympy/printing/pretty/pretty_symbology.py",
        "pretty-symbology",
    ),
    T(
        "sympy/printing/pretty/pretty_symbology.py",
        "pretty-symbology",
        "Glyph tables and symbol rendering choices for pretty printer.|Keeps math notation consistent across Expr types.",
        "sympy/printing/pretty/pretty.py",
        "pretty-printer",
        "sympy/printing/pretty/stringpict.py",
        "stringpict",
    ),
    T(
        "sympy/printing/conventions.py",
        "printing-conventions",
        "Shared naming and split_super_sub helpers across printers.|Avoid duplicating split logic in latex/str/pretty.",
        "sympy/printing/latex.py",
        "printing-latex",
        "sympy/printing/str.py",
        "printing-str",
    ),
    T(
        "sympy/printing/precedence.py",
        "printing-precedence",
        "Operator precedence tables and PRECEDENCE values.|Parenthesization decisions flow from here.",
        "sympy/printing/latex.py",
        "printing-latex",
        "sympy/printing/str.py",
        "printing-str",
        "sympy/printing/pretty/pretty.py",
        "pretty-printer",
    ),
    T(
        "sympy/printing/defaults.py",
        "printing-defaults",
        "Default printer method registrations and fallback printing.|Printer consults these before raising NotImplemented.",
        "sympy/printing/printer.py",
        "printer-base",
        "sympy/printing/str.py",
        "printing-str",
    ),
    T(
        "sympy/printing/codeprinter.py",
        "codeprinter",
        "Base for C/Fortran/Python code generation printers.|Separate from interactive str/latex backends.",
        "sympy/printing/printer.py",
        "printer-base",
        "sympy/printing/conventions.py",
        "printing-conventions",
    ),
    T(
        "sympy/printing/repr.py",
        "printing-repr",
        "ReprPrinter for debug-oriented srepr output.|Uses Printer dispatch like strprinter.",
        "sympy/printing/printer.py",
        "printer-base",
        "sympy/printing/defaults.py",
        "printing-defaults",
    ),
    T(
        "sympy/core/basic.py",
        "core-basic",
        "Base class for SymPy objects: args, substitution, and traversal.|Printers recurse through Basic.args.",
        "sympy/core/expr.py",
        "core-expr",
        "sympy/printing/printer.py",
        "printer-base",
    ),
    T(
        "sympy/core/expr.py",
        "core-expr",
        "Expr algebraic operations layer above Basic.|Printing methods often special-case Expr subclasses.",
        "sympy/core/basic.py",
        "core-basic",
        "sympy/printing/str.py",
        "printing-str",
    ),
    T(
        "sympy/printing/tree.py",
        "printing-tree",
        "Debug printer showing expression tree structure.|Helpful when extending Printer subclasses.",
        "sympy/printing/printer.py",
        "printer-base",
        "sympy/core/basic.py",
        "core-basic",
    ),
    T(
        "sympy/printing/pycode.py",
        "printing-pycode",
        "Python code emission for lambdify-style backends.|Shares CodePrinter patterns.",
        "sympy/printing/codeprinter.py",
        "codeprinter",
        "sympy/printing/printer.py",
        "printer-base",
    ),
]

TASK_SPECS: dict[str, tuple[str, list[Entry]]] = {
    "pytest-dev__pytest-5840": (PYTEST_AGENTS, PYTEST_SPEC),
    "sympy__sympy-14248": (SYMPY_AGENTS, SYMPY_SPEC),
    **EXTENDED_SPECS,
}


def export_one(task_id: str) -> None:
    meta = json.loads((EVAL_DIR / "tasks" / f"{task_id}.json").read_text())
    repo = meta["repo"]
    commit = meta["base_commit"]
    if task_id not in TASK_SPECS:
        print(f"No batch spec for {task_id}", file=sys.stderr)
        sys.exit(1)
    agents, spec = TASK_SPECS[task_id]
    dest = Path("/tmp") / f"quality-{task_id.replace('__', '-')}"
    print(f"==> {task_id}: clone {repo} @ {commit[:8]}…")
    ensure_repo(dest, repo, commit)
    apply_entries(dest, agents, spec)
    out = EVAL_DIR / "tasks" / f"{task_id}.tags.diff"
    export_diff(dest, out)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("task_id", nargs="?", help="e.g. pytest-dev__pytest-5840")
    ap.add_argument("--all", action="store_true", help="export every task in TASK_SPECS")
    args = ap.parse_args()
    if args.all:
        for tid in sorted(TASK_SPECS):
            export_one(tid)
        return 0
    if not args.task_id:
        ap.print_help()
        return 1
    export_one(args.task_id)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
