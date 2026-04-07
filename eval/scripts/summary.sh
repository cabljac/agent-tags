#!/usr/bin/env bash
set -euo pipefail

# Usage:
#   ./summary.sh              # all tasks with .json (except manifest.json)
#   ./summary.sh <cohort>     # restrict to tasks listed in tasks/manifest.json

COHORT="${1:-}"
EVAL_DIR="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$EVAL_DIR/tasks/manifest.json"

collect_task_ids() {
  if [ -n "$COHORT" ]; then
    if [ ! -f "$MANIFEST" ]; then
      echo "Error: cohort requested but $MANIFEST missing" >&2
      exit 1
    fi
    jq -r --arg c "$COHORT" '.cohorts[$c][]' "$MANIFEST"
  else
    for json_file in "$EVAL_DIR"/tasks/*.json; do
      [ -f "$json_file" ] || continue
      tid=$(basename "$json_file" .json)
      [ "$tid" = "manifest" ] && continue
      echo "$tid"
    done
  fi
}

# Prints: display|best_result|loc_num|loc_den|trial_count
best_for_condition() {
  local task_id=$1
  local condition=$2
  local best_loc="0/?"
  local best_result="NOT_RUN"
  local trial_count=0

  for trial_dir in "$EVAL_DIR/results/$task_id/$condition"/trial-*; do
    [ -d "$trial_dir" ] || continue
    trial_count=$((trial_count + 1))

    loc=$(cat "$trial_dir/localization.txt" 2>/dev/null || echo "?/?")
    result=$(cat "$trial_dir/result.txt" 2>/dev/null || echo "NOT_RUN")

    correct=$(echo "$loc" | cut -d/ -f1)
    if [ "$correct" != "?" ] && [ "$correct" -gt "$(echo "$best_loc" | cut -d/ -f1)" ] 2>/dev/null; then
      best_loc="$loc"
    fi

    if [ "$result" = "RESOLVED" ]; then
      best_result="RESOLVED"
    elif [ "$result" = "PATCH_APPLY_FAILED" ] && [ "$best_result" != "RESOLVED" ]; then
      best_result="PATCH_APPLY_FAILED"
    elif [ "$result" = "FAILED" ] && [ "$best_result" != "RESOLVED" ] && [ "$best_result" != "PATCH_APPLY_FAILED" ]; then
      best_result="FAILED"
    fi
  done

  if [ "$trial_count" -eq 0 ]; then
    echo "—|NOT_RUN|0|0|0"
    return
  fi

  local loc_display="$best_loc files"
  if [ "$best_result" = "RESOLVED" ]; then
    loc_display="$loc_display ✓"
  elif [ "$best_result" = "FAILED" ]; then
    loc_display="$loc_display ✗"
  elif [ "$best_result" = "PATCH_APPLY_FAILED" ]; then
    loc_display="$loc_display ⚠"
  fi
  loc_display="$loc_display (n=$trial_count)"

  local num den
  num=$(echo "$best_loc" | cut -d/ -f1)
  den=$(echo "$best_loc" | cut -d/ -f2)
  if [ "$num" = "?" ] || [ "$den" = "?" ]; then
    num=0
    den=0
  fi
  echo "${loc_display}|${best_result}|${num}|${den}|${trial_count}"
}

# --- Per-task table ---
printf "%-40s | %-20s | %-20s\n" "Task" "Baseline" "With-tags"
printf "%-40s-+-%-20s-+-%-20s\n" "$(printf '%0.s-' {1..40})" "$(printf '%0.s-' {1..20})" "$(printf '%0.s-' {1..20})"

baseline_resolved=0
baseline_failed=0
baseline_patch_fail=0
baseline_not_run=0
with_resolved=0
with_failed=0
with_patch_fail=0
with_not_run=0
sum_base_num=0
sum_base_den=0
sum_with_num=0
sum_with_den=0
task_count=0

while IFS= read -r task_id; do
  [ -n "$task_id" ] || continue
  task_count=$((task_count + 1))

  IFS='|' read -r baseline_summary br bn bd _bt <<<"$(best_for_condition "$task_id" baseline)"
  IFS='|' read -r with_tags_summary wr wn wd _wt <<<"$(best_for_condition "$task_id" with-tags)"

  printf "%-40s | %-20s | %-20s\n" "$task_id" "$baseline_summary" "$with_tags_summary"

  case "$br" in
    RESOLVED) baseline_resolved=$((baseline_resolved + 1)) ;;
    FAILED) baseline_failed=$((baseline_failed + 1)) ;;
    PATCH_APPLY_FAILED) baseline_patch_fail=$((baseline_patch_fail + 1)) ;;
    NOT_RUN) baseline_not_run=$((baseline_not_run + 1)) ;;
  esac
  case "$wr" in
    RESOLVED) with_resolved=$((with_resolved + 1)) ;;
    FAILED) with_failed=$((with_failed + 1)) ;;
    PATCH_APPLY_FAILED) with_patch_fail=$((with_patch_fail + 1)) ;;
    NOT_RUN) with_not_run=$((with_not_run + 1)) ;;
  esac
  if [ "$bn" != "0" ] || [ "$bd" != "0" ]; then
    sum_base_num=$((sum_base_num + bn))
    sum_base_den=$((sum_base_den + bd))
  fi
  if [ "$wn" != "0" ] || [ "$wd" != "0" ]; then
    sum_with_num=$((sum_with_num + wn))
    sum_with_den=$((sum_with_den + wd))
  fi
done < <(collect_task_ids)

echo ""
echo "Legend: X/Y files = localization (best trial), ✓ = resolved, ✗ = tests failed, ⚠ = patch did not apply, n = trials run"
echo ""

# --- Aggregate (best trial per task per condition) ---
echo "=== Aggregate (best-of-trials per task) ==="
if [ -n "$COHORT" ]; then
  echo "Cohort: $COHORT"
fi
echo "Tasks in view: $task_count"
echo ""
printf "%-14s | %8s %8s %8s %8s\n" "Condition" "Resolved" "Failed" "Patch⚠" "NotRun"
printf "%-14s-+-%-8s-+-%-8s-+-%-8s-+-%-8s\n" "$(printf '%0.s-' {1..14})" "$(printf '%0.s-' {1..8})" "$(printf '%0.s-' {1..8})" "$(printf '%0.s-' {1..8})" "$(printf '%0.s-' {1..8})"
printf "%-14s | %8s %8s %8s %8s\n" "baseline" "$baseline_resolved" "$baseline_failed" "$baseline_patch_fail" "$baseline_not_run"
printf "%-14s | %8s %8s %8s %8s\n" "with-tags" "$with_resolved" "$with_failed" "$with_patch_fail" "$with_not_run"
echo ""

if [ "$sum_base_den" -gt 0 ] && [ "$sum_with_den" -gt 0 ] && [ "$sum_base_den" -eq "$sum_with_den" ]; then
  echo "Localization sum (best trial, sum of X / sum of Y across tasks with known Y):"
  echo "  baseline:    $sum_base_num / $sum_base_den"
  echo "  with-tags:   $sum_with_num / $sum_with_den"
  echo ""
  echo "Delta (with-tags − baseline): resolved $((with_resolved - baseline_resolved)), +$((sum_with_num - sum_base_num)) / $sum_base_den correct files"
elif [ "$sum_base_den" -gt 0 ] || [ "$sum_with_den" -gt 0 ]; then
  echo "Localization sum (baseline):  $sum_base_num / $sum_base_den"
  echo "Localization sum (with-tags): $sum_with_num / $sum_with_den"
  echo "(Denominators differ or unknown — per-task Y may vary.)"
fi
