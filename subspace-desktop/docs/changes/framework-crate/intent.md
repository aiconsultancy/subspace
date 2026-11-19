# The Framework as its own crate: intent

Back to the [change records](../README.md). Earlier research:
[Framework split](../framework-split/research.md).

The owner asked to finish separating the Subspace Framework from Subspace
Desktop:

- The Framework lives in its own folder, `subspace-framework/`, a sibling of
  `subspace-desktop/` at the repository root.
- Subspace Desktop imports the Framework as a crate, `subspace-framework`.
- The separation is clean enough that the two folders could become two
  repositories without further work.
- The aim is to publish `subspace-framework` to crates.io. Publishing is not
  part of this work, but the Framework must be ready: once the owner
  supplies a crates.io token, it can be published and Subspace Desktop can
  import the published crate.

The owner asked for this to be carried through unattended.
