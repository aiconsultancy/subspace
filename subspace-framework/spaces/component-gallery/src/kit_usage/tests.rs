//! The usage scan against the real source, and its parser on fixtures.

use super::{
    ROOTS, component_paths, first_use, kit_items, path_uses, production_text, project_root, sources,
};

#[test]
fn the_used_flags_match_the_source_outside_the_gallery() {
    let sources = sources();
    assert!(sources.len() > ROOTS.len(), "no Subspace source was found");
    let root = project_root();
    let wrong: Vec<String> = kit_items()
        .filter_map(|(item, kit)| {
            let found = first_use(kit, &sources);
            (found.is_some() != kit.used).then(|| match found {
                Some(path) => format!(
                    "{} is used in {}; set used: true",
                    item.name,
                    path.strip_prefix(&root).unwrap_or(path).display()
                ),
                None => format!(
                    "{} is not used outside the gallery; set used: false",
                    item.name
                ),
            })
        })
        .collect();
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn every_kit_item_names_its_paths() {
    for (item, kit) in kit_items() {
        assert!(item.name.starts_with("gpui_kit::"), "{}", item.name);
        assert!(!kit.paths.is_empty(), "{} has no paths", item.name);
    }
}

fn joined(paths: Vec<Vec<String>>) -> Vec<String> {
    paths.into_iter().map(|path| path.join("::")).collect()
}

#[test]
fn use_trees_expand_braces_self_globs_and_aliases() {
    let text = "use gpui_kit::component::{button::{self, Button as KitButton}, h_flex,\n    menu::*};\nuse gpui_kit::{App, component::tab::Tab};\nlet s = gpui_kit::component::spinner::Spinner::new();\nuse gpui_kit::base::{h_resizable, Link};";
    assert_eq!(
        joined(component_paths(text)),
        [
            "button",
            "button::Button",
            "h_flex",
            "menu::*",
            "tab::Tab",
            "spinner::Spinner::new",
            "h_resizable",
            "Link"
        ]
    );
}

#[test]
fn a_path_uses_a_component_by_name_glob_or_module() {
    let path = |text: &str| -> Vec<String> { text.split("::").map(str::to_owned).collect() };
    let needle = ["button", "Button"];
    assert!(path_uses(&path("button::Button"), &needle, ""));
    assert!(path_uses(&path("button::Button::new"), &needle, ""));
    assert!(path_uses(&path("button::*"), &needle, ""));
    assert!(path_uses(
        &path("button"),
        &needle,
        "button::Button::new(\"a\")"
    ));
    assert!(!path_uses(
        &path("button"),
        &needle,
        "button::Toggle::new(\"a\")"
    ));
    assert!(!path_uses(&path("button::Toggle"), &needle, ""));
}

#[test]
fn comments_and_inline_tests_do_not_count() {
    let text = "// gpui_kit::component::tag::Tag\n#[cfg(test)]\nmod tests;\nfn a() {}\n#[cfg(test)]\nmod tests { fn b() { use gpui_kit::component::tag::Tag; } }\nuse gpui_kit::component::kbd::Kbd;";
    assert_eq!(
        joined(component_paths(&production_text(text))),
        ["kbd::Kbd"]
    );
}
