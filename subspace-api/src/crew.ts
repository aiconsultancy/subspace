import { Effect, Schema } from "effect";
import { type Member } from "./identity";
import { problem } from "./problems";
import { Line } from "./validation";

export const RelayedCrew = Schema.Struct({ name: Line(200), model: Line(100) });
export const OptionalRelay = Schema.optional(Schema.NullOr(RelayedCrew));

export function withCrew(member: Member, relayedCrew?: typeof RelayedCrew.Type | null) {
  if (relayedCrew && member.agentRank !== "captain" && member.agentRank !== "rearAdmiral") {
    return Effect.fail(problem(400, "Only a captain or rear admiral may relay crew work."));
  }
  return Effect.succeed({ ...member, relayedCrew: relayedCrew ?? null });
}
