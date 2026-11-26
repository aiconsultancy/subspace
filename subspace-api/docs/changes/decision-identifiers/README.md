# Project decision identifiers

## Intent

Give decisions short references that people can cite in Channels and pull
requests and that show the owning project at a glance.

## Specification

See the [decision API contract](../../../README.md#decisions). The server
returns an immutable `identifier` alongside each decision UUID. REST and
MCP accept either form for a decision lookup or mutation. Relationship
payloads keep their UUID fields.

## Delivery

Apply the decision identifier migration with the API upgrade. It populates
existing references in creation order with UUID as the tie-breaker, seeds
each project's counter and adds the references to saved revision snapshots.
New decisions allocate their identifier in the same transaction as the
decision, revision and issue links.

Desktop will display and follow references after the API change ships.

## Tasks

- [x] Implement the migration, REST and MCP contract and API documentation.
- [x] Cover upgrades, concurrent creation and resolution with PostgreSQL.
