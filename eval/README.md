# agent-tags eval

Do `@agents` tags help AI coding agents resolve real GitHub issues?

Paired A/B comparison on [SWE-bench Verified](https://huggingface.co/datasets/princeton-nlp/SWE-bench_Verified) tasks. Baseline agent works on vanilla repos. With-tags agent gets pre-written `@agents` annotations and is told to run `git agent-tags context` before editing.

## Task selection rubric

When adding tasks from [`tasks/CANDIDATES.md`](tasks/CANDIDATES.md) (multi-file ground-truth patches):

1. **Diversity** — mix repos (Django, scientific Python, tooling, etc.) so results are not single-project noise.
2. **Tractability** — prefer **2–4** files in the gold patch first; larger patches are OK when the subsystem is still taggable in reasonable time.
3. **Fixture quality** — tags should map a **subsystem** (not only the gold files): named headers `@agents(name)`, `Related:` with fragments `path/file.py#name`, root `AGENTS.md`, and **no** mention of the bug or fix in tag text. For the full quality bar, checklist, and validation steps, see [`TAG_FIXTURE_GUIDE.md`](TAG_FIXTURE_GUIDE.md).
4. **Registry** — record the task in [`tasks/manifest.json`](tasks/manifest.json) (status + cohort) so batch runs stay reproducible. Use optional `tag_quality` (`bootstrap` vs `reviewed`) per task when tracking fixture maturity.

## Evaluation loop

1. **Onboard** — `./scripts/add_task.sh <instance_id>` (downloads `tasks/<id>.json`; requires `pip install datasets`).
2. **Fixture** — hand-write tags in the clone under `/tmp/agent-tags-fixture-<id>/`, or bootstrap with `python3 scripts/generate_tag_fixture.py <id>` and **edit** the diff.
3. **Register** — `python3 scripts/manifest_update.py <id> fixture-authored --cohort initial` (adjust cohort as needed).
4. **Preflight** — `./scripts/validate_fixture.sh <id>` (use `--deep` for `git apply --check` + network).
5. **Run agent** — `./scripts/run_eval.sh <id> baseline 1` and `./scripts/run_eval.sh <id> with-tags 1` (repeat trials as needed).
6. **Score** — `./scripts/evaluate.sh <id> baseline 1` and `./scripts/evaluate.sh <id> with-tags 1`.
7. **Compare** — `./scripts/compare.sh <id>` or `./scripts/summary.sh [cohort]`.

`result.txt` values: `RESOLVED`, `FAILED`, `PATCH_APPLY_FAILED`, or `NOT_RUN` if `evaluate.sh` was not run yet.

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

# 3. Generate the patch file (run from inside the fixture clone)
cd /tmp/agent-tags-fixture-<instance_id>
git add -A
(git diff; git diff --cached) > /path/to/agent-tags/eval/tasks/<instance_id>.tags.diff

# 4. Clean up
rm -rf /tmp/agent-tags-fixture-<instance_id>
```

## Running evals

```bash
cd eval

# Single task
./scripts/run_eval.sh <task_id> baseline 1
./scripts/run_eval.sh <task_id> with-tags 1
./scripts/evaluate.sh <task_id> baseline 1
./scripts/evaluate.sh <task_id> with-tags 1

# All tasks that have both .json and .tags.diff (skips already-run trials)
./scripts/run_all.sh        # 1 trial each
./scripts/run_all.sh 3      # 3 trials each

# Only the `initial` cohort from tasks/manifest.json
./scripts/run_all.sh 1 initial
./scripts/run_all.sh 3 initial

# Results
./scripts/compare.sh <task_id>   # per-task detail
./scripts/summary.sh             # all tasks (excludes tasks/manifest.json)
./scripts/summary.sh initial     # cohort aggregate + per-task rows
```

## Metrics

- **File localization** — did the agent touch the right files? (X/Y correct vs gold patch file list)
- **Resolve rate** — do `FAIL_TO_PASS` tests pass after the patch?
- **Files touched** — fewer = more precise

## Structure

```
eval/
├── TAG_FIXTURE_GUIDE.md      # quality checklist, workflow, edge cases
├── tasks/
│   ├── manifest.json           # cohorts + per-task status notes
│   ├── CANDIDATES.md           # curated multi-file SWE-bench Verified IDs
│   ├── <task_id>.json          # SWE-bench metadata (from add_task.sh)
│   └── <task_id>.tags.diff     # Pre-written tag fixture
├── results/                    # gitignored
│   └── <task_id>/
│       ├── baseline/trial-N/
│       └── with-tags/trial-N/
└── scripts/
    ├── add_task.sh
    ├── generate_tag_fixture.py # optional bootstrap for .tags.diff
    ├── quality_fixture_lib.py  # helpers for curated fixtures
    ├── quality_fixtures_batch.py # regenerate batch of reviewed .tags.diff
    ├── quality_specs_extended.py # specs for batch tasks
    ├── quality_fixture_xarray_3993.py # xarray pilot spec
    ├── manifest_update.py      # update manifest.json status / cohorts
    ├── validate_fixture.sh     # preflight JSON + diff shape (+ optional --deep)
    ├── run_eval.sh
    ├── run_all.sh
    ├── evaluate.sh
    ├── compare.sh
    └── summary.sh
```

## Cohort: `initial`

The `initial` cohort in `tasks/manifest.json` has **10** tasks (two original plus eight added for broader coverage). Re-run `./scripts/summary.sh initial` after evaluations to update aggregates.

## Results snapshot (historical)

Early single-task numbers (see repo history / local `results/`):

| Task | Baseline | With-tags |
|------|----------|-----------|
| django__django-13195 | 1/3 files | 3/3 files |
