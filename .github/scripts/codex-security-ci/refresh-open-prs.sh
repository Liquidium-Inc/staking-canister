#!/usr/bin/env bash
set -euo pipefail

# Collect eligible PRs, fail their stale policy checks, then dispatch replacement scans.

dispatch_failures=0
dispatches_file="$RUNNER_TEMP/codex-security-dispatches.jsonl"
ready_dispatches_file="$RUNNER_TEMP/codex-security-ready-dispatches.jsonl"
: > "$dispatches_file"
: > "$ready_dispatches_file"
page=1
while true; do
  pull_requests=""
  for request_attempt in 1 2 3; do
    if pull_requests="$(curl \
      --silent \
      --show-error \
      --fail-with-body \
      --max-time 30 \
      --header "Accept: application/vnd.github+json" \
      --header "Authorization: Bearer $GITHUB_TOKEN" \
      --header "X-GitHub-Api-Version: 2022-11-28" \
      "$GITHUB_API_URL/repos/$GITHUB_REPOSITORY/pulls?state=open&base=$BASE_REF&per_page=100&page=$page")"; then
      break
    fi
    pull_requests=""
    test "$request_attempt" -lt 3 || exit 1
    sleep 1
  done
  jq -c \
    --arg repository "$GITHUB_REPOSITORY" \
    '.[] | select(.head.repo.full_name == $repository) | select(.user.login != "dependabot[bot]") | {
      ref: .base.ref,
      inputs: {
        pr_number: (.number | tostring),
        head_sha: .head.sha,
        base_ref: .base.ref,
        base_sha: .base.sha
      }
    }' <<< "$pull_requests" >> "$dispatches_file"
  if test "$(jq 'length' <<< "$pull_requests")" -lt 100; then
    break
  fi
  page=$((page + 1))
done

while IFS= read -r dispatch; do
  dispatch_head_sha="$(jq -r '.inputs.head_sha' <<< "$dispatch")"
  dispatch_pr_number="$(jq -r '.inputs.pr_number' <<< "$dispatch")"
  invalidation_external_id="refresh:$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$dispatch_pr_number"
  invalidation_payload="$(jq -n \
    --arg external_id "$invalidation_external_id" \
    --arg head_sha "$dispatch_head_sha" \
    '{name: "Codex Security / policy", head_sha: $head_sha, status: "completed", conclusion: "failure", external_id: $external_id, output: {title: "Codex Security policy", summary: "The target branch changed and this pull request has not yet been rescanned."}}')"
  invalidation_confirmed=false
  if curl \
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
    --data-binary "$invalidation_payload" \
    "$GITHUB_API_URL/repos/$GITHUB_REPOSITORY/check-runs"; then
    invalidation_confirmed=true
  else
    existing_checks=""
    for request_attempt in 1 2 3; do
      if existing_checks="$(curl \
        --silent \
        --show-error \
        --fail-with-body \
        --max-time 30 \
        --header "Accept: application/vnd.github+json" \
        --header "Authorization: Bearer $GITHUB_TOKEN" \
        --header "X-GitHub-Api-Version: 2022-11-28" \
        "$GITHUB_API_URL/repos/$GITHUB_REPOSITORY/commits/$dispatch_head_sha/check-runs?check_name=Codex%20Security%20%2F%20policy&filter=all&per_page=100")"; then
        break
      fi
      existing_checks=""
      test "$request_attempt" -lt 3 || exit 1
      sleep 1
    done
    if jq -e \
      --arg external_id "$invalidation_external_id" \
      '.check_runs | any(.external_id == $external_id)' \
      <<< "$existing_checks" >/dev/null; then
      invalidation_confirmed=true
    fi
  fi
  if test "$invalidation_confirmed" = true; then
    echo "$dispatch" >> "$ready_dispatches_file"
  else
    dispatch_failures=$((dispatch_failures + 1))
  fi
done < "$dispatches_file"

while IFS= read -r dispatch; do
  if ! curl \
    --silent \
    --show-error \
    --fail-with-body \
    --max-time 30 \
    --request POST \
    --header "Accept: application/vnd.github+json" \
    --header "Authorization: Bearer $GITHUB_TOKEN" \
    --header "X-GitHub-Api-Version: 2022-11-28" \
    --data-binary "$dispatch" \
    "$GITHUB_API_URL/repos/$GITHUB_REPOSITORY/actions/workflows/codex-security.yml/dispatches"; then
    dispatch_failures=$((dispatch_failures + 1))
  fi
done < "$ready_dispatches_file"
test "$dispatch_failures" -eq 0
