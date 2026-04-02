#!/usr/bin/env bash
set -euo pipefail

# Usage: ./run_all.sh [num_trials]
# Runs both conditions for all tasks that have a .json and .tags.diff.
# Example: ./run_all.sh       # 1 trial each
#          ./run_all.sh 3     # 3 trials each

TRIALS="${1:-1}"
EVAL_DIR="$(cd "$(dirname "$0")/.." && pwd)"

# Find all tasks with both metadata and tag fixtures
TASKS=()
for json_file in "$EVAL_DIR"/tasks/*.json; do
  [ -f "$json_file" ] || continue
  task_id=$(basename "$json_file" .json)
  tag_file="$EVAL_DIR/tasks/${task_id}.tags.diff"
  if [ -f "$tag_file" ]; then
    TASKS+=("$task_id")
  else
    echo "Skipping $task_id (no .tags.diff fixture)"
  fi
done

if [ ${#TASKS[@]} -eq 0 ]; then
  echo "No tasks with tag fixtures found."
  exit 1
fi

echo "=== Running ${#TASKS[@]} task(s) x 2 conditions x $TRIALS trial(s) ==="
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
