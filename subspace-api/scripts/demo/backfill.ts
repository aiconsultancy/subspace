// What a re-run of the demo seed adds to records an older seed created, without overwriting
// values someone set since.

export type SeededFleet = {
  irc: { host: string; port: number; tls: boolean };
  starbaseChannel: string | null;
  commandChannel: string | null;
};

export type FleetFixture = {
  ircHost: string;
  ircPort: number;
  ircTls: boolean;
  starbaseChannel?: string;
  commandChannel?: string;
};

export type RegisteredAgent = {
  userId: string;
  name: string;
  rank: string;
  fleet: string;
  ship: string | null;
  fleetId: string | null;
  shipId: string | null;
  defaultModel: string;
  serviceTokenClientId: string;
  ircNick: string | null;
  version: number;
};

/**
 * The fleet fields a re-run changes: the IRC server when it differs, and each standard channel
 * the fixture names that the fleet does not record yet. A channel someone set is kept.
 */
export function missingFleetFields(fleet: SeededFleet, fixture: FleetFixture) {
  const changes: Record<string, unknown> = {};
  if (
    fleet.irc.host !== fixture.ircHost ||
    fleet.irc.port !== fixture.ircPort ||
    fleet.irc.tls !== fixture.ircTls
  ) {
    Object.assign(changes, {
      ircHost: fixture.ircHost,
      ircPort: fixture.ircPort,
      ircTls: fixture.ircTls,
    });
  }
  for (const key of ["starbaseChannel", "commandChannel"] as const) {
    const channel = fixture[key];
    if (channel && fleet[key] === null) changes[key] = channel;
  }
  return changes;
}

/**
 * The full edit that gives an agent its nick, or null when it already has one. The edit repeats
 * the agent's registration, as the API replaces it whole, and its version.
 */
export function nickEdit(agent: RegisteredAgent, ircNick: string) {
  if (agent.ircNick !== null) return null;
  return {
    name: agent.name,
    rank: agent.rank,
    fleet: agent.fleet,
    ship: agent.ship,
    fleetId: agent.fleetId,
    shipId: agent.shipId,
    defaultModel: agent.defaultModel,
    serviceTokenClientId: agent.serviceTokenClientId,
    ircNick,
    version: agent.version,
  };
}
