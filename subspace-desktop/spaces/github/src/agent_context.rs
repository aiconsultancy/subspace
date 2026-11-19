//! What a GitHub selection attaches to the chat: the pull request or issue
//! and everything its detail panel shows, including the body, checks,
//! reviews, labels and the file summary the panel lists.

use subspace_framework::dashboard::context_text::{ContextText, add_detail, reference};
use subspace_framework::dashboard::{AgentContext, Detail};

use crate::model::{branch_text, kind_text, state_text};
use subspace_framework::client_github::model::GhItem;

pub(crate) fn item_context(item: &GhItem, detail: &Detail) -> AgentContext {
    let mut text = ContextText::new();
    add_item(&mut text, item);
    add_detail(&mut text, detail);
    reference(
        format!("github:{}", item.key),
        format!("{}: {}", item.key, item.title),
        text.finish(),
    )
}

fn add_item(text: &mut ContextText, item: &GhItem) {
    text.line("Service", "GitHub");
    text.line("Repository", &item.repo);
    text.line("Number", item.number.to_string());
    text.line("Kind", kind_text(item.kind));
    text.line("Title", &item.title);
    text.line("State", state_text(item.state));
    text.line("Head", branch_text(item.head_ref.as_ref()));
    text.line("Base", branch_text(item.base_ref.as_ref()));
    text.line(
        "Assignees",
        item.assignees
            .iter()
            .map(|user| user.login.as_str())
            .collect::<Vec<_>>()
            .join(", "),
    );
    text.line("URL", &item.url);
}

#[cfg(test)]
mod tests {
    use super::item_context;
    use crate::sample;
    use subspace_framework::dashboard::{Detail, Section};

    #[test]
    fn a_pull_request_carries_its_row_and_its_panel() {
        let item = sample::items().remove(0);
        let detail = Detail {
            kind: "Pull request".into(),
            id: item.key.clone().into(),
            title: item.title.clone().into(),
            link: None,
            props: vec![
                ("Checks".into(), "3 passing".into()),
                ("Reviews".into(), "1 approved".into()),
                ("Labels".into(), "bug".into()),
                ("Files".into(), "4 files, +120 −8".into()),
            ],
            sections: vec![Section::markdown("Body", "Ports the board view.")],
        };
        let context = item_context(&item, &detail);
        assert_eq!(context.key, format!("github:{}", item.key));
        assert!(context.content.contains("Service: GitHub"));
        assert!(context.content.contains("Checks: 3 passing"));
        assert!(context.content.contains("Reviews: 1 approved"));
        assert!(context.content.contains("Files: 4 files, +120 −8"));
        assert!(context.content.contains("\nBody:\nPorts the board view."));
    }
}
