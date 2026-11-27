# Issue owners in the Issues Space

## Intent

Show who is accountable for an issue and who is doing the work. Let people
change the owner from the issue page.

## Behaviour

- Properties places Owner beside Assignee. Both the inline editor and
  **Edit issue…** use the existing property picker and save flow. The owner
  picker lists eligible humans from the people catalogue. It can set or
  replace an owner.
- Issue rows place an outlined Owner badge with a shield immediately before
  Assignee. Each badge has a tooltip and accessible label with the role and
  the person's name. Empty slots indicate when either role is unset.
- Saving sends the selected `ownerId` with the expected issue version and
  refreshes the issue list and detail from the service. The existing editor handles
  version conflicts and failed saves.

## Checks

The native tests cover the human-only picker, the versioned patch, the saved
value, the display of a former member as the current owner, and accessible
labels for both list badges. The change
reuses the Issues Space's existing layout, picker, cache and registration.

## Evidence

Captured with `make capture` in Twilight Two against the loopback fixture.

- Issues list. [Before](evidence/list-before.png) · [After](evidence/list-after.png)
- Issue properties. [Before](evidence/detail-before.png) · [After](evidence/detail-after.png)
- [Owner picker](evidence/owner-editor.png)
- List tooltips. [Owner](evidence/owner-tooltip.png) · [Assignee](evidence/assignee-tooltip.png)
