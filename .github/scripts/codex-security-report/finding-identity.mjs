import { createHash } from "node:crypto";

export const FINDING_MARKER_PATTERN =
  /<!-- codex-security-finding:([^>]+) -->\s*$/;
export const STALE_PREFIX =
  "> ⚪ **No longer reported by the latest complete Codex Security scan.**\n\n";
export const RELOCATED_PREFIX =
  "> 🔄 **Superseded by the current Codex Security report for this active finding.**\n\n";
export const SUMMARY_ONLY_PREFIX =
  "> 🔄 **Still active, but now reported in the sticky summary because it cannot be anchored to a changed line in this PR revision.**\n\n";

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

export function findingMarkerId(finding) {
  return createHash("sha256").update(findingFingerprint(finding)).digest("hex");
}

export function findingMarker(finding) {
  return `<!-- codex-security-finding:${findingMarkerId(finding)} -->`;
}

export function stripFindingStatePrefix(body) {
  let stripped = body;
  while (true) {
    const prefix = [STALE_PREFIX, RELOCATED_PREFIX, SUMMARY_ONLY_PREFIX].find(
      (candidate) => stripped.startsWith(candidate),
    );
    if (!prefix) {
      return stripped;
    }
    stripped = stripped.slice(prefix.length);
  }
}

export function severityCounts(findings) {
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

export function sortFindingsBySeverity(findings) {
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
