use super::{project_label, project_options};
use crate::{fixtures::project, model::ProjectScope};
#[test]
fn picker_lists_active_projects_and_all_projects() {
    let projects = [
        project("SUB", "Subspace", false),
        project("OLD", "Archived", true),
    ];
    let options = project_options(&projects);
    assert_eq!(
        options,
        [
            (ProjectScope::Project("SUB".into()), "SUB · Subspace".into()),
            (ProjectScope::AllProjects, "All projects".into())
        ]
    );
    assert_eq!(
        project_label(&ProjectScope::Project("SUB".into()), &projects),
        "SUB · Subspace"
    );
    assert_eq!(
        project_label(&ProjectScope::AllProjects, &[]),
        "All projects"
    );
}
