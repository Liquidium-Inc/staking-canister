#!/usr/bin/env bash
set -euo pipefail

# Verify the synthetic merge revision and execute Codex Security without a GitHub token.

merge_sha="$(git rev-parse HEAD)"
test "$(git rev-parse HEAD^1)" = "$BASE_SHA"
test "$(git rev-parse HEAD^2)" = "$HEAD_SHA"
knowledge_base_args=()
for knowledge_base in README.md AGENTS.md; do
  trusted_knowledge_base="$GITHUB_WORKSPACE/trusted-ci/$knowledge_base"
  if test -f "$trusted_knowledge_base"; then
    knowledge_base_args+=(--knowledge-base "$trusted_knowledge_base")
  fi
done

# Represent the current base-to-merge tree delta as staged changes so the CLI
# binds the scan to a deterministic working-tree digest.
git reset --soft "$BASE_SHA"
log_pipe="$SCAN_LOG.pipe"
mkfifo "$log_pipe"
tee "$SCAN_LOG" <"$log_pipe" >&2 &
log_pid=$!
set +e
timeout --signal=TERM --kill-after=30s 110m \
  "$CODEX_SECURITY_BIN" scan . \
  --working-tree \
  --base "$BASE_SHA" \
  --auth chatgpt \
  --output-dir "$SCAN_DIR" \
  --json \
  --fail-on-severity high \
  "${knowledge_base_args[@]}" \
  >"$SCAN_JSON" \
  2>"$log_pipe"
scan_exit_code=$?
wait "$log_pid"
log_exit_code=$?
unlink "$log_pipe"
if test "$scan_exit_code" -eq 0 && test "$log_exit_code" -ne 0; then
  scan_exit_code=2
fi
set -e

echo "exit_code=$scan_exit_code" >> "$GITHUB_OUTPUT"
echo "merge_sha=$merge_sha" >> "$GITHUB_OUTPUT"
echo "scan_dir=$SCAN_DIR" >> "$GITHUB_OUTPUT"
echo "scan_json=$SCAN_JSON" >> "$GITHUB_OUTPUT"
echo "scan_log=$SCAN_LOG" >> "$GITHUB_OUTPUT"
