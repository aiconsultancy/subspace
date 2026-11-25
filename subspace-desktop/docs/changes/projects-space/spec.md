# Projects behavior

Projects follows the shared initiative selection. The navigator lists projects in that initiative and marks archived records. Overview and Milestones use the same centered reading column as Issues, with left-aligned content. The project’s Overview shows its lifecycle, health, priority, lead, dates, description, milestone timeline and latest update.

The Milestones view lists current status and issue progress. It supports creating, editing, reordering, completing and reopening milestones. Deletion asks for confirmation. Archived projects allow browsing.

The client uses the existing project, update and milestone routes. Subspace uses PATCH for a milestone edit. NETT uses its organisation-scoped PUT. Both send the record version, and a failure that leaves the result uncertain requires reviewing the current record before trying again.
