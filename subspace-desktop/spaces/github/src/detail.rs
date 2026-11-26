//! The detail panel's heading and properties. The rest of the panel is in `detail/panel.rs`.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, Div, FontWeight, InteractiveElement as _, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use subspace_framework::client_github::model::{
    GhItem, GhItemDetail, ItemKind, display_name, short_login,
};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::list::{Detail, ServiceLink};

mod actions;
mod panel;

pub use panel::{OnAction, PanelAction, PanelInput, page_sections, panel_extra};

use crate::model::{age, full_date};
use crate::rows::state_label;

/// Assignees past this many collapse into "…".
const ASSIGNEES_SHOWN: usize = 4;

/// What the composer writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Approve,
    Changes,
    Comment,
}

impl Mode {
    pub fn title(self) -> &'static str {
        self.words().0
    }

    pub fn placeholder(self) -> &'static str {
        self.words().1
    }

    /// The mode's title and composer placeholder, in one place per variant.
    // qual:allow(dry, repeated_matches) reason: "This enum has one mapping of its own; rustqual groups unrelated enums only because their three-arm variant matches share one shape."
    fn words(self) -> (&'static str, &'static str) {
        match self {
            Self::Approve => ("Approve", "Optional comment"),
            Self::Changes => ("Request changes", "What needs to change?"),
            Self::Comment => ("Comment", "Leave a comment"),
        }
    }
}

type Prop = (SharedString, SharedString);

/// The panel's heading and properties.
pub fn detail(item: &GhItem, full: Option<&GhItemDetail>, now: i64) -> Detail {
    let mut props: Vec<Prop> = vec![
        ("State".into(), state_label(item).into()),
        ("Author".into(), author_text(item).into()),
    ];
    if item.kind == ItemKind::Pr {
        props.extend(pull_request_props(item, full));
    }
    props.push(("Assignees".into(), assignees_text(item).into()));
    if !item.labels.is_empty() {
        props.push(("Labels".into(), labels_text(item).into()));
    }
    props.push(("Updated".into(), updated_text(item, now).into()));
    heading_of(item, props)
}

fn author_text(item: &GhItem) -> String {
    item.author
        .as_ref()
        .map_or("Unknown".to_string(), |user| display_name(user).to_string())
}

/// The files and branch properties of a pull request.
fn pull_request_props(item: &GhItem, full: Option<&GhItemDetail>) -> Vec<Prop> {
    let files = match full.and_then(|detail| detail.changed_files) {
        Some(1) => "1 file".to_string(),
        Some(count) => format!("{count} files"),
        None => "Show diff".to_string(),
    };
    let stat = item
        .additions
        .zip(item.deletions)
        .map(|(added, deleted)| format!(" · +{added} −{deleted}"))
        .unwrap_or_default();
    let mut props: Vec<Prop> = vec![("Files".into(), format!("{files}{stat} (d)").into())];
    if let (Some(head), Some(base)) = (&item.head_ref, &item.base_ref) {
        props.push(("Branch".into(), format!("{head} → {base}").into()));
    }
    props
}

fn assignees_text(item: &GhItem) -> String {
    if item.assignees.is_empty() {
        return "Nobody (a)".to_string();
    }
    let mut names: Vec<&str> = item
        .assignees
        .iter()
        .take(ASSIGNEES_SHOWN)
        .map(display_name)
        .collect();
    if item.assignees.len() > ASSIGNEES_SHOWN {
        names.push("…");
    }
    format!("{} (a)", names.join(", "))
}

fn labels_text(item: &GhItem) -> String {
    item.labels
        .iter()
        .map(|label| label.name.as_str())
        .collect::<Vec<&str>>()
        .join(", ")
}

fn updated_text(item: &GhItem, now: i64) -> String {
    format!(
        "{} ago · {}",
        age(&item.updated_at, now),
        full_date(&item.updated_at)
    )
}

/// The kind, the short `name#number` and the link above the properties.
fn heading_of(item: &GhItem, props: Vec<Prop>) -> Detail {
    let (repo, _) = item.key.split_once('#').unwrap_or((&item.repo, ""));
    Detail {
        kind: match item.kind {
            ItemKind::Pr => "Pull request",
            ItemKind::Issue => "Issue",
        }
        .into(),
        id: format!(
            "{}#{}",
            repo.rsplit('/').next().unwrap_or(repo),
            item.number
        )
        .into(),
        title: item.title.clone().into(),
        link: Some(ServiceLink::new("GitHub ↗", item.url.clone())),
        props,
        sections: Vec::new(),
    }
}

/// The padding and corner of one mode-switch tab.
const TAB_PAD_Y_PX: f32 = 2.;
const TAB_RADIUS_PX: f32 = 4.;

