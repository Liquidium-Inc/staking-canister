#!/usr/bin/env bash
set -euo pipefail

# Publish the terminal policy result for the current PR snapshot.

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
current_head="$(jq -r '.head.sha' <<< "$pull_request")"
current_base="$(jq -r '.base.ref' <<< "$pull_request")"
current_base_sha="$(jq -r '.base.sha' <<< "$pull_request")"
current_state="$(jq -r '.state' <<< "$pull_request")"
revision_mismatch=false
if test "$current_state" != "open" || \
  test "$current_head" != "$HEAD_SHA" || \
  test "$current_base" != "$BASE_REF" || \
  test "$current_base_sha" != "$BASE_SHA"; then
  revision_mismatch=true
fi
if test "$revision_mismatch" = true || \
  { test "$SCAN_CONTEXT_OUTCOME" = "success" && test "$SCAN_CURRENT" != "true"; }; then
  stale_payload="$(jq -n \
    --arg target_url "$RUN_URL" \
    '{status: "completed", conclusion: "cancelled", details_url: $target_url, output: {title: "Codex Security policy", summary: "This scan was superseded by a newer pull-request revision or the pull request was closed."}}')"
  curl \
    --silent \
    --show-error \
    --fail-with-body \
    --max-time 30 \
    --retry 2 \
    --retry-delay 1 \
    --retry-all-errors \
    --request PATCH \
    --header "Accept: application/vnd.github+json" \
    --header "Authorization: Bearer $GITHUB_TOKEN" \
    --header "X-GitHub-Api-Version: 2022-11-28" \
    --data-binary "$stale_payload" \
    "$GITHUB_API_URL/repos/$GITHUB_REPOSITORY/check-runs/$POLICY_CHECK_ID"
  exit 0
fi

policy_state="$POLICY_STATE"
policy_description="$POLICY_DESCRIPTION"
if test "$REPORT_OUTCOME" != "success" || \
  test "$ARTIFACT_OUTCOME" != "success" || \
  { test "$policy_state" != "success" && test "$policy_state" != "failure"; } || \
  test -z "$policy_description"; then
  policy_state="failure"
  policy_description="Codex Security could not publish and preserve a complete report."
fi
check_payload="$(jq -n \
  --arg conclusion "$policy_state" \
  --arg description "$policy_description" \
  --arg target_url "$RUN_URL" \
  '{status: "completed", conclusion: $conclusion, details_url: $target_url, output: {title: "Codex Security policy", summary: $description}}')"

curl \
  --silent \
  --show-error \
  --fail-with-body \
  --max-time 30 \
  --retry 2 \
  --retry-delay 1 \
  --retry-all-errors \
  --request PATCH \
  --header "Accept: application/vnd.github+json" \
  --header "Authorization: Bearer $GITHUB_TOKEN" \
  --header "X-GitHub-Api-Version: 2022-11-28" \
  --data-binary "$check_payload" \
  "$GITHUB_API_URL/repos/$GITHUB_REPOSITORY/check-runs/$POLICY_CHECK_ID"
