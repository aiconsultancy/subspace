//! The toolbar's two menus: the view menu, which stands in for the
//! navigator while it is hidden, and the Tag filter. Both use the shared
//! filterable menu. Their options and what a pick means are pure.

use std::collections::BTreeSet;

use gpui_kit::{AnyElement, ClickEvent, Context, SharedString, Window};
use subspace_api_client::model::Project;
use subspace_framework::dashboard::action::menu_button;
use subspace_framework::dashboard::menu::{MenuContents, MenuEvent, MenuOption};
use subspace_framework::lucide::IconName;

use crate::Decisions;
use crate::model::{Choice, Maker, ProjectChoice, View};
use crate::words;

const VIEW_ANCHOR: &str = "decisions-view";
const TAG_ANCHOR: &str = "decisions-tag";
const VIEW_PREFIX: &str = "view:";
const MAKER_PREFIX: &str = "maker:";
const PROJECT_PREFIX: &str = "projects:";
const TAG_PREFIX: &str = "tag:";
/// The Tag menu's first option, which clears the tag.
const ANY_TAG: &str = "any-tag";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    View,
    Tag,
}

/// What a menu option chooses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Pick {
    View(View),
    Maker(Maker),
    Project(ProjectChoice),
    Tag(Option<String>),
}

/// The choice an option's value stands for.
pub(crate) fn pick(value: &str) -> Option<Pick> {
    if value == ANY_TAG {
        return Some(Pick::Tag(None));
    }
    if let Some(tag) = value.strip_prefix(TAG_PREFIX) {
        return Some(Pick::Tag(Some(tag.to_owned())));
    }
    if let Some(key) = value.strip_prefix(VIEW_PREFIX) {
        return View::from_key(key).map(Pick::View);
    }
    if let Some(key) = value.strip_prefix(MAKER_PREFIX) {
        return Maker::from_key(key).map(Pick::Maker);
    }
    value
        .strip_prefix(PROJECT_PREFIX)
        .map(|key| Pick::Project(ProjectChoice::from_key(key)))
}

fn option(section: &'static str, value: String, label: String, checked: bool) -> MenuOption {
    MenuOption {
        section: Some(section.into()),
        ..MenuOption::new(value, label).checked(checked)
    }
}

/// The view menu holds everything the navigator does: the views, who made
/// the decisions and the active projects.
pub(crate) fn view_options(choice: &Choice, projects: &[Project]) -> Vec<MenuOption> {
    let views = View::ALL.into_iter().map(|view| {
        let value = format!("{VIEW_PREFIX}{}", view.key());
        option("Views", value, view.label().to_owned(), view == choice.view)
    });
    let makers = Maker::ALL.into_iter().map(|maker| {
        let value = format!("{MAKER_PREFIX}{}", maker.key());
        option(
            "Made by",
            value,
            maker.label().to_owned(),
            maker == choice.maker,
        )
    });
    let every = std::iter::once((ProjectChoice::All, "All projects".to_owned()));
    let active = projects
        .iter()
        .filter(|project| project.archived_at.is_none())
        .map(|project| {
            let label = words::project(&project.key, Some(&project.name));
            (ProjectChoice::Project(project.key.clone()), label)
        });
    let projects = every.chain(active).map(|(project, label)| {
        let value = format!("{PROJECT_PREFIX}{}", project.key());
        option("Projects", value, label, project == choice.project)
    });
    views.chain(makers).chain(projects).collect()
}

/// Any tag, then every tag the reads have shown.
pub(crate) fn tag_options(chosen: Option<&str>, tags: &BTreeSet<String>) -> Vec<MenuOption> {
    let any = MenuOption::new(ANY_TAG, "Any tag").checked(chosen.is_none());
    std::iter::once(any)
        .chain(tags.iter().map(|tag| {
            MenuOption::new(format!("{TAG_PREFIX}{tag}"), tag.clone())
                .checked(chosen == Some(tag.as_str()))
        }))
        .collect()
}

/// The view menu's control, named after the current view.
pub(crate) fn view_button(this: &Decisions, cx: &mut Context<Decisions>) -> AnyElement {
    let view = this.choice.view;
    let button = menu_button("decisions-view-button", view.icon(), view.label(), false)
        .accessibility_label(format!("View: {}", view.label()))
        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
            open(this, Kind::View, window, cx);
        }));
    this.menu_anchors.trigger(VIEW_ANCHOR, button, cx)
}

