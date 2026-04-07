#!/usr/bin/env bash
set -euo pipefail

# Usage:
#   ./run_all.sh [num_trials]              # all tasks that have .json + .tags.diff
#   ./run_all.sh [num_trials] <cohort>     # only task IDs listed in tasks/manifest.json cohorts.<name>
# Examples:
#   ./run_all.sh
#   ./run_all.sh 3
#   ./run_all.sh 1 initial
#   ./run_all.sh 3 initial

TRIALS=1
COHORT=""
if [ "${1:-}" != "" ] && [[ "$1" =~ ^[0-9]+$ ]]; then
  TRIALS="$1"
  shift
fi
if [ "${1:-}" != "" ]; then
  COHORT="$1"
  shift
fi
if [ "${1:-}" != "" ]; then
  echo "Usage: run_all.sh [num_trials] [cohort_name]" >&2
  exit 1
fi

EVAL_DIR="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$EVAL_DIR/tasks/manifest.json"
VALIDATE="$EVAL_DIR/scripts/validate_fixture.sh"

TASKS=()

if [ -n "$COHORT" ]; then
  if [ ! -f "$MANIFEST" ]; then
    echo "Error: cohort '$COHORT' requested but $MANIFEST not found" >&2
    exit 1
  fi
  cnt=$(jq --arg c "$COHORT" '.cohorts[$c] | length' "$MANIFEST")
  if [ "$cnt" = "null" ] || [ "$cnt" -eq 0 ]; then
    echo "Error: unknown or empty cohort '$COHORT' (check tasks/manifest.json)" >&2
    exit 1
  fi
  while IFS= read -r task_id; do
    [ -n "$task_id" ] || continue
    json_file="$EVAL_DIR/tasks/${task_id}.json"
    tag_file="$EVAL_DIR/tasks/${task_id}.tags.diff"
    if [ -f "$json_file" ] && [ -f "$tag_file" ]; then
      TASKS+=("$task_id")
    else
      echo "Skipping $task_id (missing .json or .tags.diff for cohort '$COHORT')" >&2
    fi
  done < <(jq -r --arg c "$COHORT" '.cohorts[$c][]' "$MANIFEST")
else
  for json_file in "$EVAL_DIR"/tasks/*.json; do
    [ -f "$json_file" ] || continue
    task_id=$(basename "$json_file" .json)
    if [ "$task_id" = "manifest" ]; then
      continue
    fi
    tag_file="$EVAL_DIR/tasks/${task_id}.tags.diff"
    if [ -f "$tag_file" ]; then
      TASKS+=("$task_id")
    else
      echo "Skipping $task_id (no .tags.diff fixture)"
    fi
  done
fi

if [ ${#TASKS[@]} -eq 0 ]; then
  echo "No tasks to run."
  exit 1
fi

echo "=== Running ${#TASKS[@]} task(s) x 2 conditions x $TRIALS trial(s) ==="
if [ -n "$COHORT" ]; then
  echo "    Cohort: $COHORT"
fi
echo ""

for task_id in "${TASKS[@]}"; do
  echo "-- validate: $task_id --"
  "$VALIDATE" "$task_id"
done
echo ""

for task_id in "${TASKS[@]}"; do
  for trial in $(seq 1 "$TRIALS"); do
    for condition in baseline with-tags; do
      result_dir="$EVAL_DIR/results/$task_id/$condition/trial-$trial"
      if [ -d "$result_dir" ] && [ -f "$result_dir/agent_output.json" ]; then
        echo "Skipping $task_id $condition trial-$trial (already exists)"
        continue
      fi
      echo "--- $task_id | $condition | trial $trial ---"
      "$EVAL_DIR/scripts/run_eval.sh" "$task_id" "$condition" "$trial"
      echo ""
    done
  done
done

echo "=== All runs complete. Run ./scripts/summary.sh for results. ==="
