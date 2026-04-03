# agent-tags eval

Do `@agents` tags help AI coding agents resolve real GitHub issues?

Paired A/B comparison on SWE-bench Verified tasks. Baseline agent works on vanilla repos. With-tags agent gets pre-written `@agents` annotations and is told to run `git agent-tags context` before editing.

## Adding a new task

```bash
# 1. Download metadata and set up a tagging workspace
./scripts/add_task.sh <instance_id>

# 2. Follow the printed instructions to write tags in the workspace
#    - Tag the subsystem broadly (15-30 files), not just the patch files
#    - Use named headers: @agents(name)
#    - Use fragment references: Related: path/file.py#name
#    - Add an AGENTS.md at the repo root
#    - Don't mention the bug or fix

# 3. Generate the patch file
cd /tmp/agent-tags-fixture-<instance_id>
git add AGENTS.md
(git diff; git diff --cached) > eval/tasks/<instance_id>.tags.diff

# 4. Clean up
rm -rf /tmp/agent-tags-fixture-<instance_id>
```

## Running evals

```bash
# Single task
./scripts/run_eval.sh <task_id> baseline 1
./scripts/run_eval.sh <task_id> with-tags 1

# All tasks with fixtures (skips already-run trials)
./scripts/run_all.sh        # 1 trial each
./scripts/run_all.sh 3      # 3 trials each

# Results
./scripts/compare.sh <task_id>   # per-task detail
./scripts/summary.sh             # table across all tasks
```

## Metrics

- **File localization** — did the agent find the right files? (X/Y correct)
- **Resolve rate** — do FAIL_TO_PASS tests pass after the patch?
- **Files touched** — fewer = more precise

## Structure

```
eval/
├── tasks/
│   ├── <task_id>.json          # SWE-bench metadata (auto-downloaded)
│   └── <task_id>.tags.diff     # Pre-written tag fixture (manual)
├── results/                    # gitignored
│   └── <task_id>/
│       ├── baseline/trial-N/
│       └── with-tags/trial-N/
└── scripts/
    ├── add_task.sh             # Download task + set up tagging workspace
    ├── run_eval.sh             # Run agent on a single task
    ├── run_all.sh              # Run all tasks with fixtures
    ├── evaluate.sh             # Check patch against tests
    ├── compare.sh              # Per-task detail comparison
    └── summary.sh              # Summary table across all tasks
```

## Results so far

| Task | Baseline | With-tags |
|------|----------|-----------|
| django__django-13195 | 1/3 files | 3/3 files |
