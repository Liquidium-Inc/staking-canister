#!/usr/bin/env bash
set -euo pipefail

# Publish a terminal non-success policy check for a closed PR revision.

pull_request=""
for request_attempt in 1 2 3; do
  if pull_request="$(curl \
    --silent \
    --show-error \
    --fail-with-body \
    --max-time 30 \
    --header "Accept: application/vnd.github+json" \
    --header "Authorization: Bearer $GITHUB_TOKEN" \
    --header "X-GitHub-Api-Version: 2022-11-28" \
    "$GITHUB_API_URL/repos/$GITHUB_REPOSITORY/pulls/$PR_NUMBER")"; then
    break
  fi
  pull_request=""
  test "$request_attempt" -lt 3 || exit 1
  sleep 1
done
test "$(jq -r '.state' <<< "$pull_request")" = "closed" || exit 0
test "$(jq -r '.head.sha' <<< "$pull_request")" = "$HEAD_SHA" || exit 0
check_payload="$(jq -n \
  --arg external_id "closed:$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT" \
  --arg head_sha "$HEAD_SHA" \
  --arg target_url "$RUN_URL" \
  '{name: "Codex Security / policy", head_sha: $head_sha, status: "completed", conclusion: "cancelled", external_id: $external_id, details_url: $target_url, output: {title: "Codex Security policy", summary: "The pull request was closed before this revision completed the current security policy."}}')"
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
