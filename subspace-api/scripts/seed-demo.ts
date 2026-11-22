// The root stack calls this once for the workspace and again, on demand, for
// the fictional Fallow Reach rehearsal. Standalone API users can run seed:demo.
import { client, type Client, type Identified, type SeedIssue, type Versioned } from "./demo/api";
import { conversations } from "./demo/conversations";
import { dateAt, decisions, evidence, initiatives, labels, people, projects } from "./demo/data";
import { demoGitHub } from "./demo/github";
import { seedHistory, type SeedHistory } from "./demo/history";
import { chatScreenshot, image, issueFile } from "./demo/media";
import { defaults } from "./demo/settings";

const identity = {
  email: process.env.SEED_EMAIL ?? defaults.email,
  name: process.env.SEED_NAME ?? defaults.name,
  organisation: process.env.SEED_ORGANISATION ?? defaults.organisation,
};
const rootStack = process.env.SEED_ROOT_STACK === "1";
const workspaceOnly = process.argv.includes("--workspace-only");
const includeChat = process.argv.includes("--with-irc");
const ircHost = process.env.SEED_IRC_HOST ?? "127.0.0.1";

function port(name: string, fallback: number) {
  const value = Number(process.env[name] ?? fallback);
  if (!Number.isInteger(value) || value < 1 || value > 65535) throw new Error(`${name} is invalid`);
  return value;
}

// An agent's name is its IRC nick and account, <fleet>-<captain-id> or
// <fleet>-admiral. A captain's ship is named after it, which gives its ship
// channel #<fleet>-<captain-id> (the standard fleet channels).
const fleets = [
  {
    slug: "aurora",
    name: "Aurora fleet",
    ircHost,
    ircPort: port("SEED_IRC_PLAIN_PORT", 6667),
    ircTls: false,
    agents: [
      { handle: "aurora-admiral", name: "aurora-admiral", rank: "rearAdmiral", ship: null },
      { handle: "aurora-lyra", name: "aurora-lyra", rank: "captain", ship: "Lyra" },
      { handle: "aurora-orion", name: "aurora-orion", rank: "captain", ship: "Orion" },
      { handle: "aurora-vega", name: "aurora-vega", rank: "captain", ship: "Vega" },
      { handle: "aurora-castor", name: "aurora-castor", rank: "captain", ship: "Castor" },
    ],
  },
  {
    slug: "borealis",
    name: "Borealis fleet",
    ircHost,
    ircPort: port("SEED_IRC_TLS_PORT", 6697),
    ircTls: true,
    agents: [
      { handle: "borealis-admiral", name: "borealis-admiral", rank: "rearAdmiral", ship: null },
      { handle: "borealis-rigel", name: "borealis-rigel", rank: "captain", ship: "Rigel" },
    ],
  },
] as const;

type Actor = { userId: string; api: Client };
type Fleet = Identified & { irc: { host: string; port: number; tls: boolean } };
type Initiative = Identified & {
  name: string;
  fleetId: string | null;
  stakeholders: { name: string }[];
};
type Project = Versioned & { initiativeId: string | null; estimateScale: string };
type Comment = Identified & { body: string; actualAuthor: { userId: string } };
type Todo = Versioned & { text: string; done: boolean };

const issueDate = (index: number, number: number) => dateAt(-24 + ((index * 2 + number) % 12), 9);
const day = (offset: number) => dateAt(offset).slice(0, 10);

async function signIn(email: string, name: string): Promise<Actor> {
  const { accessToken } = await client().write<{ accessToken: string }>("POST", "/auth/dev/token", {
    ...identity,
    email,
    name,
  });
  const api = client(accessToken);
  const { userId } = await api.get<{ userId: string }>("/me");
  return { userId, api };
}

async function seedFleets(admin: Client) {
  const ids = new Map<string, string>();
  for (const { slug, agents: _agents, ...fields } of fleets) {
    let fleet = await admin.find<Fleet>(`/fleets/${slug}`);
    if (!fleet) {
      fleet = await admin.write<Fleet>("POST", "/fleets", { slug, ...fields });
    } else if (
      fleet.irc.host !== fields.ircHost ||
      fleet.irc.port !== fields.ircPort ||
      fleet.irc.tls !== fields.ircTls
    ) {
      fleet = await admin.write<Fleet>("PATCH", `/fleets/${slug}`, {
        ircHost: fields.ircHost,
        ircPort: fields.ircPort,
        ircTls: fields.ircTls,
      });
    }
    ids.set(slug, fleet.id);
  }
  return ids;
}

