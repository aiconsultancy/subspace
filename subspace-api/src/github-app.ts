import { createPrivateKey } from "node:crypto";
import { readFileSync } from "node:fs";
import { SignJWT } from "jose";
import { isUuid } from "./validation";

export function githubConfig(env: NodeJS.ProcessEnv) {
  const appId = env.GITHUB_APP_ID?.trim();
  const installationId = env.GITHUB_APP_INSTALLATION_ID?.trim();
  const pem = env.GITHUB_APP_PRIVATE_KEY?.trim();
  const file = env.GITHUB_APP_PRIVATE_KEY_FILE?.trim();
  const webhookSecret = env.GITHUB_WEBHOOK_SECRET ?? "";
  const organisationId = env.GITHUB_ORGANISATION_ID?.trim() || undefined;
  if (organisationId && !isUuid(organisationId)) {
    throw new Error("GITHUB_ORGANISATION_ID must be an organisation UUID");
  }
  if ((appId || installationId || pem || file || webhookSecret) && !organisationId) {
    throw new Error("GITHUB_ORGANISATION_ID is required for GitHub integration");
  }
  const intervalSeconds = Number(env.GITHUB_POLL_INTERVAL_SECONDS || "120");
  if (!Number.isInteger(intervalSeconds) || intervalSeconds < 10 || intervalSeconds > 86400) {
    throw new Error("GITHUB_POLL_INTERVAL_SECONDS must be between 10 and 86400");
  }
  const common = { organisationId, webhookSecret, intervalSeconds };
  if (!appId && !installationId && !pem && !file) return { ...common, app: null };
  if (
    !appId ||
    !/^[1-9]\d*$/u.test(appId) ||
    !installationId ||
    !/^[1-9]\d*$/u.test(installationId) ||
    Boolean(pem) === Boolean(file)
  ) {
    throw new Error("Set the GitHub App ID, installation ID and exactly one private key source");
  }
  try {
    const key = createPrivateKey(pem ?? readFileSync(file!, "utf8"));
    if (key.asymmetricKeyType !== "rsa" || (key.asymmetricKeyDetails?.modulusLength ?? 0) < 2048) {
      throw new Error("Invalid key type");
    }
    return { ...common, app: { appId, installationId, key } };
  } catch {
    throw new Error("The GitHub App private key must be a readable RSA PEM of at least 2048 bits");
  }
}

type App = NonNullable<ReturnType<typeof githubConfig>["app"]>;

export function createGitHubClient(app: App, request: typeof fetch = fetch) {
  let cached: { token: string; expires: number } | undefined;

  async function api(path: string, token: string, signal: AbortSignal, method = "GET") {
    const response = await request(new URL(path, "https://api.github.com"), {
      headers: {
        Accept: "application/vnd.github+json",
        Authorization: `Bearer ${token}`,
        "User-Agent": "Subspace API",
        "X-GitHub-Api-Version": "2026-03-10",
      },
      method,
      redirect: "error",
      signal: AbortSignal.any([signal, AbortSignal.timeout(15_000)]),
    });
    if (!response.ok) throw new Error(`githubHttp${response.status}`);
    return response;
  }

  async function installationToken(signal: AbortSignal) {
    if (cached && cached.expires > Date.now() + 60_000) return cached.token;
    const now = Math.floor(Date.now() / 1000);
    const jwt = await new SignJWT({})
      .setProtectedHeader({ alg: "RS256", typ: "JWT" })
      .setIssuer(app.appId)
      .setIssuedAt(now - 60)
      .setExpirationTime(now + 9 * 60)
      .sign(app.key);
    const response = await api(
      `/app/installations/${app.installationId}/access_tokens`,
      jwt,
      signal,
      "POST",
    );
    const body: { token?: string; expires_at?: string } = await response.json();
    const expires = Date.parse(body.expires_at ?? "");
    if (
      typeof body.token !== "string" ||
      !body.token ||
      !Number.isFinite(expires) ||
      expires <= Date.now()
    ) {
      throw new Error("invalidInstallationToken");
    }
    cached = { token: body.token, expires };
    return cached.token;
  }

  return {
    async installedRepositories(repositories: readonly string[], signal: AbortSignal) {
      const missing = new Set(repositories);
      const token = await installationToken(signal);
      for (let page = 1; missing.size && page <= 100; page++) {
        const response = await api(
          `/installation/repositories?per_page=100&page=${page}`,
          token,
          signal,
        );
        const body: { repositories?: unknown } = await response.json();
        if (!Array.isArray(body.repositories)) throw new Error("invalidRepositoryPage");
        for (const row of body.repositories) {
          if (typeof row?.full_name !== "string") throw new Error("invalidInstallationRepository");
          missing.delete(row.full_name.toLowerCase());
        }
        if (!/;\s*rel="next"/u.test(response.headers.get("link") ?? "")) break;
        if (missing.size && page === 100) throw new Error("installationRepositoryPaginationLimit");
      }
      return missing.size === 0;
    },
    async pulls(repository: string, page: number, signal: AbortSignal) {
      const token = await installationToken(signal);
      const response = await api(
        `/repos/${repository}/pulls?state=all&sort=updated&direction=desc&per_page=100&page=${page}`,
        token,
        signal,
      );
      const rows: unknown = await response.json();
      if (!Array.isArray(rows)) throw new Error("invalidPullPage");
      const next = /;\s*rel="next"/u.test(response.headers.get("link") ?? "");
      return { rows: rows as unknown[], next };
    },
  };
}
