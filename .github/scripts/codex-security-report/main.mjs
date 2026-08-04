import { appendFile, readFile } from "node:fs/promises";

import { config, limits } from "./config.mjs";
import { addedLinesFromPatch, findInlineLocation } from "./diff-lines.mjs";
import {
  FINDING_MARKER_PATTERN,
  RELOCATED_PREFIX,
  STALE_PREFIX,
  SUMMARY_ONLY_PREFIX,
  findingMarkerId,
  severityCounts,
  sortFindingsBySeverity,
  stripFindingStatePrefix,
} from "./finding-identity.mjs";
import {
  escapeMarkdown,
  fitCommentBody,
  inlineBody,
  sourceLink,
  statusSummary,
  truncate,
} from "./render.mjs";
import {
  assertCurrentPullRequest,
  github,
  paginate,
} from "./github-api.mjs";

const SUMMARY_MARKER = "<!-- codex-security-summary -->";

async function readJson(path) {
  try {
    return JSON.parse(await readFile(path, "utf8"));
  } catch (error) {
    if (error?.code === "ENOENT" || error instanceof SyntaxError) {
      return null;
    }
    throw error;
  }
}

async function updateReviewComment(comment, body) {
  if (comment.body === body) {
    return true;
  }
  try {
    await github(
      `/repos/${config.repository}/pulls/comments/${comment.id}`,
      { method: "PATCH", body: { body } },
    );
    return true;
  } catch {
    return false;
  }
}

// 1. Read and classify the sealed scanner output.
await assertCurrentPullRequest();

const findingsDocument = await readJson(`${config.scanDir}/findings.json`);
const coverage = await readJson(`${config.scanDir}/coverage.json`);
const validFindingsDocument = Array.isArray(findingsDocument?.findings);
const findings = validFindingsDocument ? findingsDocument.findings : [];
const sortedFindings = sortFindingsBySeverity(findings);
const scanCompleteness = validFindingsDocument
  ? (coverage?.completeness ?? "unknown")
  : "unknown";
const completeness =
  config.artifactOutcome === "success" && config.scanOutcome === "success"
    ? scanCompleteness
    : "incomplete";
const completeResult =
  completeness === "complete" &&
  (config.scanExitCode === 0 || config.scanExitCode === 1);

// 2. Match findings to changed PR lines and existing bot comments.
const pullFiles = await paginate(
  `/repos/${config.repository}/pulls/${config.pullNumber}/files`,
);
const changedLines = new Map(
  pullFiles.map((file) => [file.filename, addedLinesFromPatch(file.patch)]),
);
const existingReviewComments = await paginate(
  `/repos/${config.repository}/pulls/${config.pullNumber}/comments`,
);
const existingByFingerprint = new Map();
const duplicateCommentsByFingerprint = new Map();

