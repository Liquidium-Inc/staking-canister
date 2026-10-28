import { config, limits } from "./config.mjs";
import { findingMarker } from "./finding-identity.mjs";

// Scanner output is untrusted. Every scanner-controlled value must pass through
// this function before it is included in a GitHub comment.
export function escapeMarkdown(value) {
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

export function truncate(value, maxLength) {
  const text = String(value ?? "").trim();
  return text.length <= maxLength ? text : `${text.slice(0, maxLength - 1)}…`;
}

export function sourceLink(location) {
  if (
    !config.mergeSha ||
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
        (character) =>
          `%${character.charCodeAt(0).toString(16).toUpperCase()}`,
      ),
    )
    .join("/");
  return `[${escapeMarkdown(location.path)}:${location.startLine}](${config.serverUrl}/${config.repository}/blob/${config.mergeSha}/${encodedPath}#L${location.startLine})`;
}

export function inlineBody(finding) {
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
    `[Open the full Codex Security run](${config.runUrl})`,
    "",
    findingMarker(finding),
  ]
    .filter((line) => line !== null)
    .join("\n");
}

export function fitCommentBody(body) {
  if (Buffer.byteLength(body, "utf8") <= limits.maxSummaryBytes) {
    return body;
  }

  const suffix =
    "\n\n_Comment truncated to fit GitHub's size limit. The artifact contains the complete result._";
  const availableBytes =
    limits.maxSummaryBytes - Buffer.byteLength(suffix, "utf8");
  let truncated = Buffer.from(body, "utf8")
    .subarray(0, availableBytes)
    .toString("utf8");
  while (Buffer.byteLength(truncated, "utf8") > availableBytes) {
    truncated = truncated.slice(0, -1);
  }
  return `${truncated.trimEnd()}${suffix}`;
}

export function statusSummary(completeness) {
  if (config.scanExitCode === 0 && completeness === "complete") {
    return {
      icon: "✅",
      title: "Passed",
      detail:
        "Coverage is complete and no high or critical findings were reported.",
    };
  }
  if (config.scanExitCode === 1 && completeness === "complete") {
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
