import { createHmac } from "node:crypto";
import { readFile } from "node:fs/promises";
import { type Client, type SeedIssue, request } from "./api";
import { dateAt } from "./data";
import { webhookKeyPath } from "./settings";

async function key() {
  try {
    return await readFile(webhookKeyPath, "utf8");
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return null;
    throw error;
  }
}

export async function demoGitHub(admin: Client, local: boolean) {
  // An existing externally configured integration always retains control.
  if (!local || process.env.GITHUB_ORGANISATION_ID || process.env.GITHUB_WEBHOOK_SECRET)
    return null;
  const secret = await key();
  if (!secret) return null;

  return async (project: string, issues: SeedIssue[], index: number, captain: string) => {
    const repository = `fallow-reach-demo/${project.toLowerCase()}`;
    const path = `/projects/${project}`;
    const { items, version } = await admin.get<{
      items: { repository: string }[];
      version: number;
    }>(`${path}/repositories`);
    const repositories = items.map((item) => item.repository);
    if (!repositories.includes(repository)) {
      await admin.write("PUT", `${path}/repositories`, {
        version,
        repositories: [...repositories, repository],
      });
    }
    const now = dateAt(-5 + index);
    async function event(type: string, data: object) {
      const document = { repository: { full_name: repository }, ...data };
      const signature = createHmac("sha256", secret!)
        .update(JSON.stringify(document))
        .digest("hex");
      await request("POST", "/integrations/github/webhook", {
        body: document,
        headers: { "X-GitHub-Event": type, "X-Hub-Signature-256": `sha256=${signature}` },
      });
    }
    const issue = issues[0]!;
    const pullNumber = 42;
    const linked = await admin.all<{ repository: { fullName: string }; number: number }>(
      `/issues/${issue.id}/pull-requests`,
    );
    if (
      !linked.some((row) => row.repository.fullName === repository && row.number === pullNumber)
    ) {
      await event("pull_request", {
        pull_request: {
          number: pullNumber,
          title: `Pilot: ${issue.title}`,
          body: `Refs ${issue.identifier}`,
          user: { login: captain },
          head: { ref: "pilot/rehearsal" },
          base: { ref: "main" },
          state: "open",
          draft: index % 2 === 0,
          merged_at: null,
          closed_at: null,
          updated_at: now,
        },
      });
    }
    const deployId = 100 + index;
    const old = await admin.all<{ repository: string; statusId: number }>(
      `${path}/deployment-statuses`,
    );
    if (!old.some((row) => row.repository === repository && row.statusId === deployId)) {
      await event("deployment_status", {
        deployment: {
          id: deployId,
          sha: String(index + 1).repeat(40),
          environment: "pilot-simulator",
        },
        deployment_status: {
          id: deployId,
          state: index === 3 ? "in_progress" : "success",
          created_at: now,
        },
      });
    }
  };
}
