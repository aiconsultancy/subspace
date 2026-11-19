import { describe, expect, test } from "bun:test";
import { generateKeyPairSync } from "node:crypto";
import { jwtVerify } from "jose";
import { extractLinks } from "../src/github";
import { createGitHubClient, githubConfig } from "../src/github-app";

describe("GitHub App integration", () => {
  test("exchanges an RS256 app JWT and caches installation tokens across pages", async () => {
    const pair = generateKeyPairSync("rsa", { modulusLength: 2048 });
    const key = pair.privateKey.export({ format: "pem", type: "pkcs1" }).toString();
    const settings = githubConfig({
      GITHUB_ORGANISATION_ID: crypto.randomUUID(),
      GITHUB_APP_ID: "12",
      GITHUB_APP_INSTALLATION_ID: "34",
      GITHUB_APP_PRIVATE_KEY: key,
    });
    const calls: string[] = [];
    const fakeFetch = (async (url: URL, options: RequestInit) => {
      expect(url.origin).toBe("https://api.github.com");
      expect(options.redirect).toBe("error");
      const auth = new Headers(options.headers).get("authorization")!;
      calls.push(url.pathname);
      if (url.pathname.startsWith("/app/")) {
        expect(url.pathname).toBe("/app/installations/34/access_tokens");
        expect(options.method).toBe("POST");
        const { payload, protectedHeader } = await jwtVerify(auth.slice(7), pair.publicKey, {
          issuer: "12",
          algorithms: ["RS256"],
        });
        expect(protectedHeader.alg).toBe("RS256");
        expect(payload.exp! - payload.iat!).toBe(600);
        return Response.json({
          token: "installation-fixture",
          expires_at: new Date(Date.now() + 3600_000).toISOString(),
        });
      }
      expect(auth).toBe("Bearer installation-fixture");
      if (url.pathname === "/installation/repositories") {
        const first = url.searchParams.get("page") === "1";
        return Response.json(
          { repositories: [{ full_name: first ? "Example/Project" : "Example/Second" }] },
          { headers: first ? { Link: '<https://api.github.com/example>; rel="next"' } : {} },
        );
      }
      expect(url.pathname).toBe("/repos/example/project/pulls");
      expect(url.searchParams.get("sort")).toBe("updated");
      return Response.json([{ number: Number(url.searchParams.get("page")) }], {
        headers:
          url.searchParams.get("page") === "1"
            ? { Link: '<https://api.github.com/example>; rel="next"' }
            : {},
      });
    }) as typeof fetch;
    const client = createGitHubClient(settings.app!, fakeFetch);
    const signal = AbortSignal.timeout(1000);
    expect(await client.pulls("example/project", 1, signal)).toEqual({
      rows: [{ number: 1 }],
      next: true,
    });
    expect(await client.pulls("example/project", 2, signal)).toEqual({
      rows: [{ number: 2 }],
      next: false,
    });
    expect(
      await client.installedRepositories(["example/project", "example/second"], signal),
    ).toBeTrue();
    expect(await client.installedRepositories(["example/private"], signal)).toBeFalse();
    expect(calls).toEqual([
      "/app/installations/34/access_tokens",
      "/repos/example/project/pulls",
      "/repos/example/project/pulls",
      "/installation/repositories",
      "/installation/repositories",
      "/installation/repositories",
      "/installation/repositories",
    ]);
  });

  test("all integration credentials require an internal organisation binding", () => {
    expect(() => githubConfig({ GITHUB_WEBHOOK_SECRET: "fixture" })).toThrow(
      "GITHUB_ORGANISATION_ID",
    );
    expect(() =>
      githubConfig({ GITHUB_APP_ID: "12", GITHUB_ORGANISATION_ID: crypto.randomUUID() }),
    ).toThrow("exactly one");
    expect(githubConfig({}).app).toBeNull();
  });

  test("closing references ignore code and autolinks while branch mentions never close", () => {
    const links = extractLinks({
      title: "Resolve: PAR-15",
      body: "Fixes `PAR-16`\n\n```\nCloses PAR-17\n```\n\nhttps://example.com/PAR-18\n\nSee [PAR-20](https://example.com)",
      head: { ref: "fix/PAR-19-branch" },
    });
    expect([...links.keys()]).toEqual(["PAR-15", "PAR-20", "PAR-19"]);
    expect(links.get("PAR-15")).toMatchObject({ kind: "closes", sources: new Set(["title"]) });
    expect(links.get("PAR-19")).toMatchObject({ kind: "mentions", sources: new Set(["branch"]) });
  });
});
