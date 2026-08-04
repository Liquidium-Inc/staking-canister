#!/usr/bin/env bash
set -euo pipefail

# Record the explicit neutral policy result for Dependabot PRs.

check_payload="$(jq -n \
  --arg external_id "dependabot:$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT" \
  --arg head_sha "$HEAD_SHA" \
  --arg target_url "$RUN_URL" \
  '{name: "Codex Security / policy", head_sha: $head_sha, status: "completed", conclusion: "neutral", external_id: $external_id, details_url: $target_url, output: {title: "Codex Security policy", summary: "Dependabot pull requests are excluded from subscription-backed self-hosted scans, matching OpenAI’s published CI boundary."}}')"
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
