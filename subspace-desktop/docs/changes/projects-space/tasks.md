# Projects board checklist

- Identity and registration. Projects uses ID `projects`, the existing folder glyph and the API-backed product registrations. It owns no process.
- Layout. The Framework workspace supplies the navigator and pane. The navigator stays available for project selection. Overview and Milestones use the pane toolbar, a common text edge and native controls. The custom project record uses sections and a date axis.
- Interaction. The Space reports status, load state, menus and dialogs. Reload retries failed requests. The app’s shared initiative selection also applies in Projects.
- Connection and settings. Projects uses the active API account and needs no new configuration. It is absent in no-API builds.
- Data. Project records are held in memory under the active account and dropped when account scope changes. Activation or Reload reads fresh data in the background. Archived projects do not offer writes.
- Search, chat, agent tools and notices. Projects asks the Issues current-scope contract to open linked issues. It exposes no full-text index, chat context, notifications or desktop agent tools.
- Help. The Projects guide is linked from the developer and product indexes.
- Writes. Requests go to the established API origin and use the server’s project or milestone version. An ambiguous response never triggers an automatic retry.
- Verification. The client tests cover both milestone route families and the version and body sent on each write. The milestone editor tests target date validation. The API fixture supplies project and planning data for native capture.
