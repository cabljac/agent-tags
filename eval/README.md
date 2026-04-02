# agent-tags eval

Pilot evaluation: do `@agents` tags help AI coding agents resolve real GitHub issues?

## Design

**Paired A/B comparison** on SWE-bench Verified tasks:
- **Baseline:** Agent works on the repo as-is
- **With tags:** Agent generates `@agents` tags first, then a fresh agent instance works on the tagged repo

**Metrics:**
- Resolve rate (FAIL_TO_PASS tests pass after patch)
- File localization accuracy (did the agent find the right files?)
- Files touched (fewer = more precise)

## Tasks

| Task ID | Repo | Files in patch | Issue |
|---------|------|---------------|-------|
| `django__django-13195` | django/django | 3 | `delete_cookie()` should preserve `samesite` |
| `pylint-dev__pylint-6528` | pylint-dev/pylint | 2 | Ignore patterns not respected in recursive mode |

## Usage

```bash
# Run a single task
./scripts/run_eval.sh django__django-13195 baseline 1
./scripts/run_eval.sh django__django-13195 with-tags 1

# Evaluate results (runs tests against the patch)
./scripts/evaluate.sh django__django-13195 baseline 1
./scripts/evaluate.sh django__django-13195 with-tags 1

# Compare conditions
./scripts/compare.sh django__django-13195
```

## Structure

```
eval/
├── tasks/           # Task metadata (JSON per task)
├── results/         # Output from runs (gitignored)
│   └── <task_id>/
│       ├── baseline/trial-1/
│       └── with-tags/trial-1/
└── scripts/
    ├── run_eval.sh  # Run agent on a task
    ├── evaluate.sh  # Check patch against tests
    └── compare.sh   # Compare baseline vs with-tags
```
