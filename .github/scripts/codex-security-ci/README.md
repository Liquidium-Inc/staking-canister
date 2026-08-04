# Codex Security CI

The workflow file is intentionally only orchestration. Each non-trivial step
lives in a named script in this directory so it can be reviewed and tested as
ordinary shell code.

## Trust boundary

The workflow uses `pull_request_target`, but it never executes CI support code
from the pull-request branch. Every script and the JavaScript reporter are
checked out from the trusted base revision into `trusted-ci`. Only the
repository being scanned comes from the synthetic pull-request merge.

The scanner process receives the subscription-backed Codex credentials but no
GitHub token. GitHub mutations happen before or after the scanner, using trusted
support code.

## Flow

1. `validate-context.sh` verifies the exact PR head and base and creates the
   pending policy check.
2. The scan job checks the revision again inside its concurrency lock.
3. `run-scan.sh` verifies the synthetic merge parents and runs Codex Security
   without a GitHub token.
4. The workflow preserves the scan artifact before invoking the reporter.
5. The reporter publishes inline findings and the sticky summary.
6. `enforce-policy.sh` and `finalize-policy.sh` fail closed unless scanning,
   artifact preservation, and reporting all completed successfully.
7. `finalize-unfinished.sh` closes orphaned checks after failed or cancelled
   jobs.

## Scripts

| Script | Responsibility |
| --- | --- |
| `refresh-open-prs.sh` | Invalidates and redispatches scans when the base branch changes. |
| `publish-closed-policy.sh` | Cancels the policy result for a closed PR revision. |
| `validate-context.sh` | Validates event inputs and creates the pending check run. |
| `publish-fork-policy.sh` | Records the fail-closed result for unsupported fork PRs. |
| `publish-dependabot-policy.sh` | Records the neutral Dependabot exclusion. |
| `revalidate-scan-context.sh` | Rechecks freshness after acquiring the scan lock. |
| `run-scan.sh` | Verifies the merge revision and executes Codex Security. |
| `enforce-policy.sh` | Enforces scanner, artifact, and reporter outcomes. |
| `finalize-policy.sh` | Publishes the terminal policy-check conclusion. |
| `finalize-unfinished.sh` | Recovers and closes checks from interrupted jobs. |

## Reporter

The reporter is split into small modules under `codex-security-report/`:

| Module | Responsibility |
| --- | --- |
| `main.mjs` | Runs the reporting flow in six documented phases. |
| `config.mjs` | Reads and validates workflow inputs and size limits. |
| `github-api.mjs` | Calls GitHub and revalidates the PR before every write. |
| `diff-lines.mjs` | Maps scanner locations to lines added by the PR. |
| `finding-identity.mjs` | Owns finding IDs, markers, state prefixes, and sorting. |
| `render.mjs` | Escapes untrusted scanner text and renders bounded comments. |

Keep the mutation freshness check inside `github-api.mjs`, and route every
scanner-controlled comment value through `escapeMarkdown` in `render.mjs`.

Keep these scripts behaviorally identical across Liquidium repositories. The
only intended workflow difference is the protected base branch.
