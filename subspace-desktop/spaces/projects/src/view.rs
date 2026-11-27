//! The Projects workspace with shared navigation and project header.

mod actions;
mod linked_issues;
mod milestones;
mod overview;
#[cfg(test)]
mod tests;
mod timeline;
mod toolbar;

use chrono::NaiveDate;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div, px,
};
use subspace_api_client::initiatives::{NO_MEMBERSHIPS, not_a_member};
use subspace_framework::dashboard::{
    Tokens,
    action::{Intent, action_button},
    feedback, rich,
    row_menu::MenuChat,
    workspace,
};
use subspace_framework::lucide::IconName;

use crate::{Projects, Record, Tab, editor};

impl Render for Projects {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.follow_account(cx);
        self.restore_milestone_scroll();
        let tokens = Tokens::current(cx);
        let actions = actions::render(self, cx);
        let pane = workspace::Pane::new("projects-pane")
            .toolbar(toolbar::render(self, cx).trailing(actions))
            .child(content(self, &tokens, cx));
        let workspace = workspace::Workspace::new("projects-workspace")
            .navigator(navigator(self, cx))
            .pane(pane);
        v_flex()
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .bg(tokens.bg)
            .text_color(tokens.fg)
            .child(workspace)
            .children(self.menu_anchors.view())
            .children(editor::render(self, &tokens, cx))
    }
}

fn navigator(this: &Projects, cx: &Context<Projects>) -> workspace::Navigator {
    workspace::Navigator::new("Projects").children(this.visible_projects().map(|project| {
        let id = project.id.clone();
        let selected = this.selected.as_ref() == Some(&id);
        let row = workspace::NavRow::new(format!("project:{id}"), project.name.clone())
            .icon(IconName::Folder)
            .selected(selected)
            .quiet(project.archived)
            .on_click(cx.listener(move |this, _, _, cx| this.select(&id, cx)));
        match &project.key {
            Some(key) => row.count(key.clone()),
            None => row,
        }
    }))
}

fn content(this: &Projects, tokens: &Tokens, cx: &mut Context<Projects>) -> AnyElement {
    let body = match &this.record {
        Some(Ok(record)) if this.in_scope(&record.project) => record_body(this, record, tokens, cx),
        Some(Err(error)) => failure(&error.to_string(), tokens, cx),
        Some(Ok(_)) => div().into_any_element(),
        None if this.directory_task.is_some()
            || this.record_task.is_some()
            || this.initiatives.read(cx).reading() =>
        {
            feedback::loading_line(tokens, "projects-loading", "Loading projects…")
                .into_any_element()
        }
        None => match &this.projects {
            Err(error) => failure(&error.to_string(), tokens, cx),
            Ok(_) if this.initiative_hidden => {
                let catalog = this.initiatives.read(cx);
                if not_a_member(subspace_api_client::capabilities(), catalog) {
                    div()
                        .text_color(tokens.muted)
                        .child(NO_MEMBERSHIPS)
                        .into_any_element()
                } else {
                    failure("Initiatives are unavailable.", tokens, cx)
                }
            }
            Ok(_) => div()
                .text_color(tokens.muted)
                .child("No projects in this initiative.")
                .into_any_element(),
        },
    };
    v_flex()
        .id(match (this.milestone_pages.detail.is_some(), this.tab) {
            (true, _) => "project-milestone-detail-scroll",
            (_, Tab::Overview) => "project-overview-scroll",
            (_, Tab::Milestones) => "project-milestones-scroll",
        })
        .size_full()
        .min_h_0()
        .overflow_y_scroll()
        .track_scroll(this.milestone_pages.scroll(this.tab))
        .child(
            h_flex().w_full().justify_center().child(
                v_flex()
                    .id("projects-record-column")
                    .test_support()
                    .w_full()
                    .min_w_0()
                    .max_w(px(workspace::READING_WIDTH_PX))
                    .px(workspace::text_edge(tokens))
                    .py_5()
                    .gap_4()
                    .child(body),
            ),
        )
        .into_any_element()
}

fn failure(message: &str, tokens: &Tokens, cx: &Context<Projects>) -> AnyElement {
    v_flex()
        .gap_2()
        .child(div().text_color(tokens.muted).child(message.to_owned()))
        .child(
            h_flex().child(
                action_button(
                    "projects-retry",
                    Intent::Neutral,
                    Some(IconName::RotateCw),
                    "Reload",
                )
                .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
            ),
        )
        .into_any_element()
}

fn record_body(
    this: &Projects,
    record: &Record,
    tokens: &Tokens,
    cx: &Context<Projects>,
) -> AnyElement {
    let project = &record.project;
    if let Some(detail) = &this.milestone_pages.detail {
        return milestones::detail(this, &detail.milestone, &project.name, tokens, cx);
    }
    let initiative_name = project
        .initiative
        .as_ref()
        .map(|item| item.name.clone())
        .or_else(|| {
            let membership = this.initiatives.read(cx).selected()?;
            (project.initiative_id.as_ref() == Some(&membership.id))
                .then(|| membership.name.clone())
        });
    v_flex()
        .gap_5()
        .child(
            v_flex()
                .gap_1()
                .child(
                    div()
                        .text_xl()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(tokens.fg_strong)
                        .child(project.name.clone()),
                )
                .children(initiative_name.map(|name| div().text_color(tokens.muted).child(name))),
        )
        .child(overview::facts(project, tokens))
        .child(overview::stages(project, tokens))
        .child(match this.tab {
            Tab::Overview => overview::render(this, record, tokens, cx),
            Tab::Milestones => milestones::render(this, record, tokens, cx),
        })
        .into_any_element()
}

fn prose(id: &str, value: &str) -> AnyElement {
    div()
        .w_full()
        .max_w(px(workspace::READING_WIDTH_PX))
        .child(rich::rich_text(id.to_owned(), rich::Format::Markdown, value).chat(MenuChat::Off))
        .into_any_element()
}

fn date_label(value: Option<&str>) -> String {
    value.map_or_else(
        || "—".into(),
        |date| {
            NaiveDate::parse_from_str(date, "%Y-%m-%d").map_or_else(
                |_| date.to_owned(),
                |day| day.format("%-d %b %Y").to_string(),
            )
        },
    )
}
