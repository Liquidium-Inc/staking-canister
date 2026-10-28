import { execFile } from "node:child_process";
import { promisify } from "node:util";

import { config } from "./config.mjs";

const execFileAsync = promisify(execFile);
const mergeDiffsByPath = new Map();

export function addedLinesFromPatch(patch) {
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

async function mergeDiffForPath(path) {
  if (!mergeDiffsByPath.has(path)) {
    mergeDiffsByPath.set(
      path,
      execFileAsync(
        "git",
        [
          "--literal-pathspecs",
          "-C",
          config.mergeRepo,
          "diff",
          "--unified=0",
          "--no-ext-diff",
          config.headSha,
          config.mergeSha,
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
  if (!config.mergeSha) {
    return null;
  }
  if (config.headSha === config.mergeSha) {
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

export async function findInlineLocation(finding, changedLines) {
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