/// The Tag filter, once the reads have shown a tag or one is chosen. It
/// names the chosen tag and stands out while one is.
pub(crate) fn tag_button(this: &Decisions, cx: &mut Context<Decisions>) -> Option<AnyElement> {
    let chosen = this.choice.tag.clone();
    if chosen.is_none() && this.session.tags.is_empty() {
        return None;
    }
    let label: SharedString = chosen.clone().unwrap_or_else(|| "Tag".to_owned()).into();
    let spoken = chosen.map_or_else(|| "Tag: any".to_owned(), |tag| format!("Tag: {tag}"));
    let active = this.choice.tag.is_some();
    let button = menu_button("decisions-tag-button", IconName::Tag, label, active)
        .accessibility_label(spoken)
        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
            open(this, Kind::Tag, window, cx);
        }));
    Some(this.menu_anchors.trigger(TAG_ANCHOR, button, cx))
}

/// Opens a menu at its toolbar control; the same call closes it again.
pub(crate) fn open(
    this: &mut Decisions,
    kind: Kind,
    window: &mut Window,
    cx: &mut Context<Decisions>,
) {
    let (anchor, contents) = match kind {
        Kind::View => (
            VIEW_ANCHOR,
            MenuContents::new(
                "View",
                "Find a view",
                view_options(&this.choice, &this.session.projects),
            ),
        ),
        Kind::Tag => (
            TAG_ANCHOR,
            MenuContents::new(
                "Tag",
                "Find a tag",
                tag_options(this.choice.tag.as_deref(), &this.session.tags),
            ),
        ),
    };
    let anchor = this.menu_anchors.anchor(anchor, cx);
    let Some(menu) = this.menu_anchors.open(anchor, contents, window, cx) else {
        return;
    };
    cx.subscribe_in(&menu, window, |this, _, event: &MenuEvent, _, cx| {
        if matches!(event, MenuEvent::Query(_)) {
            return;
        }
        this.menu = None;
        if let MenuEvent::Pick(value) = event
            && let Some(choice) = pick(value)
        {
            this.apply_pick(choice, cx);
        }
        cx.notify();
    })
    .detach();
    this.menu = Some(menu);
    cx.notify();
}

impl Decisions {
    pub(crate) fn apply_pick(&mut self, pick: Pick, cx: &mut Context<Self>) {
        match pick {
            Pick::View(view) => self.choose_view(view, cx),
            Pick::Maker(maker) => self.choose_maker(maker, cx),
            Pick::Project(project) => self.choose_project(project, cx),
            Pick::Tag(tag) => self.choose_tag(tag, cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use subspace_framework::dashboard::menu::MenuOption;

    use super::{Pick, pick, tag_options, view_options};
    use crate::fixtures;
    use crate::model::{Choice, Maker, ProjectChoice, View};

    fn labels(options: &[MenuOption]) -> Vec<&str> {
        options.iter().map(|option| option.label.as_ref()).collect()
    }

    /// The values of the checked options.
    fn checked(options: &[MenuOption]) -> Vec<&str> {
        options
            .iter()
            .filter(|option| option.checked)
            .map(|option| option.value.as_ref())
            .collect()
    }

    #[test]
    fn the_view_menu_holds_what_the_navigator_does() {
        let choice = Choice {
            maker: Maker::People,
            ..Choice::default()
        };
        let options = view_options(&choice, &fixtures::projects());
        assert_eq!(
            labels(&options),
            [
                "All decisions",
                "Proposed",
                "Accepted",
                "Rejected",
                "Superseded",
                "Anyone",
                "People",
                "Agents",
                "All projects",
                "NAV · Navigation",
                "WARP · Warp drive"
            ]
        );
        assert_eq!(
            checked(&options),
            ["view:all", "maker:people", "projects:all"]
        );
        assert_eq!(options[5].section.as_deref(), Some("Made by"));
    }

    #[test]
    fn every_option_picks_its_choice() {
        assert_eq!(pick("view:rejected"), Some(Pick::View(View::Rejected)));
        assert_eq!(pick("maker:agents"), Some(Pick::Maker(Maker::Agents)));
        assert_eq!(
            pick("projects:project:NAV"),
            Some(Pick::Project(ProjectChoice::Project("NAV".into())))
        );
        assert_eq!(
            pick("projects:all"),
            Some(Pick::Project(ProjectChoice::All))
        );
        assert_eq!(
            pick("tag:telemetry"),
            Some(Pick::Tag(Some("telemetry".into())))
        );
        assert_eq!(pick("any-tag"), Some(Pick::Tag(None)));
        assert_eq!(pick("view:unknown"), None);
        assert_eq!(pick("something"), None);
    }

    #[test]
    fn the_tag_menu_offers_any_tag_then_each_tag_read() {
        let tags: BTreeSet<String> = ["warp".to_owned(), "telemetry".to_owned()].into();
        let options = tag_options(Some("warp"), &tags);
        assert_eq!(labels(&options), ["Any tag", "telemetry", "warp"]);
        assert!(!options[0].checked && options[2].checked);
        assert!(tag_options(None, &tags)[0].checked);
    }
}
