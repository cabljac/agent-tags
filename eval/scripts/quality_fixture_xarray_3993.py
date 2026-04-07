#!/usr/bin/env python3
"""
One-off helper: apply high-quality @agents headers for pydata__xarray-3993.
Run inside a clean clone at the task base_commit (see tasks/pydata__xarray-3993.json).

  git clone https://github.com/pydata/xarray.git /tmp/xarray-q && cd /tmp/xarray-q
  git checkout <base_commit>
  python3 /path/to/agent-tags/eval/scripts/quality_fixture_xarray_3993.py /tmp/xarray-q
  (cd /tmp/xarray-q && git add -A && git diff --cached > ...tags.diff)
"""
from __future__ import annotations

import sys
from pathlib import Path


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


# (path, tag, body lines, related, optional warning)
SPEC: list[tuple[str, str, list[str], list[tuple[str, str]], str | None]] = [
    (
        "xarray/__init__.py",
        "xarray-public",
        [
            "Public package surface: DataArray, Dataset, I/O open_* helpers, align, merge, apply_ufunc.",
            "Core labeled-array logic lives under xarray/core/; backends under xarray/backends/.",
        ],
        [
            ("xarray/core/dataarray.py", "dataarray"),
            ("xarray/core/dataset.py", "dataset"),
            ("xarray/core/variable.py", "variable"),
        ],
        None,
    ),
    (
        "xarray/core/dataarray.py",
        "dataarray",
        [
            "Labeled N-D array: wraps Variable plus coord dicts and dimension names.",
            "Reductions, broadcasting, and ufuncs route through computation and duck_array_ops.",
        ],
        [
            ("xarray/core/dataset.py", "dataset"),
            ("xarray/core/variable.py", "variable"),
            ("xarray/core/common.py", "common-mixins"),
            ("xarray/core/coordinates.py", "coordinates"),
            ("xarray/core/computation.py", "computation"),
            ("xarray/core/duck_array_ops.py", "duck-ops"),
        ],
        None,
    ),
    (
        "xarray/core/dataset.py",
        "dataset",
        [
            "Dict-like container of aligned Variables sharing dimensions.",
            "Merge, align, and groupby coordinate with dataarray and merge modules.",
        ],
        [
            ("xarray/core/dataarray.py", "dataarray"),
            ("xarray/core/variable.py", "variable"),
            ("xarray/core/merge.py", "merge"),
            ("xarray/core/alignment.py", "alignment"),
            ("xarray/core/coordinates.py", "coordinates"),
        ],
        None,
    ),
    (
        "xarray/core/common.py",
        "common-mixins",
        [
            "Shared mixins for DataArray and Dataset: dims, coords, dtype, repr hooks.",
            "Depends on dtypes, duck_array_ops, and ops for type-specific behavior.",
        ],
        [
            ("xarray/core/dataarray.py", "dataarray"),
            ("xarray/core/dataset.py", "dataset"),
            ("xarray/core/variable.py", "variable"),
            ("xarray/core/dtypes.py", "dtypes"),
            ("xarray/core/duck_array_ops.py", "duck-ops"),
        ],
        None,
    ),
    (
        "xarray/core/variable.py",
        "variable",
        [
            "Variable holds data array, dimensions, and attributes; underlies DataArray internals.",
            "Indexing, broadcasting, and array ops use indexing, duck_array_ops, and nanops.",
        ],
        [
            ("xarray/core/duck_array_ops.py", "duck-ops"),
            ("xarray/core/indexing.py", "indexing"),
            ("xarray/core/dtypes.py", "dtypes"),
            ("xarray/core/nanops.py", "nanops"),
            ("xarray/core/dataarray.py", "dataarray"),
        ],
        None,
    ),
    (
        "xarray/core/duck_array_ops.py",
        "duck-ops",
        [
            "NumPy-like ops on duck arrays (NumPy, Dask, sparse); no xarray objects in API surface.",
            "Delegates dtype and compat checks to dtypes, nputils, and dask shims.",
        ],
        [
            ("xarray/core/variable.py", "variable"),
            ("xarray/core/nanops.py", "nanops"),
            ("xarray/core/dtypes.py", "dtypes"),
            ("xarray/core/computation.py", "computation"),
        ],
        None,
    ),
    (
        "xarray/core/nanops.py",
        "nanops",
        [
            "NaN-skipping reductions and helpers built on duck_array_ops.",
            "Used by Variable and computation paths for aggregations.",
        ],
        [
            ("xarray/core/duck_array_ops.py", "duck-ops"),
            ("xarray/core/variable.py", "variable"),
        ],
        None,
    ),
    (
        "xarray/core/indexes.py",
        "indexes",
        [
            "Index objects backing coordinates (pandas, CFTime); used when slicing and aligning.",
            "Coordinates and merge construct and update indexes alongside Variables.",
        ],
        [
            ("xarray/core/coordinates.py", "coordinates"),
            ("xarray/core/variable.py", "variable"),
            ("xarray/core/indexing.py", "indexing"),
        ],
        None,
    ),
    (
        "xarray/core/coordinates.py",
        "coordinates",
        [
            "Coordinates container mapping names to IndexVariable and dimension alignment.",
            "Interacts with indexes, merge, and Dataset/DataArray coord accessors.",
        ],
        [
            ("xarray/core/indexes.py", "indexes"),
            ("xarray/core/variable.py", "variable"),
            ("xarray/core/dataset.py", "dataset"),
            ("xarray/core/dataarray.py", "dataarray"),
        ],
        None,
    ),
    (
        "xarray/core/indexing.py",
        "indexing",
        [
            "Indexer types and lazy indexing for Variable and derived objects.",
            "Keeps indexing semantics separate from duck array materialization.",
        ],
        [
            ("xarray/core/variable.py", "variable"),
            ("xarray/core/dtypes.py", "dtypes"),
        ],
        None,
    ),
    (
        "xarray/core/merge.py",
        "merge",
        [
            "Dataset merge and combine logic; resolves variable name and coord conflicts.",
            "Uses alignment and Variable coercion when unifying objects.",
        ],
        [
            ("xarray/core/dataset.py", "dataset"),
            ("xarray/core/alignment.py", "alignment"),
            ("xarray/core/variable.py", "variable"),
        ],
        None,
    ),
    (
        "xarray/core/alignment.py",
        "alignment",
        [
            "Align DataArray/Dataset objects on shared (and new) dimension labels.",
            "Feeds merge, concat, and broadcast helpers used by Dataset operations.",
        ],
        [
            ("xarray/core/dataset.py", "dataset"),
            ("xarray/core/dataarray.py", "dataarray"),
            ("xarray/core/variable.py", "variable"),
        ],
        None,
    ),
    (
        "xarray/core/computation.py",
        "computation",
        [
            "apply_ufunc, where, dot, and broadcasting rules across xarray objects.",
            "Bridges Variables and duck_array_ops for vectorized numpy ufuncs.",
        ],
        [
            ("xarray/core/dataarray.py", "dataarray"),
            ("xarray/core/dataset.py", "dataset"),
            ("xarray/core/variable.py", "variable"),
            ("xarray/core/duck_array_ops.py", "duck-ops"),
        ],
        None,
    ),
    (
        "xarray/core/ops.py",
        "ops",
        [
            "Operator overload hooks and injected methods for DataArray/Dataset.",
            "Coordinates with arithmetic mixin and duck_array_ops for elementwise math.",
        ],
        [
            ("xarray/core/dataarray.py", "dataarray"),
            ("xarray/core/dataset.py", "dataset"),
            ("xarray/core/arithmetic.py", "arithmetic"),
        ],
        None,
    ),
    (
        "xarray/core/arithmetic.py",
        "arithmetic",
        [
            "SupportsArithmetic protocol and mixins shared by Variable, DataArray, Dataset.",
            "Ops dispatch through ops and duck_array_ops after alignment.",
        ],
        [
            ("xarray/core/ops.py", "ops"),
            ("xarray/core/dataarray.py", "dataarray"),
            ("xarray/core/dataset.py", "dataset"),
            ("xarray/core/variable.py", "variable"),
        ],
        None,
    ),
    (
        "xarray/core/dtypes.py",
        "dtypes",
        [
            "Promotion rules, dtype inference, and NA sentinel handling for Variables.",
            "Consumed by Variable, duck_array_ops, and indexing edge cases.",
        ],
        [
            ("xarray/core/variable.py", "variable"),
            ("xarray/core/duck_array_ops.py", "duck-ops"),
        ],
        None,
    ),
    (
        "xarray/core/options.py",
        "options",
        [
            "Global OPTIONS (keep_attrs, display style) read by common and repr paths.",
            "set_options context manager is re-exported from xarray/__init__.py.",
        ],
        [
            ("xarray/__init__.py", "xarray-public"),
            ("xarray/core/common.py", "common-mixins"),
        ],
        None,
    ),
    (
        "xarray/core/utils.py",
        "utils",
        [
            "Small frozen dict helpers, hashing, and duck-array detection utilities.",
            "Used across core modules for lightweight structure sharing.",
        ],
        [
            ("xarray/core/variable.py", "variable"),
            ("xarray/core/dataset.py", "dataset"),
        ],
        None,
    ),
]

