#!/usr/bin/env bash
set -euo pipefail

# Recheck the PR snapshot after the scan job acquires its concurrency lock.

echo "current=false" >> "$GITHUB_OUTPUT"
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
test "$(jq -r '.state' <<< "$pull_request")" = "open" || exit 0
test "$(jq -r '.head.sha' <<< "$pull_request")" = "$HEAD_SHA" || exit 0
test "$(jq -r '.base.ref' <<< "$pull_request")" = "$BASE_REF" || exit 0
test "$(jq -r '.base.sha' <<< "$pull_request")" = "$BASE_SHA" || exit 0
echo "current=true" >> "$GITHUB_OUTPUT"
