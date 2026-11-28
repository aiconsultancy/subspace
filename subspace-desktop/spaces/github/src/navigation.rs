//! Contract navigation uses the normal list and detail reader. One transient
//! row keeps an out-of-list target visible without changing section results.

use gpui_kit::Context;
use subspace_framework::client_github::model::{GhItemDetail, ItemKind};
use subspace_framework::contracts::github::recognition::PullRequestLink;
use subspace_framework::core::http::{Error, status};
use subspace_framework::dashboard::list::{Detail, Section, ServiceLink};
use subspace_framework::dashboard::{DashboardEvent, Glyph, Group, ReturnPoint, Row, Tone};

use crate::{Github, model, rows};

pub(super) struct Target {
    link: PullRequestLink,
    key: String,
    pub(super) origin: Option<ReturnPoint>,
}

/// The PRs Space shows only the pull request it asked for; any other detail
/// reads as not found.
pub(crate) fn requested_pr<T>(
    (detail, cached): (GhItemDetail, T),
    key: &str,
) -> Result<(GhItemDetail, T), Error> {
    if detail.item.kind != ItemKind::Pr || !detail.item.key.eq_ignore_ascii_case(key) {
        return Err(Error::Status {
            status: status::NOT_FOUND,
            message: "Pull request not found.".into(),
        });
    }
    Ok((detail, cached))
}

pub(super) fn open(board: &mut Github, url: &str, cx: &mut Context<Github>) -> bool {
    let origin = board.arrival.take();
    let Some(link) = PullRequestLink::parse(url) else {
        return false;
    };
    let key = loaded_key(board, &link.key()).unwrap_or_else(|| link.key());
    board.navigation = Some(Target { link, key, origin });
    board.composer = None;
    board.diff = None;
    board._detail_task = None;
    board.detail_key = None;
    reveal(board, cx);
    true
}

fn loaded_key(board: &Github, key: &str) -> Option<String> {
    let account = board.account_data()?;
    account
        .sections
        .values()
        .flat_map(|section| &section.items)
        .chain(account.details.values().map(|detail| &detail.item))
        .find(|item| item.kind == ItemKind::Pr && item.key.eq_ignore_ascii_case(key))
        .map(|item| item.key.clone())
}

/// Adopt GitHub's repository spelling after an out-of-list read.
pub(super) fn canonicalize(board: &mut Github, requested: &str, canonical: &str) -> bool {
    let Some(target) = board
        .navigation
        .as_mut()
        .filter(|target| target.key == requested)
    else {
        return false;
    };
    target.key = canonical.to_owned();
    true
}

pub(super) fn reveal(board: &mut Github, cx: &mut Context<Github>) {
    let Some(target) = &board.navigation else {
        return;
    };
    let key = target.key.clone();
    board.rebuild(cx);
    if board.list.reveal(&key) {
        board.ensure_detail(cx);
        cx.emit(DashboardEvent::Opened(key.into()));
    }
    cx.notify();
}

pub(super) fn selection_changed(
    board: &mut Github,
    selected: Option<&str>,
    cx: &mut Context<Github>,
) {
    if board
        .navigation
        .as_ref()
        .is_some_and(|target| Some(target.key.as_str()) != selected)
    {
        clear(board, cx);
    }
}

pub(super) fn clear(board: &mut Github, cx: &mut Context<Github>) {
    if board.navigation.take().is_some() {
        board.rebuild(cx);
    }
}

pub(super) fn append(board: &Github, groups: &mut Vec<Group>) {
    let Some(target) = &board.navigation else {
        return;
    };
    if groups
        .iter()
        .flat_map(|g| &g.rows)
        .any(|r| r.key == target.key)
    {
        return;
    }
    let mut row = match board.find_item(&target.key) {
        Some(item) => rows::row(
            item,
            board.account.as_deref().unwrap_or_default(),
            board.now,
        ),
        None => pending_row(target),
    };
    row.key = target.key.clone().into();
    let group_key = model::group_key(target.link.repo());
    if let Some(group) = groups
        .iter_mut()
        .find(|g| g.key.eq_ignore_ascii_case(&group_key))
    {
        group.rows.push(row);
        group.count = Some(group.rows.len().to_string().into());
    } else {
        groups.push(Group {
            key: group_key.into(),
            name: target.link.repo().to_owned().into(),
            icon: Glyph::new("▣", Tone::Blue),
            count: Some("1".into()),
            rows: vec![row],
        });
    }
}

fn pending_row(target: &Target) -> Row {
    Row {
        key: target.key.clone().into(),
        icon: Glyph::new("⇅", Tone::Muted),
        id: Some(format!("#{}", target.link.number()).into()),
        title: target.key.clone().into(),
        trailing: Vec::new(),
        done: false,
        indent: 0,
    }
}

pub(super) fn pending_detail(board: &Github, key: &str) -> Option<Detail> {
    let target = board
        .navigation
        .as_ref()
        .filter(|target| target.key == key)?;
    let message = board
        .account_data()
        .and_then(|account| account.detail_errors.get(key))
        .or(board.accounts_error.as_ref())
        .map_or("Loading…", String::as_str);
    Some(Detail {
        kind: "Pull request".into(),
        id: format!(
            "{}#{}",
            target.link.repo().rsplit('/').next()?,
            target.link.number()
        )
        .into(),
        title: key.to_owned().into(),
        link: Some(ServiceLink::new("GitHub ↗", target.link.url())),
        sections: vec![Section::text("Description", message)],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests;
