# Projects behavior

Projects follows the shared initiative selection. The navigator lists projects in that initiative and marks archived records. Overview and Milestones use the same centered reading column as Issues, with left-aligned content. The project’s Overview shows its lifecycle, health, priority, lead, dates, description, milestone timeline and latest update.

The Milestones view lists current status and issue progress. It supports creating, editing, reordering, completing and reopening milestones. Deletion asks for confirmation. Archived projects allow browsing.

The client uses the existing project, update and milestone routes. Subspace uses PATCH for a milestone edit. NETT uses its organisation-scoped PUT. Both send the record version, and a failure that leaves the result uncertain requires reviewing the current record before trying again.

Overview offers a versioned project editor for the name, description, status, priority and dates. The desktop uses the existing project PUT on Subspace and NETT. Timeline milestones preview linked issues and open the same detail page as the Milestones list. The detail reads every page of the existing milestone issue filter, including archived linked issues the API allows the person to see. Selecting an issue uses the Issues navigation contract. Back restores the milestone and its scroll position.
