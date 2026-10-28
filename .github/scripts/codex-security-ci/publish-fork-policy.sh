#!/usr/bin/env bash
set -euo pipefail

# Fail closed for fork PRs that cannot use subscription-backed scanning.

check_payload="$(jq -n \
  --arg external_id "fork:$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT" \
  --arg head_sha "$HEAD_SHA" \
  --arg target_url "$RUN_URL" \
  '{name: "Codex Security / policy", head_sha: $head_sha, status: "completed", conclusion: "failure", external_id: $external_id, details_url: $target_url, output: {title: "Codex Security policy", summary: "Fork pull requests require a trusted same-repository branch for subscription-backed scanning."}}')"
curl \
  --silent \
  --show-error \
  --fail-with-body \
  --max-time 30 \
  --retry 2 \
  --retry-delay 1 \
  --retry-all-errors \
  --request POST \
  --header "Accept: application/vnd.github+json" \
  --header "Authorization: Bearer $GITHUB_TOKEN" \
  --header "X-GitHub-Api-Version: 2022-11-28" \
  --data-binary "$check_payload" \
  "$GITHUB_API_URL/repos/$GITHUB_REPOSITORY/check-runs"