/// The composer's mode switch, shown when more than one mode applies.
pub fn mode_switch(
    modes: &[Mode],
    current: Mode,
    tokens: &Tokens,
    on: impl Fn(Mode, &mut Window, &mut App) + 'static,
) -> Option<Div> {
    if modes.len() < 2 {
        return None;
    }
    let on = std::rc::Rc::new(on);
    Some(
        h_flex()
            .w_full()
            .min_w_0()
            .flex_wrap()
            .gap_1()
            .mb_2()
            .children(modes.iter().map(|&mode| {
                let on = on.clone();
                div()
                    .id(("gh-mode", mode as usize))
                    .px_2()
                    .py(px(TAB_PAD_Y_PX))
                    .rounded(px(TAB_RADIUS_PX))
                    .cursor_pointer()
                    .text_color(if mode == current {
                        tokens.fg_strong
                    } else {
                        tokens.muted
                    })
                    .when(mode == current, |el| el.bg(tokens.select))
                    .child(mode.title())
                    .on_click(move |_, window, cx| on(mode, window, cx))
            })),
    )
}

/// The title's gap below it and its size, a property row's height and its
/// label column, in the panel over the diff view.
const TITLE_GAP_PX: f32 = 14.;
const TITLE_SIZE_PX: f32 = 16.;
const PROPERTY_HEIGHT_PX: f32 = 28.;
const LABEL_WIDTH_PX: f32 = 96.;

/// The panel frame's properties, for the panel over the diff view (the
/// shared list draws its own).
pub fn props(detail: &Detail, tokens: &Tokens) -> Div {
    v_flex()
        .child(
            div()
                .mb(px(TITLE_GAP_PX))
                .text_size(px(TITLE_SIZE_PX))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.fg_strong)
                .child(detail.title.clone()),
        )
        .text_size(tokens.row_font)
        .children(detail.props.iter().map(|(label, value)| {
            h_flex()
                .min_h(px(PROPERTY_HEIGHT_PX))
                .gap_3()
                .child(
                    div()
                        .w(px(LABEL_WIDTH_PX))
                        .flex_shrink_0()
                        .text_color(tokens.muted)
                        .child(label.clone()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_color(tokens.fg_strong)
                        .child(value.clone()),
                )
        }))
}

/// "short · Name" for the assignee menu.
pub fn person_label(login: &str, name: Option<&str>) -> String {
    // The menu cuts a long label to one line.
    match name.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => format!("{} · {name}", short_login(login)),
        None => short_login(login).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use subspace_framework::client_github::model::{GhUser, ItemKind};

    use super::{detail, person_label};
    use crate::sample;

    #[test]
    fn person_labels_are_short_login_and_name() {
        assert_eq!(person_label("ann_emu", Some("Ann")), "ann · Ann");
        assert_eq!(person_label("bob", None), "bob");
        assert_eq!(person_label("bob", Some(" ")), "bob");
    }

    #[test]
    fn pull_request_properties_read_state_files_branch_and_assignees() {
        let mut item = sample::items()
            .into_iter()
            .find(|item| item.kind == ItemKind::Pr)
            .expect("the sample holds a pull request");
        item.assignees = (0..5)
            .map(|index| GhUser {
                login: format!("user{index}"),
                name: None,
            })
            .collect();
        let detail = detail(&item, None, i64::MAX / 2);
        let value = |name: &str| {
            detail
                .props
                .iter()
                .find(|(label, _)| label.as_ref() == name)
                .map(|(_, value)| value.to_string())
        };

        assert_eq!(detail.kind.as_ref(), "Pull request");
        assert_eq!(value("State").as_deref(), Some("Open"));
        assert!(value("Files").is_some_and(|files| files.starts_with("Show diff · +")));
        assert!(value("Branch").is_some_and(|branch| branch.contains('→')));
        assert!(
            value("Assignees").is_some_and(|names| names.contains("…")),
            "a fifth assignee collapses"
        );
    }

    #[test]
    fn issue_properties_drop_the_pull_request_rows() {
        let item = sample::items()
            .into_iter()
            .find(|item| item.kind == ItemKind::Issue && !item.labels.is_empty())
            .expect("the sample holds a labelled issue");
        let detail = detail(&item, None, i64::MAX / 2);
        let labels: Vec<&str> = detail
            .props
            .iter()
            .map(|(label, _)| label.as_ref())
            .collect();

        assert_eq!(detail.kind.as_ref(), "Issue");
        assert!(!labels.contains(&"Files"), "{labels:?}");
        assert!(!labels.contains(&"Branch"), "{labels:?}");
        assert!(labels.contains(&"Labels"), "{labels:?}");
    }
}
