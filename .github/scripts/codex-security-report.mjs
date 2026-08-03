import { appendFile, readFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

const SUMMARY_MARKER = "<!-- codex-security-summary -->";
const STALE_PREFIX =
  "> ⚪ **No longer reported by the latest complete Codex Security scan.**\n\n";
const RELOCATED_PREFIX =
  "> 🔄 **Superseded by the current Codex Security report for this active finding.**\n\n";
const SUMMARY_ONLY_PREFIX =
  "> 🔄 **Still active, but now reported in the sticky summary because it cannot be anchored to a changed line in this PR revision.**\n\n";
const MAX_INLINE_COMMENTS = 50;
const MAX_SUMMARY_FINDINGS = 100;
const MAX_SUMMARY_FINDING_BYTES = 45_000;
const MAX_SUMMARY_BYTES = 60_000;
const GITHUB_REQUEST_TIMEOUT_MS = 30_000;
const GITHUB_MAX_ATTEMPTS = 3;

function requiredEnv(name) {
  const value = process.env[name]?.trim();
  if (!value) {
    throw new Error(`Missing required environment variable: ${name}`);
  }
  return value;
}

const token = requiredEnv("GITHUB_TOKEN");
const repository = requiredEnv("GITHUB_REPOSITORY");
const pullNumber = Number(requiredEnv("PR_NUMBER"));
const baseRef = requiredEnv("BASE_REF");
const baseSha = requiredEnv("BASE_SHA");
const headSha = requiredEnv("HEAD_SHA");
const githubOutput = requiredEnv("GITHUB_OUTPUT");
const mergeRepo = requiredEnv("MERGE_REPO");
const mergeSha = process.env.MERGE_SHA?.trim() ?? "";
const scanDir = requiredEnv("SCAN_DIR");
const rawScanExitCode = process.env.SCAN_EXIT_CODE?.trim();
const parsedScanExitCode = Number(rawScanExitCode);
const scanExitCode =
  rawScanExitCode && Number.isInteger(parsedScanExitCode)
    ? parsedScanExitCode
    : 2;
const artifactOutcome = process.env.ARTIFACT_OUTCOME?.trim() ?? "failure";
const serverUrl = process.env.GITHUB_SERVER_URL ?? "https://github.com";
const apiUrl = (process.env.GITHUB_API_URL ?? "https://api.github.com").replace(
  /\/$/,
  "",
);
const runUrl = `${serverUrl}/${repository}/actions/runs/${requiredEnv("GITHUB_RUN_ID")}`;
const codexSecurityVersion = process.env.CODEX_SECURITY_VERSION ?? "unknown";

if (!Number.isInteger(pullNumber) || pullNumber < 1) {
  throw new Error("PR_NUMBER must be a positive integer");
}

async function github(path, options = {}) {
  const method = options.method ?? "GET";
  if (method !== "GET" && options.revalidate !== false) {
    await assertCurrentPullRequest();
  }
  const maxAttempts =
    method === "POST" && options.retry !== true ? 1 : GITHUB_MAX_ATTEMPTS;
  for (let attempt = 1; attempt <= maxAttempts; attempt += 1) {
    const controller = new AbortController();
    const timeout = setTimeout(
      () => controller.abort(),
      GITHUB_REQUEST_TIMEOUT_MS,
    );
    let response;
    let responseBody;
    try {
      response = await fetch(`${apiUrl}${path}`, {
        method,
        headers: {
          Accept: "application/vnd.github+json",
          Authorization: `Bearer ${token}`,
          "User-Agent": "liquidium-codex-security-ci",
          "X-GitHub-Api-Version": "2022-11-28",
        },
        body:
          options.body === undefined ? undefined : JSON.stringify(options.body),
        signal: controller.signal,
      });
      responseBody = response.status === 204 ? "" : await response.text();
    } catch (error) {
      if (attempt === maxAttempts) {
        const detail = error instanceof Error ? error.message : String(error);
        throw new Error(
          `GitHub API ${method} ${path} failed after ${attempt} attempts: ${detail}`,
        );
      }
      await new Promise((resolve) => setTimeout(resolve, attempt * 1000));
      continue;
    } finally {
      clearTimeout(timeout);
    }

    if (response.ok) {
      return response.status === 204 ? null : JSON.parse(responseBody);
    }

    if (
      attempt < maxAttempts &&
      (response.status === 429 || response.status >= 500)
    ) {
      await new Promise((resolve) => setTimeout(resolve, attempt * 1000));
      continue;
    }

    throw new Error(
      `GitHub API ${method} ${path} failed (${response.status}): ${responseBody}`,
    );
  }

  throw new Error(`GitHub API ${method} ${path} failed unexpectedly`);
}

async function assertCurrentPullRequest() {
  const pullRequest = await github(`/repos/${repository}/pulls/${pullNumber}`);
  if (
    pullRequest.state !== "open" ||
    pullRequest.head?.sha !== headSha ||
    pullRequest.base?.ref !== baseRef ||
    pullRequest.base?.sha !== baseSha
  ) {
    throw new Error(
      `Refusing stale Codex Security report mutation for head ${headSha} targeting ${baseRef}@${baseSha}; current PR state is ${pullRequest.state ?? "unknown"} with head ${pullRequest.head?.sha ?? "unknown"} targeting ${pullRequest.base?.ref ?? "unknown"}@${pullRequest.base?.sha ?? "unknown"}.`,
    );
  }
  return pullRequest;
}

async function paginate(path) {
  const items = [];
  for (let page = 1; ; page += 1) {
    const separator = path.includes("?") ? "&" : "?";
    const batch = await github(`${path}${separator}per_page=100&page=${page}`);
    items.push(...batch);
    if (batch.length < 100) {
      return items;
    }
  }
}

async function readJson(path) {
  try {
    return JSON.parse(await readFile(path, "utf8"));
  } catch (error) {
    if (error?.code === "ENOENT") {
      return null;
    }
    throw error;
  }
}

function addedLinesFromPatch(patch) {
  const added = new Set();
  if (!patch) {
    return added;
  }

  let newLine = 0;
  let inHunk = false;
  for (const line of patch.split("\n")) {
    const hunk = line.match(/^@@ -\d+(?:,\d+)? \+(\d+)(?:,\d+)? @@/);
    if (hunk) {
      newLine = Number(hunk[1]);
      inHunk = true;
      continue;
    }
    if (!inHunk) {
      continue;
    }
    if (line.startsWith("+")) {
      added.add(newLine);
      newLine += 1;
      continue;
    }
    if (line.startsWith("-")) {
      continue;
    }
    if (line.startsWith(" ") || line === "") {
      newLine += 1;
    }
  }
  return added;
}

const mergeDiffsByPath = new Map();

async function mergeDiffForPath(path) {
  if (!mergeDiffsByPath.has(path)) {
    mergeDiffsByPath.set(
      path,
      execFileAsync(
        "git",
        [
          "--literal-pathspecs",
          "-C",
          mergeRepo,
          "diff",
          "--unified=0",
          "--no-ext-diff",
          headSha,
          mergeSha,
          "--",
          path,
        ],
        { maxBuffer: 10 * 1024 * 1024 },
      ).then(({ stdout }) => stdout),
    );
  }
  return mergeDiffsByPath.get(path);
}

async function headLineForMergeLine(path, mergeLine) {
  if (!mergeSha) {
    return null;
  }
  if (headSha === mergeSha) {
    return mergeLine;
  }

  const diff = await mergeDiffForPath(path);
  let lineOffset = 0;
  for (const line of diff.split("\n")) {
    const hunk = line.match(/^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/);
    if (!hunk) {
      continue;
    }

    const headCount = Number(hunk[2] ?? "1");
    const mergeStart = Number(hunk[3]);
    const mergeCount = Number(hunk[4] ?? "1");
    if (
      mergeLine < mergeStart ||
      (mergeCount === 0 && mergeLine === mergeStart)
    ) {
      break;
    }
    if (mergeCount > 0 && mergeLine < mergeStart + mergeCount) {
      return null;
    }
    lineOffset += headCount - mergeCount;
  }

  const headLine = mergeLine + lineOffset;
  return headLine > 0 ? headLine : null;
}

async function findInlineLocation(finding, changedLines) {
  for (const location of finding.locations ?? []) {
    const lines = changedLines.get(location.path);
    if (!lines || lines.size === 0) {
      continue;
    }

    const start = location.startLine;
    const end = Math.min(location.endLine ?? start, start + 1000);
    for (let line = start; line <= end; line += 1) {
      const headLine = await headLineForMergeLine(location.path, line);
      if (headLine !== null && lines.has(headLine)) {
        return { path: location.path, line: headLine };
      }
    }
  }
  return null;
}

function findingFingerprint(finding) {
  for (const candidate of [
    finding.fingerprints?.primary,
    finding.findingId,
    finding.extensions?.candidateId,
    finding.extensions?.reportId,
  ]) {
    if (typeof candidate === "string" && candidate.trim()) {
      return candidate.trim();
    }
  }

  const fallbackInput = JSON.stringify({
    ruleId: finding.ruleId ?? null,
    title: finding.title ?? null,
    locations: finding.locations ?? [],
  });
  return `fallback_${createHash("sha256").update(fallbackInput).digest("hex").slice(0, 24)}`;
}

function findingMarker(finding) {
  return `<!-- codex-security-finding:${findingMarkerId(finding)} -->`;
}

function findingMarkerId(finding) {
  return createHash("sha256").update(findingFingerprint(finding)).digest("hex");
}

function escapeMarkdown(value) {
  return String(value ?? "")
    .replaceAll("\\", "\\\\")
    .replaceAll("`", "\\`")
    .replaceAll("*", "\\*")
    .replaceAll("_", "\\_")
    .replaceAll("[", "\\[")
    .replaceAll("]", "\\]")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll("#", "\\#")
    .replaceAll("|", "\\|")
    .replaceAll("@", "@\u200B")
    .replace(/\b([a-z][a-z0-9+.-]*):\/\//gi, "$1\u200B://")
    .replace(/\bwww\./gi, "www\u200B.")
    .replaceAll("\n", " ")
    .trim();
}

function truncate(value, maxLength) {
  const text = String(value ?? "").trim();
  return text.length <= maxLength ? text : `${text.slice(0, maxLength - 1)}…`;
}

function sourceLink(location) {
  if (
    !mergeSha ||
    typeof location?.path !== "string" ||
    !location.path ||
    !Number.isInteger(location.startLine) ||
    location.startLine < 1
  ) {
    return "—";
  }
  const encodedPath = location.path
    .split("/")
    .map((segment) =>
      encodeURIComponent(segment).replace(
        /[!'()*]/g,
        (character) => `%${character.charCodeAt(0).toString(16).toUpperCase()}`,
      ),
    )
    .join("/");
  return `[${escapeMarkdown(location.path)}:${location.startLine}](${serverUrl}/${repository}/blob/${mergeSha}/${encodedPath}#L${location.startLine})`;
}

function inlineBody(finding) {
  const severity = escapeMarkdown(finding.severity?.level ?? "unknown");
  const confidence = escapeMarkdown(finding.confidence?.level ?? "unknown");
  const title = escapeMarkdown(truncate(finding.title, 300));
  const summary = escapeMarkdown(truncate(finding.summary, 2400));
  const remediation = escapeMarkdown(truncate(finding.remediation, 1800));
  const rationale = escapeMarkdown(truncate(finding.severity?.rationale, 1200));

  return [
    `### 🛡️ ${severity.toUpperCase()}: ${title}`,
    "",
    summary,
    "",
    rationale ? `**Why ${severity}:** ${rationale}` : null,
    `**Confidence:** ${confidence}`,
    remediation ? `**Remediation:** ${remediation}` : null,
    "",
    `[Open the full Codex Security run](${runUrl})`,
    "",
    findingMarker(finding),
  ]
    .filter((line) => line !== null)
    .join("\n");
}

function stripFindingStatePrefix(body) {
  let stripped = body;
  while (true) {
    const prefix = [
      STALE_PREFIX,
      RELOCATED_PREFIX,
      SUMMARY_ONLY_PREFIX,
    ].find((candidate) => stripped.startsWith(candidate));
    if (!prefix) {
      return stripped;
    }
    stripped = stripped.slice(prefix.length);
  }
}

function stripFindingMarker(body) {
  return body.replace(/\n*<!-- codex-security-finding:[^>]+ -->\s*$/, "");
}

async function updateReviewComment(comment, body) {
  if (comment.body === body) {
    return true;
  }
  try {
    await github(`/repos/${repository}/pulls/comments/${comment.id}`, {
      method: "PATCH",
      body: { body },
    });
    return true;
  } catch {
    return false;
  }
}

function fitCommentBody(body) {
  if (Buffer.byteLength(body, "utf8") <= MAX_SUMMARY_BYTES) {
    return body;
  }

  const suffix =
    "\n\n_Comment truncated to fit GitHub's size limit. The artifact contains the complete result._";
  const availableBytes = MAX_SUMMARY_BYTES - Buffer.byteLength(suffix, "utf8");
  let truncated = Buffer.from(body, "utf8")
    .subarray(0, availableBytes)
    .toString("utf8");
  while (Buffer.byteLength(truncated, "utf8") > availableBytes) {
    truncated = truncated.slice(0, -1);
  }
  return `${truncated.trimEnd()}${suffix}`;
}

function severityCounts(findings) {
  const counts = {
    critical: 0,
    high: 0,
    medium: 0,
    low: 0,
    informational: 0,
  };
  for (const finding of findings) {
    const severity = finding.severity?.level;
    if (typeof severity === "string" && Object.hasOwn(counts, severity)) {
      counts[severity] += 1;
    }
  }
  return counts;
}

function sortFindingsBySeverity(findings) {
  const rank = { critical: 0, high: 1, medium: 2, low: 3, informational: 4 };
  const rankOf = (finding) => {
    const severity = finding.severity?.level;
    return typeof severity === "string" && Object.hasOwn(rank, severity)
      ? rank[severity]
      : 5;
  };
  return findings
    .map((finding, index) => ({ finding, index }))
    .sort(
      (left, right) =>
        rankOf(left.finding) - rankOf(right.finding) ||
        left.index - right.index,
    )
    .map(({ finding }) => finding);
}

function statusSummary(completeness) {
  if (scanExitCode === 0 && completeness === "complete") {
    return {
      icon: "✅",
      title: "Passed",
      detail:
        "Coverage is complete and no high or critical findings were reported.",
    };
  }
  if (scanExitCode === 1 && completeness === "complete") {
    return {
      icon: "❌",
      title: "Blocked",
      detail: "A complete scan reported at least one high or critical finding.",
    };
  }
  return {
    icon: "⚠️",
    title: "Incomplete",
    detail:
      "The scan had incomplete coverage or a runtime/input error. Treat this as a failed security check, not as a clean result.",
  };
}

await assertCurrentPullRequest();

const findingsDocument = await readJson(`${scanDir}/findings.json`);
const coverage = await readJson(`${scanDir}/coverage.json`);
const validFindingsDocument = Array.isArray(findingsDocument?.findings);
const findings = validFindingsDocument ? findingsDocument.findings : [];
const sortedFindings = sortFindingsBySeverity(findings);
const scanCompleteness = validFindingsDocument
  ? (coverage?.completeness ?? "unknown")
  : "unknown";
const completeness =
  artifactOutcome === "success" ? scanCompleteness : "incomplete";
const completeResult =
  completeness === "complete" && (scanExitCode === 0 || scanExitCode === 1);

const pullFiles = await paginate(
  `/repos/${repository}/pulls/${pullNumber}/files`,
);
const changedLines = new Map(
  pullFiles.map((file) => [file.filename, addedLinesFromPatch(file.patch)]),
);
const existingReviewComments = await paginate(
  `/repos/${repository}/pulls/${pullNumber}/comments`,
);
const existingByFingerprint = new Map();
const duplicateCommentsByFingerprint = new Map();
for (const comment of existingReviewComments) {
  if (comment.user?.login !== "github-actions[bot]") {
    continue;
  }
  const marker = comment.body?.match(
    /<!-- codex-security-finding:([^>]+) -->\s*$/,
  );
  if (marker) {
    const fingerprint = marker[1].trim();
    const existing = existingByFingerprint.get(fingerprint);
    if (!existing) {
      existingByFingerprint.set(fingerprint, comment);
      continue;
    }

    const duplicates = duplicateCommentsByFingerprint.get(fingerprint) ?? [];
    if (Number(comment.id) > Number(existing.id)) {
      duplicates.push(existing);
      existingByFingerprint.set(fingerprint, comment);
    } else {
      duplicates.push(comment);
    }
    duplicateCommentsByFingerprint.set(fingerprint, duplicates);
  }
}

const activeFingerprints = new Set(sortedFindings.map(findingMarkerId));
const deliveredInline = new Set();
const newInlineComments = [];
const newInlineFingerprints = new Set();
const cleanupWithoutReview = new Map();
const cleanupAfterReview = new Map();
let inlineUpdateFailures = 0;

for (const finding of sortedFindings) {
  const fingerprint = findingMarkerId(finding);
  const location = await findInlineLocation(finding, changedLines);
  const existing = existingByFingerprint.get(fingerprint);
  const duplicates = duplicateCommentsByFingerprint.get(fingerprint) ?? [];
  if (existing) {
    const desiredBody = inlineBody(finding);
    if (!location) {
      const summaryOnlyBody = `${SUMMARY_ONLY_PREFIX}${desiredBody}`;
      if (!(await updateReviewComment(existing, summaryOnlyBody))) {
        inlineUpdateFailures += 1;
      }
      for (const duplicate of duplicates) {
        cleanupWithoutReview.set(duplicate.id, duplicate);
      }
      continue;
    }

    const isCurrentLocation =
      existing.commit_id === headSha &&
      existing.path === location.path &&
      existing.line === location.line &&
      existing.side === "RIGHT";
    if (isCurrentLocation) {
      deliveredInline.add(fingerprint);
      if (!(await updateReviewComment(existing, desiredBody))) {
        inlineUpdateFailures += 1;
      }
      for (const duplicate of duplicates) {
        cleanupWithoutReview.set(duplicate.id, duplicate);
      }
      continue;
    }

    if (newInlineComments.length >= MAX_INLINE_COMMENTS) {
      const summaryOnlyBody = `${SUMMARY_ONLY_PREFIX}${desiredBody}`;
      if (!(await updateReviewComment(existing, summaryOnlyBody))) {
        inlineUpdateFailures += 1;
      }
      for (const duplicate of duplicates) {
        cleanupWithoutReview.set(duplicate.id, duplicate);
      }
      continue;
    }

    deliveredInline.add(fingerprint);
    newInlineFingerprints.add(fingerprint);
    newInlineComments.push({
      path: location.path,
      line: location.line,
      side: "RIGHT",
      body: desiredBody,
    });
    for (const comment of [existing, ...duplicates]) {
      cleanupAfterReview.set(comment.id, comment);
    }
    continue;
  }

  if (location && newInlineComments.length < MAX_INLINE_COMMENTS) {
    deliveredInline.add(fingerprint);
    newInlineFingerprints.add(fingerprint);
    newInlineComments.push({
      path: location.path,
      line: location.line,
      side: "RIGHT",
      body: inlineBody(finding),
    });
  }
}

let reviewWarning =
  inlineUpdateFailures > 0
    ? `GitHub could not refresh ${inlineUpdateFailures} existing inline comment${inlineUpdateFailures === 1 ? "" : "s"}; the sticky summary is authoritative and the next scan will retry.`
    : null;
let reviewCreated = false;
if (newInlineComments.length > 0) {
  try {
    await github(`/repos/${repository}/pulls/${pullNumber}/reviews`, {
      method: "POST",
      body: {
        commit_id: headSha,
        event: "COMMENT",
        body: `Codex Security reported ${newInlineComments.length} new finding${newInlineComments.length === 1 ? "" : "s"} on changed lines. See the sticky summary for the complete result.`,
        comments: newInlineComments,
      },
    });
    reviewCreated = true;
  } catch (error) {
    reviewWarning = `GitHub rejected the batched inline review: ${error.message}`;
    for (const fingerprint of newInlineFingerprints) {
      deliveredInline.delete(fingerprint);
    }
  }
}

const commentsToRelocate = new Map(cleanupWithoutReview);
if (reviewCreated) {
  for (const [id, comment] of cleanupAfterReview) {
    commentsToRelocate.set(id, comment);
  }
}
let cleanupFailures = 0;
for (const comment of commentsToRelocate.values()) {
  const priorBody = stripFindingStatePrefix(comment.body ?? "");
  const desiredBody = `${RELOCATED_PREFIX}${priorBody}`;
  if (comment.body === desiredBody) {
    continue;
  }
  try {
    await github(`/repos/${repository}/pulls/comments/${comment.id}`, {
      method: "PATCH",
      body: { body: desiredBody },
    });
  } catch {
    cleanupFailures += 1;
  }
}
if (cleanupFailures > 0) {
  const cleanupWarning = `GitHub could not mark ${cleanupFailures} superseded inline comment${cleanupFailures === 1 ? "" : "s"}; the next scan will retry.`;
  reviewWarning = [reviewWarning, cleanupWarning].filter(Boolean).join(" ");
}

let staleFailures = 0;
if (completeResult) {
  for (const [fingerprint, comment] of existingByFingerprint) {
    if (!activeFingerprints.has(fingerprint)) {
      const comments = [
        comment,
        ...(duplicateCommentsByFingerprint.get(fingerprint) ?? []),
      ];
      for (const staleComment of comments) {
        const activeBody = stripFindingStatePrefix(staleComment.body ?? "");
        const desiredBody = `${STALE_PREFIX}${activeBody}`;
        if (staleComment.body !== desiredBody) {
          try {
            await github(
              `/repos/${repository}/pulls/comments/${staleComment.id}`,
              {
                method: "PATCH",
                body: { body: desiredBody },
              },
            );
          } catch {
            staleFailures += 1;
          }
        }
      }
    }
  }
}
if (staleFailures > 0) {
  const staleWarning = `GitHub could not mark ${staleFailures} resolved inline comment${staleFailures === 1 ? "" : "s"}; the next scan will retry.`;
  reviewWarning = [reviewWarning, staleWarning].filter(Boolean).join(" ");
}

const counts = severityCounts(findings);
const status = statusSummary(completeness);
const summaryLines = [
  SUMMARY_MARKER,
  `## ${status.icon} Codex Security: ${status.title}`,
  "",
  status.detail,
  "",
  "| Coverage | Critical | High | Medium | Low | Informational |",
  "| --- | ---: | ---: | ---: | ---: | ---: |",
  `| ${escapeMarkdown(completeness)} | ${counts.critical} | ${counts.high} | ${counts.medium} | ${counts.low} | ${counts.informational} |`,
  "",
  `Scanned PR head \`${headSha.slice(0, 12)}\` with \`@openai/codex-security@${codexSecurityVersion}\`. [Open logs and the 7-day scan artifact](${runUrl}).`,
];

if (artifactOutcome !== "success") {
  summaryLines.push(
    "",
    "> ⚠️ The scan artifact could not be preserved. Treat this result as incomplete.",
  );
}

if (reviewWarning) {
  summaryLines.push(
    "",
    `> ⚠️ ${escapeMarkdown(truncate(reviewWarning, 1000))}`,
  );
}

if (findings.length === 0) {
  summaryLines.push(
    "",
    completeness === "complete"
      ? "No reportable findings."
      : "No sealed findings document was available. This does **not** mean the PR is clean.",
  );
} else {
  summaryLines.push(
    "",
    "### Findings",
    "",
    "| Severity | Finding | Location | Delivery |",
    "| --- | --- | --- | --- |",
  );

  let shownFindings = 0;
  for (const finding of sortedFindings) {
    if (shownFindings >= MAX_SUMMARY_FINDINGS) {
      break;
    }
    const fingerprint = findingMarkerId(finding);
    const location = finding.locations?.[0];
    const row = `| ${escapeMarkdown(finding.severity?.level ?? "unknown")} | **${escapeMarkdown(truncate(finding.title, 240))}** — ${escapeMarkdown(truncate(finding.summary, 360))} | ${sourceLink(location)} | ${deliveredInline.has(fingerprint) ? "inline comment" : "summary only"} |`;
    if (
      Buffer.byteLength([...summaryLines, row].join("\n"), "utf8") >
      MAX_SUMMARY_FINDING_BYTES
    ) {
      break;
    }
    summaryLines.push(row);
    shownFindings += 1;
  }

  if (findings.length > shownFindings) {
    summaryLines.push(
      "",
      `_Only the first ${shownFindings} findings are shown here. The artifact contains all ${findings.length}._`,
    );
  }
}

const deferred = Array.isArray(coverage?.deferred) ? coverage.deferred : [];
const openQuestions = Array.isArray(coverage?.openQuestions)
  ? coverage.openQuestions
  : [];
if (deferred.length > 0 || openQuestions.length > 0) {
  summaryLines.push("", "### Coverage gaps", "");
  for (const item of deferred.slice(0, 10)) {
    summaryLines.push(
      `- **Deferred:** ${escapeMarkdown(truncate(item.reason, 500))}`,
    );
  }
  for (const item of openQuestions.slice(0, 10)) {
    summaryLines.push(
      `- **Open question:** ${escapeMarkdown(truncate(item.question, 500))}`,
    );
  }
}

if (!completeResult && existingByFingerprint.size > 0) {
  summaryLines.push(
    "",
    "_Previous inline findings were not marked resolved because this scan was not complete._",
  );
}

const summaryBody = fitCommentBody(summaryLines.join("\n"));
const issueComments = await paginate(
  `/repos/${repository}/issues/${pullNumber}/comments`,
);
let existingSummaries = issueComments.filter(
  (comment) =>
    comment.user?.login === "github-actions[bot]" &&
    comment.body?.startsWith(`${SUMMARY_MARKER}\n`),
);

if (existingSummaries.length === 0) {
  const createdSummary = await github(
    `/repos/${repository}/issues/${pullNumber}/comments`,
    {
      method: "POST",
      body: { body: summaryBody },
    },
  );
  const refreshedSummaries = (
    await paginate(`/repos/${repository}/issues/${pullNumber}/comments`)
  ).filter(
    (comment) =>
      comment.user?.login === "github-actions[bot]" &&
      comment.body?.startsWith(`${SUMMARY_MARKER}\n`),
  );
  existingSummaries = [
    createdSummary,
    ...refreshedSummaries.filter((comment) => comment.id !== createdSummary.id),
  ];
}

existingSummaries.sort((left, right) => Number(left.id) - Number(right.id));
const [existingSummary, ...duplicateSummaries] = existingSummaries;
if (!existingSummary) {
  throw new Error("GitHub did not return the Codex Security summary comment");
}
if (existingSummary.body !== summaryBody) {
  await github(`/repos/${repository}/issues/comments/${existingSummary.id}`, {
    method: "PATCH",
    body: { body: summaryBody },
  });
}
for (const duplicateSummary of duplicateSummaries) {
  await github(`/repos/${repository}/issues/comments/${duplicateSummary.id}`, {
    method: "PATCH",
    body: { body: "<!-- codex-security-summary-retired -->" },
  });
}

await appendFile(
  githubOutput,
  `policy_state=${status.title === "Passed" ? "success" : "failure"}\npolicy_description=${status.detail}\n`,
  "utf8",
);

process.stdout.write(
  `Codex Security report updated: findings=${findings.length} coverage=${completeness} inline=${deliveredInline.size}\n`,
);
