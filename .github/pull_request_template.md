## Summary

<!-- What changed and why -->

## Eval fixture PRs (if touching `eval/tasks/*.tags.diff`)

- [ ] `tag_quality` updated in `eval/tasks/manifest.json` if needed (`bootstrap` vs `reviewed`)
- [ ] `./eval/scripts/validate_fixture.sh <task_id> --deep` passes
- [ ] In a clone at `base_commit`: `git agent-tags broken` is clean (no broken `#fragment` refs)
- [ ] See `eval/TAG_FIXTURE_GUIDE.md` for the quality checklist
