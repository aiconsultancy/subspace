//! One browser destination for buttons, shortcuts, menus and chat replay.

use subspace_framework::client_github::copilot_agents::{
    COPILOT_AGENTS_HOME_URL, session_browser_url,
};
use subspace_framework::dashboard::{ServiceLink, links};

use crate::model::{AgentRow, Kind};

pub(crate) fn link(row: &AgentRow) -> Option<ServiceLink> {
    if row.kind != Kind::Cloud {
        return None;
    }
    let url = links::safe_url(&row.html_url)
        .or_else(|| session_browser_url(&row.pull_request.as_ref()?.url, None))
        .unwrap_or_else(|| COPILOT_AGENTS_HOME_URL.to_owned());
    let label = if url == COPILOT_AGENTS_HOME_URL {
        "Open GitHub Agents in browser (o)"
    } else {
        "Open session in browser (o)"
    };
    Some(ServiceLink::new(label, url))
}

#[cfg(test)]
mod tests {
    use subspace_framework::client_github::copilot_agents::{
        COPILOT_AGENTS_HOME_URL, PullRequestArtifact,
    };
    use subspace_framework::client_github::model::PrState;

    use super::link;
    use crate::model::{Kind, tests::row};

    #[test]
    fn cloud_links_fall_back_truthfully_and_local_chats_never_get_a_browser_link() {
        let mut cloud = row("t1", Kind::Cloud, "completed", "");
        cloud.html_url.clear();
        let home = link(&cloud).unwrap();
        assert_eq!(home.url, COPILOT_AGENTS_HOME_URL);
        assert!(home.label.contains("GitHub Agents"));
        cloud.pull_request = Some(PullRequestArtifact {
            number: 7,
            url: "https://github.com/Owner/one/pull/7".into(),
            state: PrState::Open,
        });
        let focused = link(&cloud).unwrap();
        assert_eq!(
            focused.url,
            "https://github.com/Owner/one/pull/7/agent-sessions"
        );
        assert!(focused.label.contains("session"));
        cloud.kind = Kind::Local;
        assert!(link(&cloud).is_none());
    }
}