AGENTS_MD = """# Agent Context

This codebase uses `@agents` tags — structured comments in source files that describe what each file does and how it relates to others.

Core labeled-array types (**DataArray**, **Dataset**, **Variable**) and most numerical behavior live under `xarray/core/` (see `dataarray.py`, `dataset.py`, `variable.py`, `computation.py`, `duck_array_ops.py`).

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


def main() -> int:
    root = Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
    if not (root / "xarray").is_dir():
        print("Expected xarray package at ROOT/xarray", file=sys.stderr)
        return 1
    agents = root / "AGENTS.md"
    agents.write_text(AGENTS_MD, encoding="utf-8")

    for rel, name, body, related, warn in SPEC:
        path = root / rel
        if not path.is_file():
            print(f"Missing {path}", file=sys.stderr)
            return 1
        text = path.read_text(encoding="utf-8")
        lines = text.splitlines(keepends=True)
        if any("@agents(" in line for line in lines[:40]):
            print(f"Skip (already tagged): {rel}")
            continue
        block = format_block(name, body, related, warn)
        idx = find_insert_line(lines)
        new_text = "".join(lines[:idx] + [block] + lines[idx:])
        path.write_text(new_text, encoding="utf-8")
        print(f"Tagged {rel}")
    print("Done. Run: git add -A && git agent-tags check")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