async function seedActors(developer: Actor) {
  const actors = new Map([["developer", developer]]);
  const { api } = developer;
  const existing = await api.all<{ serviceTokenClientId: string }>("/agents");
  for (const person of people) {
    actors.set(person.handle, await signIn(`${person.handle}@example.com`, person.name));
  }
  for (const fleet of fleets) {
    for (const { handle, ...agent } of fleet.agents) {
      const email = `${handle}@example.com`;
      if (!existing.some((row) => row.serviceTokenClientId === email)) {
        await api.write("POST", "/agents", {
          ...agent,
          fleet: fleet.slug,
          defaultModel: "rehearsal-model",
          serviceTokenClientId: email,
        });
      }
      actors.set(handle, await signIn(email, agent.name));
    }
  }
  return actors;
}

async function seedInitiatives(
  developer: Actor,
  actors: Map<string, Actor>,
  fleetIds: Map<string, string>,
) {
  const { api } = developer;
  const existing = await api.all<Initiative>("/initiatives?includeArchived=true");
  const result = [];
  for (const spec of initiatives) {
    let row = existing.find(({ name }) => name === spec.name);
    if (!row) {
      row = await api.write<Initiative>("POST", "/initiatives", {
        name: spec.name,
        description: spec.description,
        ownerId: actors.get(spec.stakeholders[0]!)!.userId,
        status: "active",
        targetDate: day(14),
        channelConfiguration: {
          channels: spec.channels,
          memberUserIds: [...actors.values()].map(({ userId }) => userId),
        },
      });
    }
    const fleetId = fleetIds.get(spec.fleet)!;
    if (row.fleetId !== fleetId) {
      await api.write("PUT", `/initiatives/${row.id}/fleet`, { fleetId });
    }
    for (const person of people.filter(({ handle }) => spec.stakeholders.includes(handle))) {
      if (!row.stakeholders.some(({ name }) => name === person.name)) {
        await api.write("POST", `/initiatives/${row.id}/stakeholders`, {
          name: person.name,
          role: person.role,
          slackHandle: `@${person.handle}`,
        });
      }
    }
    result.push(row.id);
  }
  return result;
}

function description(spec: (typeof projects)[number], summary: string, number: number) {
  const fixture = evidence[spec.key];
  if (number !== 1) {
    return `## Pilot scope\n\n${summary}\n\n[Rehearsal notes](https://example.com/fallow-reach/${spec.key.toLowerCase()})`;
  }
  return [
    "## Fallow Reach rehearsal",
    summary,
    fixture.report,
    "| Pass | Input | Evidence |\n| --- | --- | --- |\n| Baseline | Synthetic replay | Operator trace |\n| Pilot | Two repeat runs | Review screenshot |",
    `\`\`\`sh\n${fixture.code}\n\`\`\``,
    "### Acceptance\n\n- [x] Record the baseline\n- [ ] Repeat on the pilot configuration\n- [ ] Review the result with operations",
    `[Rehearsal notes](https://example.com/fallow-reach/${spec.key.toLowerCase()})`,
  ].join("\n\n");
}

async function seedProject(
  admin: Client,
  spec: (typeof projects)[number],
  index: number,
  initiativeId: string,
  actors: Map<string, Actor>,
  labelIds: Map<string, string>,
  history: SeedHistory | null,
) {
  const path = `/projects/${spec.key}`;
  const lead = actors.get(spec.lead)!;
  const captain = actors.get(spec.captain)!;
  let project = await admin.find<Project>(path);
  if (!project) {
    project = await admin.write<Project>("POST", "/projects", {
      name: spec.name,
      key: spec.key,
      leadId: lead.userId,
      description: spec.brief,
      status: spec.status,
      priority: index === 3 ? 1 : 2,
      startDate: day(-24),
      targetDate: day(12 + index * 3),
    });
  }
  if (project.initiativeId !== initiativeId) {
    project = await admin.write<Project>("PUT", `${path}/initiative`, {
      initiativeId,
      version: project.version,
    });
  }
  if (project.estimateScale === "notUsed") {
    ({ project } = await admin.write<{ project: Project }>("PUT", `${path}/estimate-scale`, {
      scale: "fibonacci",
      version: project.version,
    }));
  }
  const states = await admin.all<Identified & { type: string }>(`${path}/workflow-states`);
  const existing = await admin.all<SeedIssue>(`/issues?project=${spec.key}&includeArchived=true`);
  const result: SeedIssue[] = [];
  for (const [offset, [title, summary, state, label, priority, parent]] of spec.issues.entries()) {
    const number = offset + 1;
    let issue = existing.find((row) => row.title === title);
    const text = description(spec, summary, number);
    if (!issue) {
      issue = await (offset % 2 === 0 ? captain.api : lead.api).write<SeedIssue>(
        "POST",
        "/issues",
        {
          projectId: project.id,
          title,
          summary,
          description: text,
          stateId: states.find((row) => row.type === (state === "duplicate" ? "triage" : state))!
            .id,
          assigneeId: state === "backlog" ? null : offset % 3 === 0 ? captain.userId : lead.userId,
          ownerId: lead.userId,
          priority,
          estimate: [2, 3, 5, 8][offset % 4],
          dueDate: day(state === "completed" ? -7 + (offset % 5) : 7 + (offset % 8)),
          labelIds: [labelIds.get(label)!],
          parentId: parent ? result[parent - 1]!.id : null,
          ...(state === "duplicate" ? { relations: { duplicateOf: result[0]!.id } } : {}),
        },
      );
      await history?.issue(issue.id, issueDate(index, number));
    } else if (number <= 5 && issue.version === 1) {
      // Enrich the old starter issues while preserving existing descriptions and edits.
      const detail = await admin.get<SeedIssue & { description: string | null }>(
        `/issues/${issue.id}`,
      );
      if (!detail.description) {
        issue = await admin.write<SeedIssue>("PATCH", `/issues/${issue.id}`, {
          version: detail.version,
          description: text,
        });
      }
    }
    result.push(issue);
  }
  return { ...project, issues: result };
}