for (const comment of existingReviewComments) {
  if (comment.user?.login !== "github-actions[bot]") {
    continue;
  }
  const marker = comment.body?.match(FINDING_MARKER_PATTERN);
  if (!marker) {
    continue;
  }

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

// 3. Update current comments and prepare one batched review for new anchors.
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
      if (
        !(await updateReviewComment(
          existing,
          `${SUMMARY_ONLY_PREFIX}${desiredBody}`,
        ))
      ) {
        inlineUpdateFailures += 1;
      }
      for (const duplicate of duplicates) {
        cleanupWithoutReview.set(duplicate.id, duplicate);
      }
      continue;
    }

    const isCurrentLocation =
      existing.commit_id === config.headSha &&
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

    if (newInlineComments.length >= limits.maxInlineComments) {
      if (
        !(await updateReviewComment(
          existing,
          `${SUMMARY_ONLY_PREFIX}${desiredBody}`,
        ))
      ) {
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

  if (location && newInlineComments.length < limits.maxInlineComments) {
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
    await github(
      `/repos/${config.repository}/pulls/${config.pullNumber}/reviews`,
      {
        method: "POST",
        body: {
          commit_id: config.headSha,
          event: "COMMENT",
          body: `Codex Security reported ${newInlineComments.length} new finding${newInlineComments.length === 1 ? "" : "s"} on changed lines. See the sticky summary for the complete result.`,
          comments: newInlineComments,
        },
      },
    );
    reviewCreated = true;
  } catch (error) {
    reviewWarning = `GitHub rejected the batched inline review: ${error.message}`;
    for (const fingerprint of newInlineFingerprints) {
      deliveredInline.delete(fingerprint);
    }
  }
}

// 4. Retire duplicate, relocated, and stale inline comments.
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
    await github(
      `/repos/${config.repository}/pulls/comments/${comment.id}`,
      { method: "PATCH", body: { body: desiredBody } },
    );
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
    if (activeFingerprints.has(fingerprint)) {
      continue;
    }
    const comments = [
      comment,
      ...(duplicateCommentsByFingerprint.get(fingerprint) ?? []),
    ];
    for (const staleComment of comments) {
      const activeBody = stripFindingStatePrefix(staleComment.body ?? "");
      const desiredBody = `${STALE_PREFIX}${activeBody}`;
      if (staleComment.body === desiredBody) {
        continue;
      }
      try {
        await github(
          `/repos/${config.repository}/pulls/comments/${staleComment.id}`,
          { method: "PATCH", body: { body: desiredBody } },
        );
      } catch {
        staleFailures += 1;
      }
    }
  }
}
if (staleFailures > 0) {
  const staleWarning = `GitHub could not mark ${staleFailures} resolved inline comment${staleFailures === 1 ? "" : "s"}; the next scan will retry.`;
  reviewWarning = [reviewWarning, staleWarning].filter(Boolean).join(" ");
}

// 5. Build the complete, size-bounded sticky summary.
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
  `Scanned PR head \`${config.headSha.slice(0, 12)}\` with \`@openai/codex-security@${config.codexSecurityVersion}\`. [Open logs and the 7-day scan artifact](${config.runUrl}).`,
];

if (config.artifactOutcome !== "success" || config.scanOutcome !== "success") {
  summaryLines.push(
    "",
    "> ⚠️ The scan step or artifact preservation did not complete successfully. Treat this result as incomplete.",
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
    if (shownFindings >= limits.maxSummaryFindings) {
      break;
    }
    const fingerprint = findingMarkerId(finding);
    const location = finding.locations?.[0];
    const row = `| ${escapeMarkdown(finding.severity?.level ?? "unknown")} | **${escapeMarkdown(truncate(finding.title, 240))}** — ${escapeMarkdown(truncate(finding.summary, 360))} | ${sourceLink(location)} | ${deliveredInline.has(fingerprint) ? "inline comment" : "summary only"} |`;
    if (
      Buffer.byteLength([...summaryLines, row].join("\n"), "utf8") >
      limits.maxSummaryFindingBytes
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

// 6. Upsert one sticky summary and expose the policy result to the workflow.
const summaryBody = fitCommentBody(summaryLines.join("\n"));
const issueComments = await paginate(
  `/repos/${config.repository}/issues/${config.pullNumber}/comments`,
);
let existingSummaries = issueComments.filter(
  (comment) =>
    comment.user?.login === "github-actions[bot]" &&
    comment.body?.startsWith(`${SUMMARY_MARKER}\n`),
);

if (existingSummaries.length === 0) {
  const createdSummary = await github(
    `/repos/${config.repository}/issues/${config.pullNumber}/comments`,
    { method: "POST", body: { body: summaryBody } },
  );
  const refreshedSummaries = (
    await paginate(
      `/repos/${config.repository}/issues/${config.pullNumber}/comments`,
    )
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
  await github(
    `/repos/${config.repository}/issues/comments/${existingSummary.id}`,
    { method: "PATCH", body: { body: summaryBody } },
  );
}
for (const duplicateSummary of duplicateSummaries) {
  await github(
    `/repos/${config.repository}/issues/comments/${duplicateSummary.id}`,
    {
      method: "PATCH",
      body: { body: "<!-- codex-security-summary-retired -->" },
    },
  );
}

await appendFile(
  config.githubOutput,
  `policy_state=${status.title === "Passed" ? "success" : "failure"}\npolicy_description=${status.detail}\n`,
  "utf8",
);

process.stdout.write(
  `Codex Security report updated: findings=${findings.length} coverage=${completeness} inline=${deliveredInline.size}\n`,
);
