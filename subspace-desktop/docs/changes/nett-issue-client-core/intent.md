# Intent: NETT issue client core

Load NETT dev issues in Subspace as the first part of the Issues and Channels initiative. This
delivery supplies the non-UI core so the following Claude crew can build the
issue panes from NETT's desktop design without reimplementing transport,
authentication, credentials or IRC reference parsing.

The first slice is readonly. It uses the existing Indra CLI public WorkOS dev
client, with no new application, client secret or service token. Design
design, specification and development edge admission remain
separate. Tests are required to use offline loopback fixtures.

The owner separately approved the Defiant Subspace clone and feature branches
from `feat/subspace-desktop`. The worker commits for the captain's review,
without pushing, opening a PR, deploying or changing NETT.
