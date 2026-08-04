function requiredEnv(name) {
  const value = process.env[name]?.trim();
  if (!value) {
    throw new Error(`Missing required environment variable: ${name}`);
  }
  return value;
}

const rawScanExitCode = process.env.SCAN_EXIT_CODE?.trim();
const parsedScanExitCode = Number(rawScanExitCode);
const repository = requiredEnv("GITHUB_REPOSITORY");
const serverUrl = process.env.GITHUB_SERVER_URL ?? "https://github.com";

export const config = Object.freeze({
  token: requiredEnv("GITHUB_TOKEN"),
  repository,
  pullNumber: Number(requiredEnv("PR_NUMBER")),
  baseRef: requiredEnv("BASE_REF"),
  baseSha: requiredEnv("BASE_SHA"),
  headSha: requiredEnv("HEAD_SHA"),
  githubOutput: requiredEnv("GITHUB_OUTPUT"),
  mergeRepo: requiredEnv("MERGE_REPO"),
  mergeSha: process.env.MERGE_SHA?.trim() ?? "",
  scanDir: requiredEnv("SCAN_DIR"),
  scanExitCode:
    rawScanExitCode && Number.isInteger(parsedScanExitCode)
      ? parsedScanExitCode
      : 2,
  artifactOutcome: process.env.ARTIFACT_OUTCOME?.trim() ?? "failure",
  scanOutcome: process.env.SCAN_OUTCOME?.trim() ?? "failure",
  serverUrl,
  apiUrl: (process.env.GITHUB_API_URL ?? "https://api.github.com").replace(
    /\/$/,
    "",
  ),
  runUrl: `${serverUrl}/${repository}/actions/runs/${requiredEnv("GITHUB_RUN_ID")}`,
  codexSecurityVersion: process.env.CODEX_SECURITY_VERSION ?? "unknown",
});

if (!Number.isInteger(config.pullNumber) || config.pullNumber < 1) {
  throw new Error("PR_NUMBER must be a positive integer");
}

export const limits = Object.freeze({
  maxInlineComments: 50,
  maxSummaryFindings: 100,
  maxSummaryFindingBytes: 45_000,
  maxSummaryBytes: 60_000,
  githubRequestTimeoutMs: 30_000,
  githubMaxAttempts: 3,
});
