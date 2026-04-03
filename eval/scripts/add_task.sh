#!/usr/bin/env bash
set -euo pipefail

# Usage: ./add_task.sh <instance_id>
# Downloads SWE-bench task metadata and sets up a workspace for writing tags.
# Example: ./add_task.sh django__django-13195

TASK_ID="${1:?Usage: add_task.sh <instance_id>}"

EVAL_DIR="$(cd "$(dirname "$0")/.." && pwd)"
TASK_FILE="$EVAL_DIR/tasks/${TASK_ID}.json"
TAG_PATCH="$EVAL_DIR/tasks/${TASK_ID}.tags.diff"
FIXTURE_DIR="/tmp/agent-tags-fixture-${TASK_ID}"

# Step 1: Download task metadata if not present
if [ -f "$TASK_FILE" ]; then
  echo "Task metadata already exists: $TASK_FILE"
else
  echo "==> Downloading task metadata from SWE-bench Verified..."
  python3 -c "
import json
from datasets import load_dataset
ds = load_dataset('princeton-nlp/SWE-bench_Verified', split='test')
for row in ds:
    if row['instance_id'] == '${TASK_ID}':
        with open('${TASK_FILE}', 'w') as f:
            json.dump(row, f, indent=2)
        print('Saved to ${TASK_FILE}')
        break
else:
    print('ERROR: ${TASK_ID} not found in SWE-bench Verified')
    exit(1)
"
fi

# Step 2: Show task info
REPO=$(jq -r '.repo' "$TASK_FILE")
BASE_COMMIT=$(jq -r '.base_commit' "$TASK_FILE")

echo ""
echo "=== Task: $TASK_ID ==="
echo "Repo: $REPO"
echo "Base commit: $BASE_COMMIT"
echo ""
echo "Ground truth files (from patch):"
jq -r '.patch' "$TASK_FILE" | grep '^diff --git' | sed 's|diff --git a/||;s| b/.*||' | sort -u
echo ""
echo "Problem statement (first 5 lines):"
jq -r '.problem_statement' "$TASK_FILE" | head -5
echo "..."
echo ""

# Step 3: Check if tag fixture already exists
if [ -f "$TAG_PATCH" ]; then
  echo "Tag fixture already exists: $TAG_PATCH"
  echo "To regenerate, delete it and re-run this script."
  exit 0
fi

# Step 4: Set up workspace for writing tags
if [ -d "$FIXTURE_DIR" ]; then
  echo "Fixture workspace already exists: $FIXTURE_DIR"
  echo "To start fresh, run: rm -rf $FIXTURE_DIR"
else
  echo "==> Cloning $REPO at $BASE_COMMIT into $FIXTURE_DIR..."
  git clone --quiet "https://github.com/${REPO}.git" "$FIXTURE_DIR"
  git -C "$FIXTURE_DIR" checkout --quiet "$BASE_COMMIT"
  echo "==> Workspace ready."
fi

echo ""
echo "================================================"
echo "  Next steps:"
echo "================================================"
echo ""
echo "  1. cd $FIXTURE_DIR"
echo "  2. Read the source files and write @agents tags"
echo "     - Tag the subsystem broadly, not just the ground truth files"
echo "     - Use named headers: # @agents(name)"
echo "     - Use fragment references: Related: path/file.py#name"
echo "     - Add an AGENTS.md at the repo root"
echo "     - Don't mention the bug or fix in the tags"
echo "  3. Generate the patch:"
echo "     cd $FIXTURE_DIR"
echo "     git add AGENTS.md  # stage new file"
echo "     (git diff; git diff --cached) > $TAG_PATCH"
echo "  4. Clean up: rm -rf $FIXTURE_DIR"
echo ""
echo "  Then run the eval:"
echo "     ./scripts/run_eval.sh $TASK_ID baseline 1"
echo "     ./scripts/run_eval.sh $TASK_ID with-tags 1"
echo "     ./scripts/compare.sh $TASK_ID"
echo ""
