use std::cell::Cell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    Context, Entity, IntoElement, ParentElement as _, Render, Styled as _, Subscription,
    TestAppContext, VisualTestContext, Window, div, px,
};

use super::{
    FILTER_LABEL, PickerSetup, PickerView, RepositoryPicker, RepositoryPickerEvent,
    RepositorySearch, apply, favourites, is_favourite, options, toggled, write,
};
use crate::menu::MenuAnchors;

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn toggling_adds_at_the_end_or_removes_without_regard_to_case() {
    let list = names(&["acme/web", "acme/api"]);
    assert_eq!(
        toggled(&list, "acme/tools"),
        ["acme/web", "acme/api", "acme/tools"]
    );
    assert_eq!(toggled(&list, "ACME/Web"), ["acme/api"]);
    assert!(is_favourite(&list, "Acme/API"));
    assert!(!is_favourite(&list, "acme/ap"));
}

#[test]
fn the_picker_lists_the_filter_then_favourites_then_new_results() {
    let view = PickerView {
        favourites: names(&["acme/web", "acme/api"]),
        results: Some(names(&["ACME/WEB", "acme/tools"])),
        query: String::new(),
        filter: Some(true),
    };
    let shown = options(&view);
    let rows: Vec<(&str, Option<bool>, bool, Option<&str>)> = shown
        .iter()
        .map(|option| {
            (
                option.label.as_ref(),
                option.starred,
                option.checked,
                option.section.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            (FILTER_LABEL, None, true, Some(super::FILTER_SECTION)),
            ("acme/web", Some(true), false, Some("Favourites")),
            ("acme/api", Some(true), false, Some("Favourites")),
            ("acme/tools", Some(false), false, Some("Your repositories")),
        ]
    );
    let searched = options(&PickerView {
        query: "API".into(),
        filter: None,
        ..view
    });
    let labels: Vec<&str> = searched.iter().map(|o| o.label.as_ref()).collect();
    assert_eq!(labels, ["acme/api", "acme/tools"]);
    assert_eq!(searched[1].section.as_deref(), Some("GitHub results"));
    assert_eq!(searched[0].accessible_label(), "acme/api, favourite");
}

#[cfg(unix)]
#[test]
fn writing_favourites_saves_an_owner_only_config_file() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("config.json");
    std::fs::write(&file, r#"{"repos":["acme/web"],"keep":true}"#).unwrap();
    let saved = write(&file, &names(&["acme/web", "acme/api"])).unwrap();
    assert_eq!(saved, ["acme/web", "acme/api"]);
    let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    let root: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(root["keep"], true);
    assert!(write(&file, &names(&["no slash"])).is_err());
}

fn install_services(cx: &mut gpui_kit::App, repos: &[&str]) {
    let config = subspace_core::config::Config {
        repos: names(repos),
        ..Default::default()
    };
    cx.set_global(crate::Services {
        credentials: Arc::new(subspace_core::credentials::Credentials::from_system(
            &config,
        )),
        config: Arc::new(config),
        tools: Default::default(),
        search: None,
        config_file: Default::default(),
    });
}

#[gpui_kit::test]
fn applying_favourites_updates_the_running_config_once(cx: &mut TestAppContext) {
    let changes = Rc::new(Cell::new(0));
    cx.update(|cx| {
        install_services(cx, &["acme/web"]);
        let seen = changes.clone();
        cx.observe_global::<crate::Services>(move |_| seen.set(seen.get() + 1))
            .detach();
    });
    cx.run_until_parked();
    cx.update(|cx| apply(names(&["acme/web", "acme/api"]), cx));
    cx.run_until_parked();
    cx.update(|cx| {
        assert_eq!(favourites(cx), ["acme/web", "acme/api"]);
        apply(names(&["acme/web", "acme/api"]), cx);
    });
    cx.run_until_parked();
    assert_eq!(changes.get(), 1, "an unchanged list does not notify");
}

struct Owner {
    anchors: MenuAnchors,
    picker: Option<Entity<RepositoryPicker>>,
    favourites: Vec<String>,
    filter: bool,
    events: Vec<RepositoryPickerEvent>,
    _subscription: Option<Subscription>,
}

impl Owner {
    fn open(&mut self, search: RepositorySearch, window: &mut Window, cx: &mut Context<Self>) {
        let anchor = self.anchors.anchor("repositories", cx);
        let setup = PickerSetup {
            favourites: self.favourites.clone(),
            filter: Some(self.filter),
            search,
        };
        let Some(picker) = RepositoryPicker::open(&self.anchors, anchor, setup, window, cx) else {
            return;
        };
        self._subscription = Some(cx.subscribe(&picker, |owner, picker, event, cx| {
            owner.events.push(event.clone());
            match event {
                RepositoryPickerEvent::Toggle(repo) => {
                    owner.favourites = toggled(&owner.favourites, repo);
                }
                RepositoryPickerEvent::ToggleFilter => owner.filter = !owner.filter,
                RepositoryPickerEvent::Closed => return,
            }
            let (favourites, filter) = (owner.favourites.clone(), Some(owner.filter));
            picker.update(cx, |picker, cx| {
                picker.set_favourites(favourites, filter, cx)
            });
        }));
        self.picker = Some(picker);
    }
}

impl Render for Owner {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let trigger = self.anchors.trigger(
            "repositories",
            gpui_kit::component::button::Button::new("repositories-button").label("Repositories"),
            cx,
        );
        div()
            .size_full()
            .child(div().absolute().left(px(40.)).top(px(40.)).child(trigger))
            .children(self.anchors.view())
    }
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }
}

