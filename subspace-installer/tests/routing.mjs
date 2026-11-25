import assert from "node:assert/strict";
import { dirname, join } from "node:path";

const runtime = await Bun.file("/etc/subspace-fleet/gateway.json").json();
const source = dirname(runtime.entrypoint);
const { ircSchema } = await import(`${source}/config.ts`);
const transport = join(source, "transports/irc");
const irc = await Bun.file(join(transport, "routing.ts")).exists() ? transport : join(source, "irc");
const { fold, parseMessage } = await import(join(irc, "protocol.ts"));
const { routeIrcMessage } = await import(join(irc, "routing.ts"));
const raw = await Bun.file(runtime.config).json();
// Older Stargate releases used zod schemas; current ones use Effect Schema.
const parse = typeof ircSchema.parse === "function"
  ? (entry) => ircSchema.parse(entry)
  : (await import(Bun.resolveSync("effect", source))).Schema.decodeUnknownSync(ircSchema);
const roster = Object.fromEntries(raw.irc.map((entry) => [entry.id, parse(entry)]));

function admitted(name, channel, account, text) {
  const config = roster[name];
  const nick = config.account;
  return Boolean(routeIrcMessage(
    parseMessage(`@account=${account};msgid=fixture-turn :${account}!u@localhost PRIVMSG ${channel} :${text}`),
    {
      config,
      member: { id: name, key: name, role: "captain", account: nick, nick, channels: config.channels },
      currentNick: nick,
      fold,
      now: () => 1,
      sent: () => false,
      joined: (target) => config.channels.includes(target),
    },
  ));
}

assert(admitted("orion", "#example-fleet-orion", "operator", "Please review the build."));
assert(!admitted("lyra", "#example-fleet-orion", "operator", "Please review the build."));
assert(admitted("lyra", "#example-fleet-orion", "operator", "@example-lyra: please review."));
assert(!admitted("lyra", "#example-fleet-orion", "example-orion", "Please review the build."));
assert(admitted("lyra", "#example-fleet-orion", "example-orion", "@example-lyra: please review."));
assert(!admitted("lyra", "#example-fleet-orion", "unlisted", "@example-lyra: please review."));
assert(admitted("atlas", "#example-fleet-starbase", "operator", "Let's plan the next release."));
assert(!admitted("orion", "#example-fleet-starbase", "operator", "@example-orion: please review."));
// The fleet command channel wakes an agent only when it is tagged, by a person or its rear admiral.
const command = roster.atlas.channels[1];
assert(!admitted("orion", command, "operator", "Please review the build."));
assert(admitted("orion", command, "operator", "@example-orion: please review."));
assert(admitted("orion", command, "example-atlas", "@example-orion: please pick up the issue."));
assert(!admitted("orion", command, "unlisted", "@example-orion: please review."));
assert(admitted("orion", "#example-fleet-orion", "example-medical", "example-orion: Sentry HTTP 500"));
assert(!admitted("orion", "#example-fleet-orion", "example-medical", "Sentry HTTP 500"));
assert(!admitted("lyra", "#example-fleet-orion", "example-medical", "@example-lyra: Sentry HTTP 500"));
