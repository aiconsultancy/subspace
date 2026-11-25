use gpui_kit::{AppContext as _, Task, TestAppContext};
use subspace_api_client::projects::PlanningProject;
use subspace_framework::dashboard::Dashboard;

use crate::{Projects, Record};

fn record(initiative: &str) -> Record {
    let project = PlanningProject {
        id: "project-1".into(),
        key: Some("EX".into()),
        name: "Example project".into(),
        lead_name: None,
        status: "started".into(),
        health: None,
        priority: 2,
        start_date: None,
        target_date: None,
        description: None,
        archived: false,
        initiative_id: Some(initiative.into()),
        initiative: None,
    };
    Record {
        project,
        milestones: Ok(Vec::new()),
        update: Ok(None),
    }
}

#[gpui_kit::test]
fn directory_reload_and_an_initiative_move_disable_milestone_writes(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_, board) = cx.update(|cx| {
        gpui_kit::open_window(Default::default(), cx, |window, cx| {
            cx.new(|cx| Projects::new(window, cx))
        })
        .unwrap()
    });
    board.update(cx, |this, cx| {
        this.initiative_id = Some("initiative-1".into());
        this.initiative_hidden = false;
        let current = record("initiative-1");
        this.selected = Some(current.project.id.clone());
        this.projects = Ok(vec![current.project.clone()]);
        this.record = Some(Ok(current));
        assert!(this.may_write());
        this.directory_task = Some(Task::ready(()));
        assert!(
            !this.may_write(),
            "wait until the directory refresh has finished"
        );
        this.directory_task = None;
        this.apply_record(Ok(record("initiative-2")), cx);
        assert!(!this.may_write());
        assert!(this.selected.is_none());
        assert_eq!(this.visible_projects().count(), 0);
    });
}
