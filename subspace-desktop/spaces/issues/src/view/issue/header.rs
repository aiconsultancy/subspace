use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Div, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_api_client::model::{IssueDetail, IssueProvenance};
use subspace_framework::dashboard::action::{Intent, action_button, icon_button};
use subspace_framework::dashboard::anchored::AnchoredContextMenuExt as _;
use subspace_framework::dashboard::row_menu::{self, GroupedMenu, MenuCommand, MenuGroup};
use subspace_framework::dashboard::tokens::ButtonRole;
use subspace_framework::dashboard::workspace::Toolbar;
use subspace_framework::dashboard::{Tokens, badge::badge, heading_font};
use subspace_framework::lucide::IconName;

use crate::Issues;
use crate::model::ProjectScope;

/// The title's size at the default heading size: one clear step above body
/// text, scaled as headings scale.
const ISSUE_TITLE_PX: f32 = 20.;
/// The most of an issue's title its tab shows before it is cut.
const TAB_TITLE_CHARS: usize = 40;

/// The open issue's title on its tab, after its key, cut short so one long
/// title cannot crowd the tab strip. Empty while the issue loads.
pub(super) fn tab_title(title: Option<&str>) -> String {
    let Some(title) = title.map(str::trim).filter(|title| !title.is_empty()) else {
        return String::new();
    };
    let mut chars = title.chars();
    let shown: String = chars.by_ref().take(TAB_TITLE_CHARS).collect();
    if chars.next().is_some() {
        format!("{}…", shown.trim_end())
    } else {
        shown
    }
}

/// An issue's identifier and title in one line, the title cut as on its
/// tab, such as the parent's step in the breadcrumb.
pub(super) fn tab_label(identifier: &str, title: Option<&str>) -> String {
    let title = tab_title(title);
    if title.is_empty() {
        identifier.to_owned()
    } else {
        format!("{identifier} · {title}")
    }
}

/// The page's toolbar, as its path bar was: Back and the path on the
/// leading side, Edit issue… and More on the trailing side. The dock
/// switches are the status bar's (`Dashboard::docks`).
pub(super) fn toolbar(
    issues: &Issues,
    detail: Option<&IssueDetail>,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Toolbar {
    // An icon button; its accessible name and tooltip name the destination.
    let back = icon_button(
        "issue-back",
        Intent::Quiet,
        IconName::ArrowLeft,
        issues.back.label(),
    )
    .on_click(cx.listener(|this, _, window, cx| {
        this.go_back(window, cx);
    }));
    Toolbar::new("issue-toolbar")
        .leading(back)
        .when_some(detail, |toolbar, detail| {
            toolbar.leading(path_segments(issues, detail, tokens, cx))
        })
        .when(issues.can_edit_issue(), |toolbar| {
            toolbar.trailing(crate::editing::view::edit_button(cx))
        })
        .trailing(super::more::render(issues, cx))
}

/// The breadcrumb: the project, the parent tracker when there is one, then
/// the open issue's identifier in strong text. The project and the parent
/// open; muted chevrons separate the steps. Its links take the toolbar's
/// text size, as the toolbar's other controls do.
fn path_segments(
    issues: &Issues,
    detail: &IssueDetail,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> impl IntoElement {
    let key = &detail.fields.project_key;
    let project = issues
        .projects
        .iter()
        .find(|project| &project.key == key)
        .map_or_else(
            || key.clone(),
            |project| format!("{key} · {}", project.name),
        );
    h_flex()
        .id("issue-path")
        .aria_label("Breadcrumb")
        .gap_1()
        .min_w_0()
        .flex_wrap()
        .text_color(tokens.muted)
        .text_size(tokens.secondary_size())
        .child(
            action_button(
                "issue-project",
                Intent::Quiet,
                Some(IconName::Folder),
                project,
            )
            .role(ButtonRole::Toolbar)
            .on_click(cx.listener({
                let key = key.clone();
                move |this, _, window, cx| {
                    if this.inline_navigation_blocked(cx) {
                        return;
                    }
                    this.close_detail(cx);
                    this.set_project(ProjectScope::Project(key.clone()), cx);
                    this.focus_list(window, cx);
                }
            })),
        )
        .when_some(detail.parent.as_ref(), |path, parent| {
            let identifier = parent.identifier.clone();
            path.child(
                action_button(
                    "issue-parent",
                    Intent::Quiet,
                    Some(IconName::ChevronRight),
                    tab_label(&identifier, Some(&parent.title)),
                )
                .role(ButtonRole::Toolbar)
                .accessibility_label(format!("Open parent {identifier}"))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.focus_list(window, cx);
                    this.open_detail(identifier.clone(), cx);
                })),
            )
        })
        .child(current_step(&detail.fields.identifier, tokens))
}

