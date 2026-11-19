import assert from "node:assert/strict";
import { dirname } from "node:path";

const runtime = await Bun.file("/etc/subspace-fleet/gateway.json").json();
const source = dirname(runtime.entrypoint);
const { ircSchema } = await import(`${source}/config.ts`);
const { fold, parseMessage } = await import(`${source}/irc/protocol.ts`);
const { routeIrcMessage } = await import(`${source}/irc/routing.ts`);
const raw = await Bun.file(runtime.config).json();
const roster = Object.fromEntries(raw.irc.map((entry) => [entry.id, ircSchema.parse(entry)]));

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

assert(admitted("orion", "#orion-work", "operator", "Please review the build."));
assert(!admitted("lyra", "#orion-work", "operator", "Please review the build."));
assert(admitted("lyra", "#orion-work", "operator", "@example-lyra: please review."));
assert(!admitted("lyra", "#orion-work", "example-orion", "Please review the build."));
assert(admitted("lyra", "#orion-work", "example-orion", "@example-lyra: please review."));
assert(!admitted("lyra", "#orion-work", "unlisted", "@example-lyra: please review."));
assert(admitted("atlas", "#starbase", "operator", "Let's plan the next release."));
assert(!admitted("orion", "#starbase", "operator", "@example-orion: please review."));
assert(admitted("orion", "#orion-work", "example-medical", "example-orion: Sentry HTTP 500"));
assert(!admitted("orion", "#orion-work", "example-medical", "Sentry HTTP 500"));
assert(!admitted("lyra", "#orion-work", "example-medical", "@example-lyra: Sentry HTTP 500"));
