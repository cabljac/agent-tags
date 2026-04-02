#!/usr/bin/env bash
set -euo pipefail

# Usage: ./evaluate.sh <task_id> <condition> [trial_num]
# Checks if the agent's patch resolves the failing tests.

TASK_ID="${1:?Usage: evaluate.sh <task_id> <condition> [trial_num]}"
CONDITION="${2:?Usage: evaluate.sh <task_id> <condition|baseline|with-tags> [trial_num]}"
TRIAL="${3:-1}"

EVAL_DIR="$(cd "$(dirname "$0")/.." && pwd)"
TASK_FILE="$EVAL_DIR/tasks/${TASK_ID}.json"
RESULTS_DIR="$EVAL_DIR/results/${TASK_ID}/${CONDITION}/trial-${TRIAL}"
PATCH_FILE="$RESULTS_DIR/patch.diff"

if [ ! -f "$PATCH_FILE" ]; then
  echo "Error: no patch found at $PATCH_FILE. Run run_eval.sh first."
  exit 1
fi

REPO=$(jq -r '.repo' "$TASK_FILE")
BASE_COMMIT=$(jq -r '.base_commit' "$TASK_FILE")
FAIL_TO_PASS=$(jq -r '.FAIL_TO_PASS' "$TASK_FILE")
TEST_PATCH=$(jq -r '.test_patch // empty' "$TASK_FILE")

WORK_DIR=$(mktemp -d "/tmp/agent-tags-verify-${TASK_ID}-XXXXXX")
echo "Verify workspace: $WORK_DIR"

cleanup() {
  rm -rf "$WORK_DIR"
}
trap cleanup EXIT

# Clone and checkout
git clone --quiet "https://github.com/${REPO}.git" "$WORK_DIR/repo"
cd "$WORK_DIR/repo"
git checkout --quiet "$BASE_COMMIT"

# Apply the test patch (adds the FAIL_TO_PASS tests)
if [ -n "$TEST_PATCH" ]; then
  echo "$TEST_PATCH" | git apply --allow-empty -
  echo "==> Applied test patch"
fi

# Apply the agent's patch
if ! git apply --allow-empty "$PATCH_FILE" 2>"$RESULTS_DIR/apply_error.txt"; then
  echo "RESULT: PATCH_APPLY_FAILED"
  echo "PATCH_APPLY_FAILED" > "$RESULTS_DIR/result.txt"
  cat "$RESULTS_DIR/apply_error.txt"
  exit 1
fi
echo "==> Applied agent patch"

# Run the failing tests
echo "==> Running FAIL_TO_PASS tests: $FAIL_TO_PASS"

# Parse the test list (JSON array as string)
TESTS=$(echo "$FAIL_TO_PASS" | jq -r '.[]' 2>/dev/null || echo "$FAIL_TO_PASS")

TEST_PASSED=true
for test in $TESTS; do
  echo "  Running: $test"
  if ! python -m pytest "$test" -x --tb=short > "$RESULTS_DIR/test_output.txt" 2>&1; then
    echo "  FAILED: $test"
    TEST_PASSED=false
  else
    echo "  PASSED: $test"
  fi
done

if [ "$TEST_PASSED" = true ]; then
  echo "RESULT: RESOLVED"
  echo "RESOLVED" > "$RESULTS_DIR/result.txt"
else
  echo "RESULT: FAILED"
  echo "FAILED" > "$RESULTS_DIR/result.txt"
fi

# File localization analysis
echo ""
echo "==> File localization analysis"
GROUND_TRUTH=$(jq -r '.patch' "$TASK_FILE" | grep '^diff --git' | sed 's|diff --git a/||;s| b/.*||' | sort -u)
AGENT_FILES=$(cat "$RESULTS_DIR/files_changed.txt" | sort -u)

echo "Ground truth files:"
echo "$GROUND_TRUTH"
echo ""
echo "Agent touched files:"
echo "$AGENT_FILES"
echo ""

# Calculate overlap
CORRECT=0
TOTAL_GT=$(echo "$GROUND_TRUTH" | wc -l | tr -d ' ')
for f in $GROUND_TRUTH; do
  if echo "$AGENT_FILES" | grep -qF "$f"; then
    CORRECT=$((CORRECT + 1))
  fi
done
echo "File localization: $CORRECT / $TOTAL_GT ground truth files found"
echo "$CORRECT/$TOTAL_GT" > "$RESULTS_DIR/localization.txt"
