#!/usr/bin/env python3
"""Update eval/tasks/manifest.json task status (and optional cohort membership)."""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def load_manifest(path: Path) -> dict:
    if not path.is_file():
        return {"version": 1, "cohorts": {}, "tasks": {}}
    return json.loads(path.read_text(encoding="utf-8"))


def save_manifest(path: Path, data: dict) -> None:
    path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("task_id", help="SWE-bench instance_id")
    ap.add_argument(
        "status",
        choices=[
            "candidate",
            "metadata-downloaded",
            "fixture-authored",
            "evaluated",
        ],
        help="Lifecycle state for this task",
    )
    ap.add_argument(
        "--cohort",
        action="append",
        default=[],
        metavar="NAME",
        help="Add task to named cohort (repeatable). Creates empty list if missing.",
    )
    ap.add_argument(
        "--eval-root",
        type=Path,
        default=Path(__file__).resolve().parent.parent,
    )
    ap.add_argument(
        "--tag-quality",
        choices=["bootstrap", "reviewed"],
        help="Optional: set tasks.<id>.tag_quality for fixture maturity tracking.",
    )
    args = ap.parse_args()
    manifest_path: Path = args.eval_root / "tasks" / "manifest.json"
    data = load_manifest(manifest_path)
    data.setdefault("version", 1)
    data.setdefault("cohorts", {})
    data.setdefault("tasks", {})

    entry = data["tasks"].get(args.task_id, {})
    entry["status"] = args.status
    if args.tag_quality:
        entry["tag_quality"] = args.tag_quality
    if args.cohort:
        existing = set(entry.get("cohorts", []))
        existing.update(args.cohort)
        entry["cohorts"] = sorted(existing)
        for name in args.cohort:
            cohort = data["cohorts"].setdefault(name, [])
            if args.task_id not in cohort:
                cohort.append(args.task_id)
                data["cohorts"][name] = sorted(cohort)
    data["tasks"][args.task_id] = entry
    save_manifest(manifest_path, data)
    print(f"Updated {manifest_path}: {args.task_id} -> {args.status}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