async function addComment(
  author: Actor,
  issue: SeedIssue,
  text: string | readonly [string, string],
  when: string,
  history: SeedHistory | null,
  parent?: string,
) {
  const [body, previous] = typeof text === "string" ? [text, undefined] : text;
  const path = `/issues/${issue.id}/comments`;
  const prior = (await author.api.all<Comment>(parent ? `${path}/${parent}/replies` : path)).find(
    (row) =>
      (row.body === body || row.body === previous) && row.actualAuthor.userId === author.userId,
  );
  if (prior) return prior.id;
  const saved = await author.api.write<Comment>("POST", path, {
    body,
    ...(parent ? { parentCommentId: parent } : {}),
  });
  await history?.comment(issue.id, saved.id, when);
  return saved.id;
}

async function seedTodos(author: Actor, issue: SeedIssue, texts: readonly string[]) {
  const path = `/issues/${issue.id}/todos`;
  const prior = await author.api.all<Todo>(path);
  const missing = texts.filter((text) => !prior.some((row) => row.text === text));
  if (!missing.length) return;
  const { items } = await author.api.write<{ items: Todo[] }>("POST", path, {
    items: missing.map((text) => ({ text })),
  });
  const first = items.find((item) => item.text === texts[0]);
  if (first)
    await author.api.write("PATCH", `${path}/${first.id}`, { version: first.version, done: true });
}

async function seedCollaboration(
  spec: (typeof projects)[number],
  index: number,
  rows: SeedIssue[],
  actors: Map<string, Actor>,
  history: SeedHistory | null,
) {
  const fixture = evidence[spec.key];
  const lead = actors.get(spec.lead)!;
  const captain = actors.get(spec.captain)!;
  const issue = rows[0]!;
  const media = await issueFile(lead.api, issue.id, fixture.file, image(fixture.file), "images");
  const screenshot = `\n\n![${fixture.alt}](nett-media:${media.id})`;
  const report = fixture.report + screenshot;
  const thread = await addComment(
    lead,
    issue,
    "previousReport" in fixture ? [report, fixture.previousReport + screenshot] : report,
    dateAt(-12 + index, 10),
    history,
  );
  await addComment(captain, issue, fixture.response, dateAt(-9 + index, 14), history, thread);
  await addComment(
    actors.get("developer")!,
    issue,
    fixture.next,
    dateAt(-6 + index, 15),
    history,
    thread,
  );
  await seedTodos(captain, issue, fixture.todos);

  const secondary = rows[1]!;
  const followUp = [
    [
      "The drift only starts after the second burn. Use three waypoints and keep the relay's original coordinates in the trace.",
      "Confirmed. I will add a case that recomputes the second leg without changing the first.",
    ],
    [
      "The six-hour report stalls after 18,000 samples. Shorter reports stay responsive.",
      "I can page by sample range. Let's keep the same CSV column order for comparisons.",
    ],
    [
      "At regional zoom the eastern checkpoint label covers the corridor. The checkpoint should yield first.",
      "I will keep the corridor pinned and use the narrow-window snapshot for the review.",
    ],
    [
      "The new counters agree after the first handover. We still need the delayed acknowledgement case.",
      "I can add the delayed case to the simulator run and compare sent and replayed counts.",
    ],
  ][index]!;
  const followUpThread = await addComment(
    lead,
    secondary,
    followUp[0]!,
    dateAt(-8 + index, 10),
    history,
  );
  await addComment(
    captain,
    secondary,
    followUp[1]!,
    dateAt(-5 + index, 15),
    history,
    followUpThread,
  );
  if (spec.key === "NAV" || spec.key === "RELAY") {
    const log = spec.key === "NAV" ? "tile-replay.txt" : "relay-handover.txt";
    const text =
      spec.key === "NAV"
        ? "09:41:01 replay=fallow-reach cache=cold\n09:41:02 decode_ms=87 pointer_ms=4\n09:41:04 decode_ms=41 cache=warm\n"
        : "relay handover rehearsal\nframes sent: 240\nframes acknowledged: 240\nreplays filtered: 1\n";
    const attachment = await issueFile(
      captain.api,
      issue.id,
      log,
      new Blob([text], { type: "text/plain" }),
      "attachments",
    );
    await addComment(
      captain,
      issue,
      `[Download the rehearsal trace](nett-media:${attachment.mediaId})`,
      dateAt(-2, 11),
      history,
      thread,
    );
  }
  return media.url;
}

