//! Reconcile the visible project with changes to the API catalogue.

use gpui_kit::Context;
use subspace_api_client::model::Project;

use crate::{Issues, initiative};

// qual:allow(coupling, oi) reason: "The board applies catalogue changes, the project choice and the visible list together."
impl Issues {
    pub(crate) fn set_projects(&mut self, projects: Vec<Project>, cx: &mut Context<Self>) {
        let current = match (self.projects.is_empty(), initiative::saved_project(cx)) {
            (true, saved) if self.initiative_id.is_some() => saved,
            (true, None) => None,
            _ => Some(self.project.clone()),
        };
        self.projects = projects;
        let project = initiative::scoped_project(self, current);
        if project != self.project {
            self.cancel_search_fallback();
            self.set_list_project(project);
            self.listing.clear();
            self.read_visible(cx);
        }
        crate::pickers::refresh_project_menu(self, cx);
        cx.notify();
    }
}
