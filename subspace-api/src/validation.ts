import { Schema } from "effect";

const hasControl = (value: string) =>
  Array.from(value).some(
    (character) => character.charCodeAt(0) < 32 || character.charCodeAt(0) === 127,
  );

export const Uuid = Schema.String.check(Schema.isUUID());
export const OptionalString = Schema.optional(Schema.String);
export const NullableString = Schema.NullOr(Schema.String);

export const Name = Schema.String.check(
  Schema.isMinLength(1),
  Schema.isMaxLength(120),
  Schema.makeFilter((value) => value.trim() === value && !hasControl(value), {
    expected: "a single line without control characters",
  }),
);
export const Slug = Schema.String.check(
  Schema.isPattern(/^[a-z0-9]+(?:-[a-z0-9]+)*$/),
  Schema.isMaxLength(80),
);
export const Email = Schema.String.check(
  Schema.isPattern(/^[^\s@]+@[^\s@]+\.[^\s@]+$/u),
  Schema.isMaxLength(254),
);
export const Host = Schema.String.check(
  Schema.isPattern(/^(?:[a-zA-Z0-9](?:[a-zA-Z0-9.-]*[a-zA-Z0-9])?|\[?[0-9a-fA-F:]+\]?)$/),
  Schema.isMaxLength(253),
);
export const Port = Schema.Int.check(Schema.isBetween({ minimum: 1, maximum: 65535 }));
export const Timestamp = Schema.String;
export const dateTime = (value: Date) => value.toISOString();
export const today = () => new Date().toISOString().slice(0, 10);

export const PageQuery = {
  limit: Schema.optional(Schema.Int.check(Schema.isBetween({ minimum: 1, maximum: 200 }))),
  cursor: OptionalString,
};

export const pageOf = <T extends Schema.Constraint>(item: T) =>
  Schema.Struct({ items: Schema.Array(item), nextCursor: Schema.NullOr(Schema.String) });

export const isUuid = (value: string) =>
  /^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/i.test(value);
export const dateTimeOrNull = (value: Date | null) => (value ? dateTime(value) : null);
export const DateOnly = Schema.String.check(
  Schema.isPattern(/^\d{4}-\d{2}-\d{2}$/u),
  Schema.makeFilter(
    (value) => {
      const date = new Date(value);
      return (
        value >= "0001-01-01" &&
        !Number.isNaN(date.valueOf()) &&
        date.toISOString().slice(0, 10) === value
      );
    },
    { title: "Calendar date", expected: "a valid calendar date" },
  ),
);
export const Paragraph = (max: number) =>
  Schema.String.check(
    Schema.isMaxLength(max),
    Schema.makeFilter((value) => !value.includes("\0"), { expected: "text without NUL" }),
  );
export const Line = (max: number) =>
  Paragraph(max).check(
    Schema.isMinLength(1),
    Schema.makeFilter((value) => value.trim() === value && !hasControl(value), {
      expected: "a single line without control characters",
    }),
  );
export const Ref = Line(120);
export const Priority = Schema.Int.check(Schema.isBetween({ minimum: 0, maximum: 4 }));