async function seedDecisions(
  admin: Client,
  projectsByKey: Map<string, { id: string; issues: SeedIssue[]; media: string }>,
) {
  const unsupported = [404, 405, 501];
  const notice = "Demo decisions skipped: this API does not expose decision writes and reads.";
  const enabled = await admin.find("/decisions?limit=1", unsupported);
  if (!enabled) {
    console.error(notice);
    return;
  }

  const known = new Map(
    (await admin.all<SeedIssue>("/decisions")).map((row) => [row.title, row.id]),
  );
  for (const spec of decisions) {
    if (known.has(spec.title)) continue;
    const project = projectsByKey.get(spec.key)!;
    const issueIds = spec.issues.map((number) => project.issues[number - 1]!.id);
    const created = await admin.optionalWrite<Identified>(
      "POST",
      "/decisions",
      {
        projectId: project.id,
        issueIds,
        title: spec.title,
        decision: spec.decision,
        why: spec.why,
        context: `Pilot evidence is attached to ${project.issues[0]!.identifier}.`,
        evidenceKind: "documentOrUrl",
        evidenceReference: project.media,
        evidenceExcerpt: evidence[spec.key].alt,
        implications: "Update the rehearsal checklist and use this result for the next review.",
        status: spec.status,
        decidedOn: day(spec.day),
        tags: ["pilot", "fallow-reach"],
        supersedesId: "supersedes" in spec ? known.get(spec.supersedes) : null,
      },
      unsupported,
    );
    if (!created) {
      console.error(notice);
      return;
    }
    known.set(spec.title, created.id);
  }
}

async function main() {
  const history = rootStack
    ? await seedHistory(process.env.DATABASE_URL, identity.organisation)
    : null;
  try {
    const developer = await signIn(identity.email, identity.name);
    const { api } = developer;
    const fleetIds = await seedFleets(api);
    if (workspaceOnly) {
      console.error("Local workspace ready (demo data disabled).");
      return;
    }

    const actors = await seedActors(developer);
    const initiativeIds = await seedInitiatives(developer, actors, fleetIds);
    const labelIds = new Map(
      (await api.all<Identified & { name: string }>("/labels?includeArchived=true")).map((row) => [
        row.name,
        row.id,
      ]),
    );
    for (const label of labels) {
      if (!labelIds.has(label.name)) {
        const { id } = await api.write<Identified>("POST", "/labels", label);
        labelIds.set(label.name, id);
      }
    }

    const github = await demoGitHub(api, rootStack);
    const projectsByKey = new Map<string, { id: string; issues: SeedIssue[]; media: string }>();
    for (const [index, spec] of projects.entries()) {
      const project = await seedProject(
        api,
        spec,
        index,
        initiativeIds[spec.initiative]!,
        actors,
        labelIds,
        history,
      );
      const media = await seedCollaboration(spec, index, project.issues, actors, history);
      await github?.(spec.key, project.issues, index, spec.captain);
      projectsByKey.set(spec.key, { ...project, media });
    }
    await seedDecisions(api, projectsByKey);
    if (includeChat) {
      const screenshot = await chatScreenshot(developer.api, evidence.NAV.file);
      const byKey = new Map([...projectsByKey].map(([key, row]) => [key, row.issues]));
      console.log(JSON.stringify(conversations(byKey, screenshot)));
    }
    console.error(
      `Fallow Reach demo ready: ${projects.length} projects, ${projects.reduce((sum, project) => sum + project.issues.length, 0)} issues.`,
    );
  } finally {
    await history?.close();
  }
}

await main();