/// The breadcrumb's last step: the open issue, not a link.
fn current_step(identifier: &str, tokens: &Tokens) -> Div {
    h_flex()
        .gap_1()
        .child(
            Icon::from(IconName::ChevronRight)
                .xsmall()
                .text_color(tokens.muted),
        )
        .child(
            div()
                .id("issue-path-identifier")
                .aria_label(identifier.to_owned())
                .test_support()
                .font_weight(FontWeight::MEDIUM)
                .text_color(tokens.fg_strong)
                .child(identifier.to_owned()),
        )
}

/// The title, one step above body text, with the Tracker and Archived marks
/// where they apply, then the meta line. The state shows once, in the
/// workflow and the inspector's State row, not as a badge here.
pub(super) fn render(detail: &IssueDetail, tokens: &Tokens, cx: &mut Context<Issues>) -> Div {
    let fields = &detail.fields;
    let tracker = fields.kind == "tracker";
    let archived = fields.archived_at.is_some();
    v_flex()
        .gap_1p5()
        .child(
            h_flex()
                .min_w_0()
                .gap_3()
                .flex_wrap()
                .child(
                    div()
                        .id("issue-title")
                        .aria_label(fields.title.clone())
                        .test_support()
                        .min_w_0()
                        .max_w_full()
                        .font_family(heading_font(cx))
                        .text_size(tokens.heading_size(ISSUE_TITLE_PX))
                        .font_weight(FontWeight::BOLD)
                        .text_color(tokens.fg_strong)
                        .child(subspace_framework::dashboard::rich::plain_text(
                            "issue-title-text",
                            &fields.title,
                        )),
                )
                .when(tracker || archived, |row| {
                    row.child(
                        h_flex()
                            .flex_shrink_0()
                            .gap_2()
                            .when(tracker, |marks| {
                                marks.child(badge("Tracker", tokens.surface, tokens.fg))
                            })
                            .when(archived, |marks| {
                                marks.child(badge("Archived", tokens.surface, tokens.muted))
                            }),
                    )
                }),
        )
        .child(metadata(detail, tokens))
}

fn metadata(detail: &IssueDetail, tokens: &Tokens) -> Div {
    let fields = &detail.fields;
    v_flex()
        .gap_1()
        .text_color(tokens.muted)
        .text_size(tokens.secondary_size())
        .child(
            h_flex()
                .gap_3()
                .flex_wrap()
                .items_start()
                .child(
                    h_flex()
                        .gap_2()
                        .child(copyable_identifier(&fields.identifier))
                        .when_some(detail.created_by.as_ref(), |row, person| {
                            let verb = if fields.kind == "tracker" {
                                "Added by"
                            } else {
                                "Opened by"
                            };
                            row.child(subspace_framework::dashboard::rich::plain_text(
                                "issue-opened-by",
                                &format!("· {verb} {}", person.attribution()),
                            ))
                        }),
                )
                .children(super::header_progress::render(detail, tokens)),
        )
        .when_some(detail.fields.provenance.as_ref(), |column, provenance| {
            column.child(subspace_framework::dashboard::rich::plain_text(
                "issue-provenance",
                &provenance_line(provenance),
            ))
        })
}

fn copyable_identifier(value: &str) -> AnyElement {
    let value: SharedString = value.to_owned().into();
    div()
        .id("issue-identifier")
        .aria_label(value.clone())
        .test_support()
        .child(
            subspace_framework::dashboard::rich::plain_text("issue-identifier-text", &value)
                .menu(false),
        )
        .anchored_context_menu(move |menu, window, cx| {
            let value = value.clone();
            let copy = MenuCommand::new(
                MenuGroup::Copy,
                "Copy issue number",
                IconName::Copy,
                move |_, cx| subspace_framework::dashboard::links::copy(value.to_string(), cx),
            );
            row_menu::build(menu, GroupedMenu::new().with(copy), window, cx)
        })
        .into_any_element()
}

fn provenance_line(source: &IssueProvenance) -> SharedString {
    let system = if source.source_system == "linear" {
        "Linear"
    } else {
        &source.source_system
    };
    let mut text = format!("Imported from {system} · {}", source.original_identifier);
    if let Some(author) = &source.original_creator_name {
        text.push_str(" · originally by ");
        text.push_str(author);
    }
    text.into()
}

#[cfg(test)]
mod tests {
    use super::{tab_label, tab_title};

    #[test]
    fn the_issue_tab_names_the_issue_and_cuts_a_long_title() {
        assert_eq!(
            tab_title(Some("Course correction preview")),
            "Course correction preview"
        );
        assert_eq!(tab_title(None), "", "while it loads");
        assert_eq!(
            tab_label("SUB-12", Some("Course correction preview")),
            "SUB-12 · Course correction preview"
        );
        assert_eq!(tab_label("SUB-12", None), "SUB-12", "while it loads");
        assert_eq!(tab_label("SUB-12", Some("  ")), "SUB-12");
        let long = "Day 1 blocker: confirmed sample booking refuses timing and fee amendments";
        assert_eq!(
            tab_label("SUB-1540", Some(long)),
            "SUB-1540 · Day 1 blocker: confirmed sample booking…"
        );
    }
}
