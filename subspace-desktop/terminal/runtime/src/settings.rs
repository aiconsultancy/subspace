//! Configuration loading and appearance-dependent style resolution.

use std::path::PathBuf;

mod profile_font;

use subspace_framework::dashboard::Tokens;
use subspace_ghostty_config::{
    Appearance, GhosttyConfig, Paths, ThemeSearchPaths, ThemeValues, Warning, load, resolve_theme,
};

pub struct LoadedConfig {
    pub config: GhosttyConfig,
    pub profile_font: crate::style::ProfileFont,
    pub themes: [ThemeValues; 2],
    pub config_warnings: Vec<String>,
    pub theme_warnings: [Vec<String>; 2],
}

const DARK_APPEARANCE_THRESHOLD: f32 = 0.5;

/// Launch settings shared by the Terminal board and embedded sessions.
pub fn session_options(config: &GhosttyConfig) -> crate::SessionOptions {
    crate::SessionOptions {
        command: config.command.clone(),
        working_directory: config.working_directory.as_ref().map(|value| match value {
            subspace_ghostty_config::WorkingDirectory::Home => "home".to_owned(),
            subspace_ghostty_config::WorkingDirectory::Path(path) => {
                path.to_string_lossy().into_owned()
            }
        }),
        term: config.term.clone(),
        scrollback_limit_bytes: usize::try_from(config.scrollback_limit).ok(),
        environment: Vec::new(),
    }
}

/// Read configuration and both theme-pair variants once for startup or Reload.
pub fn load_config() -> LoadedConfig {
    register_platform_fonts();
    let paths = config_paths();
    let (config, parsed_warnings) = load(&paths);
    let config_warnings = parsed_warnings.iter().map(warning_text).collect::<Vec<_>>();
    let theme_paths = theme_paths(&paths);
    let (light, light_warnings) = resolve_theme(&config, Appearance::Light, &theme_paths);
    let (dark, dark_warnings) = resolve_theme(&config, Appearance::Dark, &theme_paths);
    let theme_warnings = [
        light_warnings.iter().map(warning_text).collect::<Vec<_>>(),
        dark_warnings.iter().map(warning_text).collect::<Vec<_>>(),
    ];
    LoadedConfig {
        config,
        profile_font: profile_font::load(),
        themes: [light, dark],
        config_warnings,
        theme_warnings,
    }
}

/// Make system-provided Terminal fonts available before listing or resolving them.
pub fn register_platform_fonts() {
    profile_font::register();
}

/// List installed families that give normal terminal characters equal advances.
pub fn fixed_width_families(cx: &gpui_kit::App) -> Vec<String> {
    let text = cx.text_system();
    let mut names = text.all_font_names();
    names.retain(|name| crate::font::fixed_width(text, &gpui_kit::font(name.clone())));
    names.sort_by_key(|name| name.to_lowercase());
    names
}

pub fn current_appearance(tokens: &Tokens) -> Appearance {
    if tokens.bg.l < DARK_APPEARANCE_THRESHOLD {
        Appearance::Dark
    } else {
        Appearance::Light
    }
}

// qual:allow(complexity, max_cyclomatic=11) reason: "The terminal palette must compare every token that can affect ANSI or font derivation."
pub fn tokens_changed(previous: Option<&Tokens>, current: &Tokens) -> bool {
    let Some(previous) = previous else {
        return true;
    };
    previous.bg != current.bg
        || previous.fg != current.fg
        || previous.fg_strong != current.fg_strong
        || previous.select != current.select
        || previous.red != current.red
        || previous.yellow != current.yellow
        || previous.green != current.green
        || previous.cyan != current.cyan
        || previous.blue != current.blue
        || previous.magenta != current.magenta
        || previous.mono_font != current.mono_font
}

pub fn appearance_index(appearance: Appearance) -> usize {
    match appearance {
        Appearance::Light => 0,
        Appearance::Dark => 1,
    }
}

pub fn appearance_theme(themes: &[ThemeValues; 2], appearance: Appearance) -> &ThemeValues {
    &themes[appearance_index(appearance)]
}

