//! Fixture records shared by the board's tests.

use crate::{
    load::mapping,
    model::{Record, RunTarget},
};

/// A successful `CI` run on `main` by `ann` in `repo`.
pub(crate) fn run(repo: &str) -> Record {
    let mut record = mapping::notice(repo, "run-1", "CI");
    record.status = "success".into();
    record.run = Some(RunTarget {
        repo: repo.into(),
        id: 1,
        workflow_id: 2,
        workflow: "CI".into(),
        path: String::new(),
        sha: String::new(),
        branch: "main".into(),
        actor: "ann".into(),
        attempt: 1,
    });
    record
}