fn search_log() -> (RepositorySearch, Arc<Mutex<Vec<String>>>) {
    let log = Arc::new(Mutex::new(Vec::new()));
    let seen = log.clone();
    let search: RepositorySearch = Arc::new(move |text: &str| {
        seen.lock().unwrap().push(text.to_owned());
        Ok(match text {
            "" => names(&["acme/web", "acme/api"]),
            _ => names(&["acme/tools", "acme/web"]),
        })
    });
    (search, log)
}

/// The picker's option labels, with a star after each favourite.
fn labels(owner: &Entity<Owner>, cx: &mut VisualTestContext) -> Vec<String> {
    owner.read_with(cx, |owner, cx| {
        let view = owner.picker.as_ref().unwrap().read(cx).view().clone();
        options(&view)
            .into_iter()
            .map(|option| match option.starred {
                Some(true) => format!("{} ★", option.label),
                _ => option.label.to_string(),
            })
            .collect()
    })
}

#[gpui_kit::test]
fn search_star_and_unstar_keep_the_picker_open(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::menu::bind_keys(cx);
    });
    let (owner, cx) = cx.add_window_view(|_, _| Owner {
        anchors: MenuAnchors::default(),
        picker: None,
        favourites: names(&["acme/web"]),
        filter: false,
        events: Vec::new(),
        _subscription: None,
    });
    let (search, log) = search_log();
    owner.update_in(cx, |owner, window, cx| owner.open(search, window, cx));
    settle(cx);
    assert_eq!(labels(&owner, cx), [FILTER_LABEL, "acme/web ★", "acme/api"]);
    cx.simulate_keystrokes("t o");
    cx.executor().advance_clock(Duration::from_millis(300));
    settle(cx);
    assert_eq!(*log.lock().unwrap(), ["", "to"]);
    assert_eq!(labels(&owner, cx), [FILTER_LABEL, "acme/tools"]);
    cx.simulate_keystrokes("down enter");
    settle(cx);
    owner.read_with(cx, |owner, _| {
        assert_eq!(owner.favourites, ["acme/web", "acme/tools"]);
        assert_eq!(
            owner.events,
            [RepositoryPickerEvent::Toggle("acme/tools".into())]
        );
    });
    assert_eq!(labels(&owner, cx), [FILTER_LABEL, "acme/tools ★"]);
    cx.update(|window, _| assert!(window.try_find("menu").is_some(), "the picker stays open"));
    cx.simulate_keystrokes("up enter");
    settle(cx);
    owner.read_with(cx, |owner, _| assert!(owner.filter));
    cx.simulate_keystrokes("escape");
    settle(cx);
    owner.read_with(cx, |owner, _| {
        assert_eq!(owner.events.last(), Some(&RepositoryPickerEvent::Closed));
    });
    cx.update(|window, _| assert!(window.try_find("menu").is_none()));
}
