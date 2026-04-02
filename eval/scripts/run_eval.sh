#!/usr/bin/env bash
set -euo pipefail

# Usage: ./run_eval.sh <task_id> <condition> [trial_num]
# condition: "baseline" or "with-tags"
# Example: ./run_eval.sh django__django-13195 baseline 1

TASK_ID="${1:?Usage: run_eval.sh <task_id> <condition> [trial_num]}"
CONDITION="${2:?Usage: run_eval.sh <task_id> <condition|baseline|with-tags> [trial_num]}"
TRIAL="${3:-1}"

EVAL_DIR="$(cd "$(dirname "$0")/.." && pwd)"
TASK_FILE="$EVAL_DIR/tasks/${TASK_ID}.json"
RESULTS_DIR="$EVAL_DIR/results/${TASK_ID}/${CONDITION}/trial-${TRIAL}"

if [ ! -f "$TASK_FILE" ]; then
  echo "Error: task file not found: $TASK_FILE"
  exit 1
fi

# Parse task metadata
REPO=$(jq -r '.repo' "$TASK_FILE")
BASE_COMMIT=$(jq -r '.base_commit' "$TASK_FILE")
PROBLEM=$(jq -r '.problem_statement' "$TASK_FILE")
FAIL_TO_PASS=$(jq -r '.FAIL_TO_PASS' "$TASK_FILE")

# Set up workspace
WORK_DIR=$(mktemp -d "/tmp/agent-tags-eval-${TASK_ID}-XXXXXX")
echo "Workspace: $WORK_DIR"

cleanup() {
  echo "Cleaning up $WORK_DIR"
  rm -rf "$WORK_DIR"
}
trap cleanup EXIT

# Clone and checkout
echo "==> Cloning $REPO at $BASE_COMMIT"
git clone --quiet "https://github.com/${REPO}.git" "$WORK_DIR/repo"
cd "$WORK_DIR/repo"
git checkout --quiet "$BASE_COMMIT"

# If with-tags condition, generate tags first
if [ "$CONDITION" = "with-tags" ]; then
  echo "==> Generating @agents tags"
  claude -p "$(cat <<PROMPT
You are preparing a codebase for another AI agent to work on. Read the repository
structure and add @agents file headers to the most important source files.

Focus on files that:
- Are core modules (not tests, not config)
- Have non-obvious relationships to other files
- Have constraints a future editor should know about

For each file, add a comment block in the first few lines using the file's
native comment style:

For Python:
# @agents
# <what this file does>
# Related: <path/to/related/file.py>

For JavaScript/TypeScript:
// @agents
// <what this file does>
// Related: <path/to/related/file.ts>

Rules:
- Paths in Related: must be repo-root-relative
- Only add tags to files you've actually read
- Be terse and specific — what it does, what it relates to, what constraints apply
- Don't tag every file — focus on the 10-20 most important ones
- Don't modify any logic, only add comment headers

Do this now. Don't ask questions, just read the code and add the tags.
PROMPT
  )" > "$WORK_DIR/tag_generation.log" 2>&1
  echo "==> Tags generated (log: $WORK_DIR/tag_generation.log)"
fi

# Run the agent on the issue
echo "==> Running agent ($CONDITION, trial $TRIAL)"
AGENT_PROMPT="$(cat <<PROMPT
You are working on the repository: ${REPO}

Fix the following GitHub issue. Read the relevant code, understand the problem,
and make the minimal code changes needed to fix it. Do not modify tests.

ISSUE:
${PROBLEM}

Fix this issue by editing the source code. When you are done, stop.
PROMPT
)"

mkdir -p "$RESULTS_DIR"

# Run claude and capture output
claude -p "$AGENT_PROMPT" > "$RESULTS_DIR/agent_output.txt" 2>&1

# Capture the diff
git diff > "$RESULTS_DIR/patch.diff"
git diff --stat > "$RESULTS_DIR/patch_stat.txt"

# Record which files were changed
git diff --name-only > "$RESULTS_DIR/files_changed.txt"

echo "==> Results saved to $RESULTS_DIR"
echo "Files changed:"
cat "$RESULTS_DIR/files_changed.txt"
echo ""
echo "Diff stat:"
cat "$RESULTS_DIR/patch_stat.txt"
