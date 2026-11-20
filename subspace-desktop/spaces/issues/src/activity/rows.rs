use chrono::Utc;
use gpui_kit::component::{Icon, Sizable as _, h_flex};
use gpui_kit::{
    AnyElement, App, IntoElement, ParentElement as _, SharedString, Styled as _, div, relative,
};
use subspace_api_client::hub::{ActivityListSnapshot, ActivitySection, ReadError};
use subspace_api_client::model::IssueListItem;
use subspace_framework::dashboard::dates::relative_time;
use subspace_framework::dashboard::row_menu::MenuChat;
use subspace_framework::dashboard::{Glyph, Group, Row, RowMenu, Tokens, Tone};

use crate::model::StateKind;
use crate::view::lanes;

pub(super) const SECTIONS: [ActivitySection; 2] =
    [ActivitySection::Trackers, ActivitySection::Recent];

const METADATA_WIDTH: f32 = 0.48;
const STATE_WIDTH: f32 = 0.44;

type Entries = Vec<(SharedString, Entry)>;

pub(super) enum Intent {
    Issue(String),
    Page(ActivitySection, bool),
    None,
}

pub(super) struct Entry {
    pub(super) intent: Intent,
    status: Option<SharedString>,
    state_kind: StateKind,
    summary: Option<SharedString>,
}

impl Entry {
    pub(super) fn leading(&self, cx: &App) -> Option<AnyElement> {
        self.status.as_ref()?;
        let (icon, tone) = lanes::state_icon(self.state_kind);
        Some(
            Icon::from(icon)
                .xsmall()
                .text_color(Tokens::current(cx).tone(tone))
                .into_any_element(),
        )
    }

    pub(super) fn trailing(&self, cx: &App) -> Option<AnyElement> {
        let tokens = Tokens::current(cx);
        let status = self.status.clone()?;
        Some(
            h_flex()
                .max_w(relative(METADATA_WIDTH))
                .min_w_0()
                .gap_3()
                .text_size(tokens.secondary_size())
                .text_color(tokens.muted)
                .children(
                    self.summary
                        .clone()
                        .map(|summary| div().truncate().child(summary)),
                )
                .child(
                    div()
                        .max_w(relative(STATE_WIDTH))
                        .flex_shrink_0()
                        .truncate()
                        .child(status),
                )
                .into_any_element(),
        )
    }

    pub(super) fn accessible(&self, row: &Row) -> SharedString {
        let (Some(identifier), Some(status)) = (&row.id, &self.status) else {
            return row.title.clone();
        };
        let name = format!("{identifier}, {}, {status}", row.title);
        self.summary.as_ref().map_or_else(
            || name.clone().into(),
            |summary| format!("{name}, {summary}").into(),
        )
    }
}

pub(super) fn section(
    section: ActivitySection,
    snapshot: Option<&ActivityListSnapshot>,
) -> (Group, Entries) {
    let (key, name) = match section {
        ActivitySection::Trackers => ("trackers", "Trackers in progress"),
        ActivitySection::Recent => ("recent", "Recently updated · Past 7 days"),
    };
    let page = snapshot.and_then(ActivityListSnapshot::page);
    let count = page
        .filter(|page| page.next_cursor.is_none())
        .map(|page| page.items.len().to_string().into());
    let mut records = page.map_or_else(Vec::new, |page| {
        page.items.iter().map(|item| issue_row(key, item)).collect()
    });
    if let Some(notice) = section_notice(key, section, snapshot, records.is_empty()) {
        if snapshot.is_some_and(|list| list.error().is_some()) {
            records.insert(0, notice);
        } else {
            records.push(notice);
        }
    }
    let mut group = Group {
        key: format!("activity-group:{key}").into(),
        name: name.into(),
        icon: Glyph::new("", Tone::Muted),
        count,
        rows: Vec::with_capacity(records.len()),
    };
    let mut entries = Vec::with_capacity(records.len());
    for (row, entry) in records {
        entries.push((row.key.clone(), entry));
        group.rows.push(row);
    }
    (group, entries)
}

fn issue_row(section: &str, item: &IssueListItem) -> (Row, Entry) {
    let fields = &item.fields;
    let age = relative_time(&fields.last_activity_at, Utc::now());
    let summary = crate::activity_words::summary(item.last_activity.as_ref(), &age);
    let entry = Entry {
        intent: Intent::Issue(fields.identifier.clone()),
        status: Some(fields.state.name.clone().into()),
        state_kind: StateKind::parse(&fields.state.kind),
        summary: Some(summary.into()),
    };
    let mut row = base_row(
        format!("activity:{section}:{}", fields.id),
        fields.title.clone(),
    );
    row.id = Some(fields.identifier.clone().into());
    (row, entry)
}

fn section_notice(
    key: &str,
    section: ActivitySection,
    state: Option<&ActivityListSnapshot>,
    empty: bool,
) -> Option<(Row, Entry)> {
    let network = crate::scope::network_message();
    let notice = match state {
        None => ("Loading…", Intent::None),
        Some(list) if list.loading() && empty => ("Loading…", Intent::None),
        Some(list) if list.error() == Some(&ReadError::Network) => {
            (network.as_str(), Intent::Page(section, false))
        }
        Some(list) if list.error() == Some(&ReadError::SignInNeeded) => {
            (crate::scope::SIGN_IN_MESSAGE, Intent::Page(section, false))
        }
        Some(list) if list.error().is_some() => (
            "Couldn't load this section. Retry",
            Intent::Page(section, false),
        ),
        Some(list) if list.page_loading() => ("Loading more…", Intent::None),
        Some(list) if list.page_error().is_some() => {
            ("Couldn't load more. Retry", Intent::Page(section, true))
        }
        Some(list) if list.page().is_some_and(|page| page.next_cursor.is_some()) => {
            ("Load more", Intent::Page(section, true))
        }
        Some(_) if empty => (empty_message(section), Intent::None),
        Some(_) => return None,
    };
    let mut row = base_row(format!("activity-notice:{key}"), notice.0);
    row.done = matches!(notice.1, Intent::None);
    Some((
        row,
        Entry {
            intent: notice.1,
            status: None,
            state_kind: StateKind::Other,
            summary: None,
        },
    ))
}

fn empty_message(section: ActivitySection) -> &'static str {
    match section {
        ActivitySection::Trackers => "No trackers in progress",
        ActivitySection::Recent => "No recent issue updates",
    }
}

fn base_row(key: String, title: impl Into<SharedString>) -> Row {
    Row {
        key: key.into(),
        icon: Glyph::new("", Tone::Muted),
        id: None,
        title: title.into(),
        trailing: Vec::new(),
        done: false,
        indent: 0,
    }
}

pub(super) fn row_menu(row: &Row, entry: Option<&Entry>) -> RowMenu {
    let opens_details = entry.is_some_and(|entry| matches!(entry.intent, Intent::Issue(_)));
    let menu = if opens_details {
        RowMenu::from_row(row)
    } else {
        RowMenu::new("")
    };
    RowMenu {
        chat: MenuChat::Off,
        opens_details,
        ..menu
    }
}
