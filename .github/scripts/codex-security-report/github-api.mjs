import { config, limits } from "./config.mjs";

// All writes pass through this client. Before every mutation it confirms that
// the pull request still points at the exact revision that was scanned.
export async function github(path, options = {}) {
  const method = options.method ?? "GET";
  const maxAttempts =
    method === "POST" && options.retry !== true
      ? 1
      : limits.githubMaxAttempts;

  for (let attempt = 1; attempt <= maxAttempts; attempt += 1) {
    if (method !== "GET" && options.revalidate !== false) {
      await assertCurrentPullRequest();
    }

    const controller = new AbortController();
    const timeout = setTimeout(
      () => controller.abort(),
      limits.githubRequestTimeoutMs,
    );
    let response;
    let responseBody;
    try {
      response = await fetch(`${config.apiUrl}${path}`, {
        method,
        headers: {
          Accept: "application/vnd.github+json",
          Authorization: `Bearer ${config.token}`,
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

export async function assertCurrentPullRequest() {
  const pullRequest = await github(
    `/repos/${config.repository}/pulls/${config.pullNumber}`,
  );
  if (
    pullRequest.state !== "open" ||
    pullRequest.head?.sha !== config.headSha ||
    pullRequest.base?.ref !== config.baseRef ||
    pullRequest.base?.sha !== config.baseSha
  ) {
    throw new Error(
      `Refusing stale Codex Security report mutation for head ${config.headSha} targeting ${config.baseRef}@${config.baseSha}; current PR state is ${pullRequest.state ?? "unknown"} with head ${pullRequest.head?.sha ?? "unknown"} targeting ${pullRequest.base?.ref ?? "unknown"}@${pullRequest.base?.sha ?? "unknown"}.`,
    );
  }
  return pullRequest;
}

export async function paginate(path) {
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