pub fn warnings_for(config: &[String], theme: &[String], font: Vec<String>) -> Vec<String> {
    config.iter().chain(theme).cloned().chain(font).collect()
}

pub fn replace_warnings(_previous: &[String], current: Vec<String>) -> Vec<String> {
    current
}

/// The Ghostty configuration locations. Windows has no `HOME` by default, so
/// the user profile stands in for it there: `%USERPROFILE%\.config\ghostty`.
fn config_paths() -> Paths {
    let home = std::env::var_os("HOME")
        .or_else(|| cfg!(windows).then(|| std::env::var_os("USERPROFILE"))?)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    let macos = cfg!(target_os = "macos")
        .then(|| home.join("Library/Application Support/com.mitchellh.ghostty"));
    Paths {
        xdg_config_home: xdg,
        macos_app_support: macos,
        subspace_layer: home.join(".config/subspace/ghostty"),
    }
}

fn theme_paths(paths: &Paths) -> ThemeSearchPaths {
    let resources = resources_path(
        std::env::var_os("GHOSTTY_RESOURCES_DIR").as_deref(),
        cfg!(target_os = "macos"),
        &|path| path.exists(),
    );
    ThemeSearchPaths {
        subspace_layer_themes: paths.subspace_layer.join("themes"),
        xdg_themes: paths.xdg_config_home.join("ghostty/themes"),
        resources_themes: resources.map(|path| path.join("themes")),
    }
}

fn resources_path(
    override_dir: Option<&std::ffi::OsStr>,
    macos: bool,
    exists: &dyn Fn(&std::path::Path) -> bool,
) -> Option<PathBuf> {
    if let Some(path) = override_dir {
        return Some(PathBuf::from(path));
    }
    let candidates = if macos {
        vec![PathBuf::from(
            "/Applications/Ghostty.app/Contents/Resources/ghostty",
        )]
    } else {
        vec![
            PathBuf::from("/usr/local/share/ghostty"),
            PathBuf::from("/usr/share/ghostty"),
        ]
    };
    candidates.into_iter().find(|path| exists(path))
}

fn warning_text(warning: &Warning) -> String {
    format!(
        "{}:{}: {}",
        warning.file.display(),
        warning.line,
        warning.reason
    )
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::path::{Path, PathBuf};

    use super::{replace_warnings, resources_path};

    #[test]
    fn launch_options_preserve_configured_command_directory_and_term() {
        let config = subspace_ghostty_config::GhosttyConfig {
            command: Some("/bin/sh -l".into()),
            working_directory: Some(subspace_ghostty_config::WorkingDirectory::Path(
                "/work/project".into(),
            )),
            term: Some("xterm-256color".into()),
            scrollback_limit: 12345,
            ..Default::default()
        };
        let options = super::session_options(&config);
        assert_eq!(options.command.as_deref(), Some("/bin/sh -l"));
        assert_eq!(options.working_directory.as_deref(), Some("/work/project"));
        assert_eq!(options.term, config.term);
        assert_eq!(options.scrollback_limit_bytes, Some(12345));
    }

    #[test]
    fn reload_replaces_status_warnings_instead_of_appending() {
        assert_eq!(
            replace_warnings(&["old".to_owned()], vec!["new".to_owned()]),
            vec!["new"]
        );
    }

    #[test]
    fn resources_path_uses_override_and_platform_candidates() {
        assert_eq!(
            resources_path(Some(OsStr::new("/custom/ghostty")), false, &|_| false),
            Some(PathBuf::from("/custom/ghostty"))
        );
        assert_eq!(
            resources_path(None, true, &|path| {
                path == Path::new("/Applications/Ghostty.app/Contents/Resources/ghostty")
            }),
            Some(PathBuf::from(
                "/Applications/Ghostty.app/Contents/Resources/ghostty"
            ))
        );
    }

    #[test]
    fn resources_path_picks_first_existing_linux_directory() {
        assert_eq!(
            resources_path(None, false, &|path| path == Path::new("/usr/share/ghostty")),
            Some(PathBuf::from("/usr/share/ghostty"))
        );
    }
}
