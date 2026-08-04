#!/usr/bin/env bash
set -euo pipefail

# Validate the event snapshot and create this run's pending policy check.

echo "current=false" >> "$GITHUB_OUTPUT"
if test "$EVENT_ACTION" = "closed"; then
  exit 0
fi

pull_request=""
for request_attempt in 1 2 3; do
  if pull_request="$(curl --silent --show-error --fail-with-body --max-time 30 \
    --header "Accept: application/vnd.github+json" \
    --header "Authorization: Bearer $GITHUB_TOKEN" \
    --header "X-GitHub-Api-Version: 2022-11-28" \
    "$GITHUB_API_URL/repos/$GITHUB_REPOSITORY/pulls/$PR_NUMBER")"; then break; fi
  pull_request=""
  test "$request_attempt" -lt 3 || exit 1
  sleep 1
done
current_head="$(jq -r '.head.sha' <<< "$pull_request")"
current_head_repository="$(jq -r '.head.repo.full_name' <<< "$pull_request")"
current_author="$(jq -r '.user.login' <<< "$pull_request")"
current_base="$(jq -r '.base.ref' <<< "$pull_request")"
current_base_sha="$(jq -r '.base.sha' <<< "$pull_request")"
current_state="$(jq -r '.state' <<< "$pull_request")"
if test "$current_state" != "open" || \
  test "$current_head_repository" != "$GITHUB_REPOSITORY" || \
  test "$current_author" = "dependabot[bot]" || \
  test "$current_head" != "$HEAD_SHA" || \
  test "$current_base" != "$BASE_REF" || \
  test "$current_base_sha" != "$BASE_SHA"; then
  exit 0
fi

pending_payload="$(jq -n \
  --arg external_id "$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT" \
  --arg head_sha "$HEAD_SHA" \
  --arg target_url "$RUN_URL" \
  '{name: "Codex Security / policy", head_sha: $head_sha, status: "in_progress", external_id: $external_id, details_url: $target_url}')"
if ! policy_check="$(curl \
  --silent \
  --show-error \
  --fail-with-body \
  --max-time 30 \
  --request POST \
  --header "Accept: application/vnd.github+json" \
  --header "Authorization: Bearer $GITHUB_TOKEN" \
  --header "X-GitHub-Api-Version: 2022-11-28" \
  --data-binary "$pending_payload" \
  "$GITHUB_API_URL/repos/$GITHUB_REPOSITORY/check-runs")"; then
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
  policy_check="$(jq -c \
    --arg external_id "$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT" \
    '[.check_runs[] | select(.external_id == $external_id)] | max_by(.id) // empty' \
    <<< "$existing_checks")"
  if test -z "$policy_check"; then
    if ! policy_check="$(curl \
      --silent \
      --show-error \
      --fail-with-body \
      --max-time 30 \
      --request POST \
      --header "Accept: application/vnd.github+json" \
      --header "Authorization: Bearer $GITHUB_TOKEN" \
      --header "X-GitHub-Api-Version: 2022-11-28" \
      --data-binary "$pending_payload" \
      "$GITHUB_API_URL/repos/$GITHUB_REPOSITORY/check-runs")"; then
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
      policy_check="$(jq -c \
        --arg external_id "$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT" \
        '[.check_runs[] | select(.external_id == $external_id)] | max_by(.id) // empty' \
        <<< "$existing_checks")"
    fi
  fi
  test -n "$policy_check"
fi
policy_check_id="$(jq -r '.id // empty' <<< "$policy_check")"
echo "policy_check_id=$policy_check_id" >> "$GITHUB_OUTPUT"
[[ "$policy_check_id" =~ ^[0-9]+$ ]]
echo "base_ref=$current_base" >> "$GITHUB_OUTPUT"
echo "base_sha=$current_base_sha" >> "$GITHUB_OUTPUT"
echo "current=true" >> "$GITHUB_OUTPUT"
echo "head_sha=$current_head" >> "$GITHUB_OUTPUT"
echo "pr_number=$PR_NUMBER" >> "$GITHUB_OUTPUT"
