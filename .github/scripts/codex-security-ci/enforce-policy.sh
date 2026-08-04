#!/usr/bin/env bash
set -euo pipefail

# Fail unless scanning, artifact preservation, reporting, and policy evaluation succeeded.

if test "$SCAN_EXIT_CODE" -ne 0; then
  exit "$SCAN_EXIT_CODE"
fi
test "$SCAN_OUTCOME" = "success"
test "$REPORT_OUTCOME" = "success"
test "$ARTIFACT_OUTCOME" = "success"
test "$POLICY_STATE" = "success"
