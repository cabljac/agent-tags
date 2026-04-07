# High-quality `@agents` fixtures for SWE-bench eval

Eval fixtures are **not** just valid diffs: they should mirror how real teams would use agent-tags—**navigation, real dependencies, and constraints**—so `git agent-tags context` genuinely shortens the path to the right code.

Full tag syntax and rules: [SPEC.md](../SPEC.md).

## Quality checklist (pass / fail)

| Criterion | Pass |
|-----------|------|
| **Coverage** | **15–30** tagged files in the **same subsystem** as the issue (not only gold patch files). Include public entry points (`__init__`, main modules), **import neighbors**, registries/decorators, and 1–2 boundary files a senior dev would open soon. |
| **Bodies** | 2–4 lines: what the file **owns**, how it fits the pipeline, who calls / is called. **No** bug description, **no** fix hints, **no** test names from the issue. |
| **Names** | Short stable `@agents(name)` tokens (`dataset-core`, `dataarray-api`), not long path slugs. |
| **Related graph** | Each header has **several** `Related: path/file.py#other-name` edges; every `#fragment` **resolves** (`git agent-tags check`). |
| **Warnings** | Real invariants where they exist (`Warning:`, `Don't`, `Note:`). |
| **AGENTS.md** | Repo-wide how-to plus **one** repo-specific pointer (e.g. “Core types live under `xarray/core/`”). |
| **Mechanical** | In fixture clone at **base_commit**: `git agent-tags check` is clean. |
| **Apply** | `./scripts/validate_fixture.sh <task_id> --deep` passes. |

## Workflow

1. **Onboard** — `./scripts/add_task.sh <id>` → clone at `base_commit` (or use `/tmp/agent-tags-fixture-<id>`).
2. **Map the subsystem** — Start from gold files; follow **imports**, **base classes**, **registration APIs** (ORM backends, frame transforms, etc.). Stop at 15–30 files.
3. **Write tags** — Named headers, dense `Related:`, optional warnings. Skim the issue only to bound the subsystem, **not** to encode the bug in comments.
4. **Validate** — From repo root of the clone:
   ```bash
   cargo install --path /path/to/agent-tags/git-agent-tags   # once
   git agent-tags check
   git agent-tags context --for <path/to/gold_file.py> --hops 1
   ```
   Fix any broken `#name` references (most common failure).
5. **Export** — Stage all changes, then from the clone:
   ```bash
   git add -A
   (git diff; git diff --cached) > /path/to/agent-tags/eval/tasks/<id>.tags.diff
   ```
6. **Preflight in agent-tags repo** — `cd eval && ./scripts/validate_fixture.sh <id> --deep`

## Bootstrap vs hand-curated

[`scripts/generate_tag_fixture.py`](scripts/generate_tag_fixture.py) creates a **minimal** graph (gold files + generic bodies). Treat that as a **starting point**; replace slugs, expand coverage, and rewrite bodies before calling a fixture “done.”

### Curated export tooling

- [`scripts/quality_fixture_lib.py`](scripts/quality_fixture_lib.py) — shared insert/format/export helpers. Headers are placed **before** a leading module docstring when needed so tags stay within the first lines (required by [SPEC.md](../SPEC.md)).
- [`scripts/quality_fixtures_batch.py`](scripts/quality_fixtures_batch.py) — `python3 quality_fixtures_batch.py --all` regenerates several `tasks/*.tags.diff` from specs (needs network for clones).
- [`scripts/quality_specs_extended.py`](scripts/quality_specs_extended.py) — curated file lists + bodies for astropy, django-11138, matplotlib, pytest, sklearn, sphinx, sympy.
- [`scripts/quality_fixture_xarray_3993.py`](scripts/quality_fixture_xarray_3993.py) — pilot spec for `pydata__xarray-3993`.

Reference-quality examples in this repo: `tasks/django__django-13195.tags.diff`, `tasks/pylint-dev__pylint-6528.tags.diff`, and the refreshed `initial` cohort diffs (15+ files each, `tag_quality: reviewed` in [`tasks/manifest.json`](tasks/manifest.json)).

## Special case: files that do not exist at base commit

Some SWE-bench patches **add** a new file that is absent at `base_commit`. You **cannot** tag that path in the fixture diff.

**Mitigation:** Over-tag **neighbors** (imports, `__init__` registrations, callers) so `context` still routes into the right area. A neutral **Note** in AGENTS.md is OK (“New modules in this package are registered from …”)—still no issue-specific wording.

Example: `astropy__astropy-13398` and `itrs_observed_transforms.py`.

## Worked mini-example (conceptual)

**Before (minimal):** Two gold files, generic one-line body, mutual `Related` only.

**After (quality):** Same two files plus `variable.py`, `duck_array_ops.py`, `common.py`, `indexes.py`, `nanops.py`, etc.—each body states role (“wraps underlying array ops for DataArray”), `Related` lists 3–5 real collaborators, one **Warning** if ordering/coords matter.

Spot-check: `git agent-tags context --for xarray/core/dataarray.py --hops 2` should surface most of the integrate/cumulative API neighborhood without listing the whole package.

## Manifest: `tag_quality`

[`tasks/manifest.json`](tasks/manifest.json) may include per-task `tag_quality`:

- `bootstrap` — generator-only or unchecked.
- `reviewed` — checklist satisfied and `validate_fixture.sh --deep` OK.

Use this so batch runs and PRs can see which fixtures still need a human pass.
