use serde_json::json;

use super::{fixture, invocation};
use crate::project_tools::tool;

#[tokio::test]
async fn export_uses_fixed_destination_and_never_clobbers() {
    let root = tempfile::tempdir().unwrap();
    let revision = fixture(root.path());
    let edit = tool(root.path().to_owned(), true);
    let arguments = json!({"action":"export", "id":"demo", "expected":revision});
    let (decision, call) = invocation(arguments.clone(), true);
    edit.call(call).await.unwrap();
    let output = root
        .path()
        .join("project-exports")
        .join(format!("demo-{}.zip", revision.source));
    assert!(output.is_file());
    {
        let summaries = decision.summaries.lock().unwrap();
        assert_eq!(summaries.len(), 1);
        assert!(summaries[0].contains(output.to_str().unwrap()));
        assert!(summaries[0].to_lowercase().contains("credential"));
    }
    let before = std::fs::read(&output).unwrap();
    let (_, call) = invocation(arguments, true);
    assert!(edit.call(call).await.is_err());
    assert_eq!(std::fs::read(output).unwrap(), before);
}
