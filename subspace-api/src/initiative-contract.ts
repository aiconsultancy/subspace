import { Schema } from "effect";
import { Fleet } from "./fleets";
import {
  DateOnly,
  Line,
  Name,
  NullableString,
  PageQuery,
  Paragraph,
  Timestamp,
  Uuid,
} from "./validation";

const FleetFields = { fleetId: Schema.NullOr(Uuid), fleet: Schema.NullOr(Fleet) };
const Status = Schema.Literals(["planned", "active", "completed", "canceled"]);
const Channels = Paragraph(8192);
const MemberIds = Schema.Array(Uuid).check(Schema.isMaxLength(500));
const Description = Paragraph(65_536);
export const Version = Schema.Int.check(Schema.isBetween({ minimum: 1, maximum: 2147483647 }));

const StakeholderFields = {
  name: Name,
  role: Name,
  slackHandle: Schema.NullOr(Line(255)),
  teamsHandle: Schema.NullOr(Line(255)),
};
export const Stakeholder = Schema.Struct({ id: Uuid, ...StakeholderFields }).annotate({
  identifier: "InitiativeStakeholder",
});
export const NewStakeholder = Schema.Struct({
  ...StakeholderFields,
  slackHandle: Schema.optional(StakeholderFields.slackHandle),
  teamsHandle: Schema.optional(StakeholderFields.teamsHandle),
});
export const EditStakeholder = NewStakeholder.mapFields((fields) => ({
  ...fields,
  name: Schema.optional(fields.name),
  role: Schema.optional(fields.role),
}));
export const MemberInitiative = Schema.Struct({
  id: Uuid,
  name: Name,
  channels: Channels,
  stakeholders: Schema.Array(Stakeholder),
  ...FleetFields,
}).annotate({ identifier: "MemberInitiative" });

export const Initiative = Schema.Struct({
  ...MemberInitiative.fields,
  parentId: Schema.NullOr(Uuid),
  level: Schema.Int,
  path: Schema.Array(Schema.Struct({ id: Uuid, name: Name, archived: Schema.Boolean })),
  childCount: Schema.Int,
  status: Status,
  ownerId: Uuid,
  ownerName: NullableString,
  ownerAvailable: Schema.Boolean,
  targetDate: Schema.NullOr(DateOnly),
  archived: Schema.Boolean,
  ancestorArchived: Schema.Boolean,
  rollUp: Schema.Struct({
    projectCount: Schema.Int,
    completedCount: Schema.Int,
    openHealth: Schema.Struct({
      offTrack: Schema.Int,
      atRisk: Schema.Int,
      onTrack: Schema.Int,
      noUpdate: Schema.Int,
    }),
  }),
  version: Version,
  createdAt: Timestamp,
  updatedAt: Timestamp,
}).annotate({ identifier: "Initiative" });

export const InitiativeDetail = Schema.Struct({
  ...Initiative.fields,
  description: NullableString,
  directProjectCount: Schema.Int,
  today: DateOnly,
}).annotate({ identifier: "InitiativeDetail" });

export const InitiativePage = Schema.Struct({
  items: Schema.Array(Initiative),
  nextCursor: NullableString,
  totalCount: Schema.Int,
  maxLevel: Schema.Int,
  today: DateOnly,
}).annotate({ identifier: "InitiativePage" });

export const InitiativeQuery = Schema.Struct({
  ...PageQuery,
  search: Schema.optional(Paragraph(200)),
  ownerId: Schema.optional(Uuid),
  status: Schema.optional(Status),
  includeArchived: Schema.optional(Schema.Boolean),
  rootId: Schema.optional(Uuid),
  assignable: Schema.optional(Schema.Boolean),
});

export const CreateInitiative = Schema.Struct({
  name: Name,
  ownerId: Uuid,
  description: Schema.optional(Schema.NullOr(Description)),
  status: Schema.optional(Status),
  targetDate: Schema.optional(Schema.NullOr(DateOnly)),
  parentId: Schema.optional(Schema.NullOr(Uuid)),
  channelConfiguration: Schema.optional(
    Schema.Struct({
      channels: Schema.optional(Channels),
      memberUserIds: Schema.optional(MemberIds),
    }),
  ),
});

export const UpdateInitiative = Schema.Struct({
  name: Schema.optional(Name),
  ownerId: Schema.optional(Uuid),
  description: Schema.optional(Schema.NullOr(Description)),
  status: Schema.optional(Status),
  targetDate: Schema.optional(Schema.NullOr(DateOnly)),
  parentId: Schema.optional(Schema.NullOr(Uuid)),
  version: Version,
});

export const ChannelChanges = Schema.Struct({
  version: Version,
  channels: Schema.optional(Channels),
  addMemberUserIds: Schema.optional(MemberIds),
  removeMemberUserIds: Schema.optional(MemberIds),
});

export const ChannelConfiguration = Schema.Struct({
  id: Uuid,
  version: Version,
  channels: Channels,
  memberUserIds: MemberIds,
});
