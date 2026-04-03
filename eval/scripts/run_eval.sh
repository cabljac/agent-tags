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

# Write problem statement to a file (avoids quoting issues)
jq -r '.problem_statement' "$TASK_FILE" > "$WORK_DIR/problem.txt"

# If with-tags condition, apply pre-written tag fixture
APPEND_PROMPT=""
if [ "$CONDITION" = "with-tags" ]; then
  TAG_PATCH="$EVAL_DIR/tasks/${TASK_ID}.tags.diff"
  if [ ! -f "$TAG_PATCH" ]; then
    echo "Error: tag fixture not found: $TAG_PATCH"
    exit 1
  fi
  echo "==> Applying @agents tag fixture"
  git -C "$WORK_DIR/repo" apply "$TAG_PATCH"
  git -C "$WORK_DIR/repo" add -A
  git -C "$WORK_DIR/repo" commit --quiet -m "Add @agents tags (eval fixture)"
  echo "==> Tags applied and committed"

  APPEND_PROMPT='IMPORTANT: This repo uses @agents tags. You MUST run `git agent-tags context` BEFORE reading or editing any files. This outputs a map of all annotated files and their cross-file dependencies. Read the output carefully — Related: links tell you which files depend on each other. If you change a function signature, you MUST follow Related: links to find and update all callers. Do not stop until you have checked every Related: file that could be affected by your changes.'
fi

# Build the agent prompt
cat > "$WORK_DIR/agent_prompt.txt" <<AGENTEOF
You are working on the repository: ${REPO}

Fix the following GitHub issue. Read the relevant code, understand the problem,
and make the minimal code changes needed to fix it. Do not modify tests.

ISSUE:
$(cat "$WORK_DIR/problem.txt")

Fix this issue by editing the source code. When you are done, stop.
AGENTEOF

mkdir -p "$RESULTS_DIR"

# Build claude args
CLAUDE_ARGS=(
  -p "$(cat "$WORK_DIR/agent_prompt.txt")"
  --allowedTools 'Edit' 'Write' 'Read' 'Glob' 'Grep' 'Bash(git:*)' 'Bash(find:*)' 'Bash(ls:*)'
  --output-format json
)
if [ -n "$APPEND_PROMPT" ]; then
  CLAUDE_ARGS+=(--append-system-prompt "$APPEND_PROMPT")
fi

# Run claude and capture output
echo "==> Running agent ($CONDITION, trial $TRIAL)"
(cd "$WORK_DIR/repo" && claude "${CLAUDE_ARGS[@]}") > "$RESULTS_DIR/agent_output.json" 2>&1

# Capture the diff (from the repo dir)
git -C "$WORK_DIR/repo" diff > "$RESULTS_DIR/patch.diff"
git -C "$WORK_DIR/repo" diff --stat > "$RESULTS_DIR/patch_stat.txt"

# Record which files were changed
git -C "$WORK_DIR/repo" diff --name-only > "$RESULTS_DIR/files_changed.txt"

# Save the prompt and system prompt used (for debugging)
cp "$WORK_DIR/agent_prompt.txt" "$RESULTS_DIR/prompt.txt"
echo "$APPEND_PROMPT" > "$RESULTS_DIR/system_prompt_append.txt"

echo "==> Results saved to $RESULTS_DIR"
echo "Files changed:"
cat "$RESULTS_DIR/files_changed.txt"
echo ""
echo "Diff stat:"
cat "$RESULTS_DIR/patch_stat.txt"
