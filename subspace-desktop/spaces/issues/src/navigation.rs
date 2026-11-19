//! Issue pull request actions route only through the declared contract.

use gpui_kit::{Context, WeakEntity};
use subspace_api_client::hub::IssueSnapshot;
use subspace_framework::dashboard::{DashboardEvent, PaletteCommand, Toast, contracts, links};

use crate::Issues;

pub(crate) fn open_pull_request(url: &str, cx: &mut Context<Issues>) {
    match contracts::github::pull_request_navigation(url) {
        Some(navigation)
            if contracts::navigation_target(crate::ID, navigation.responder, cx).is_some() =>
        {
            cx.emit(DashboardEvent::Navigate(navigation));
        }
        Some(_) => fallback(url, "PRs is unavailable. Opening in browser.", cx),
        None => fallback(url, "This link cannot open in PRs. Opening in browser.", cx),
    }
}

fn fallback(url: &str, message: &str, cx: &mut Context<Issues>) {
    let toast = if links::open(url, cx) {
        Toast::info(message.to_owned())
    } else {
        Toast::error("This link cannot be opened.")
    };
    cx.emit(DashboardEvent::Toast(toast));
}

pub(crate) fn commands(issue: &IssueSnapshot, weak: &WeakEntity<Issues>) -> Vec<PaletteCommand> {
    issue
        .pull_requests()
        .into_iter()
        .flat_map(|page| &page.items)
        .flat_map(|pr| {
            let reference = format!("{}#{}", pr.repository.full_name, pr.number);
            let url = pr.url.clone();
            let browser_url = url.clone();
            let weak = weak.clone();
            [
                PaletteCommand::callback(format!("Open in PRs: {reference}"), move |_, cx| {
                    _ = weak.update(cx, |_, cx| open_pull_request(&url, cx));
                }),
                PaletteCommand::callback(format!("Open on GitHub: {reference}"), move |_, cx| {
                    links::open(&browser_url, cx);
                }),
            ]
        })
        .collect()
}
