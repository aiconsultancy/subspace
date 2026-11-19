use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;

use super::{mount_open, project_service};
use crate::project::{EXAMPLE_DEFINITION, ProjectDefinition};

#[gpui_kit::test]
fn starter_dashboard_is_visible_below_project_header(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let service = project_service::shared(root.path()).unwrap();
    let definition = ProjectDefinition::parse(EXAMPLE_DEFINITION.as_bytes()).unwrap();
    service
        .create(
            &definition,
            std::collections::BTreeMap::from([(
                "main.py".into(),
                b"# test-only source\n".to_vec(),
            )]),
        )
        .unwrap();
    let initial = service.open("native-demo").unwrap();
    let (_, cx) = mount_open(root.path(), initial, cx);
    cx.update(|window, cx| {
        window.render_frame(cx);
        let header = window.find("project-close");
        let summary = window.find("project-native-demo-summary");
        let rate = window.find("project-native-demo-rate");
        assert!(summary.visible(), "starter summary is clipped");
        assert!(rate.visible(), "starter slider is clipped");
        assert!(summary.bounds().top() >= header.bounds().bottom());
        assert!(rate.bounds().top() >= summary.bounds().bottom());
    });
}
