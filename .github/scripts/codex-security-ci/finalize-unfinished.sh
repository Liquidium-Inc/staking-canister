#!/usr/bin/env bash
set -euo pipefail

# Recover and close policy checks left pending by failed or cancelled jobs.

if test -z "$POLICY_CHECK_ID"; then
  existing_checks="$(curl \
    --silent \
    --show-error \
    --fail-with-body \
    --max-time 30 \
    --retry 2 \
    --retry-delay 1 \
    --retry-all-errors \
    --header "Accept: application/vnd.github+json" \
    --header "Authorization: Bearer $GITHUB_TOKEN" \
    --header "X-GitHub-Api-Version: 2022-11-28" \
    "$GITHUB_API_URL/repos/$GITHUB_REPOSITORY/commits/$HEAD_SHA/check-runs?check_name=Codex%20Security%20%2F%20policy&filter=all&per_page=100")"
  POLICY_CHECK_ID="$(jq -r \
    --arg external_id "$EXTERNAL_ID" \
    '[.check_runs[] | select(.external_id == $external_id)] | max_by(.id).id // empty' \
    <<< "$existing_checks")"
  test -n "$POLICY_CHECK_ID" || exit 0
fi
policy_check=""
for request_attempt in 1 2 3; do
  if policy_check="$(curl --silent --show-error --fail-with-body --max-time 30 \
    --header "Accept: application/vnd.github+json" \
    --header "Authorization: Bearer $GITHUB_TOKEN" \
    --header "X-GitHub-Api-Version: 2022-11-28" \
    "$GITHUB_API_URL/repos/$GITHUB_REPOSITORY/check-runs/$POLICY_CHECK_ID")"; then break; fi
  policy_check=""
  test "$request_attempt" -lt 3 || exit 1
  sleep 1
done
test "$(jq -r '.status' <<< "$policy_check")" != "completed" || exit 0
conclusion="failure"
summary="Codex Security scan failed before publishing a terminal policy result."
if test "$CONTEXT_RESULT" != "success"; then
  summary="Codex Security could not validate the pull-request context."
elif test "$SCAN_RESULT" = "cancelled"; then
  conclusion="cancelled"
  summary="Codex Security scan was cancelled before completion."
fi
check_payload="$(jq -n \
  --arg conclusion "$conclusion" \
  --arg summary "$summary" \
  --arg target_url "$RUN_URL" \
  '{status: "completed", conclusion: $conclusion, details_url: $target_url, output: {title: "Codex Security policy", summary: $summary}}')"
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
