use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

const MAX_PATH_BYTES: usize = 1024;
const MAX_COMPONENT_BYTES: usize = 255;
const MAX_DEPTH: usize = 32;
const DEVICE_NAME_BYTES: usize = 4;

pub(super) fn text(path: &Path) -> Result<String, String> {
    path.iter()
        .map(|component| {
            component
                .to_str()
                .ok_or_else(|| "Package filename is not portable text.".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|components| components.join("/"))
}

/// Use the portable ASCII filename subset. This also avoids Unicode normalization
/// collisions between macOS, Windows and Linux without platform-dependent checks.
pub(super) fn relative(text: &str) -> Result<PathBuf, String> {
    if text.len() > MAX_PATH_BYTES || text.is_empty() || text.starts_with('/') {
        return Err("Select a non-empty relative board path of at most 1024 bytes.".into());
    }
    for part in text.split('/') {
        check_component(part)?;
    }
    if text.split('/').count() > MAX_DEPTH {
        return Err("The package exceeds 32 directory levels.".into());
    }
    Ok(PathBuf::from(text))
}

fn check_component(part: &str) -> Result<(), String> {
    require_component(portable_component(part), device_component(part))
}

fn device_component(part: &str) -> bool {
    let stem = part
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == DEVICE_NAME_BYTES
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[DEVICE_NAME_BYTES - 1].is_ascii_digit())
}

fn portable_component(part: &str) -> bool {
    !matches!(part, "" | "." | "..")
        && !part.eq_ignore_ascii_case(".git")
        && !part.ends_with(['.', ' '])
        && part.len() <= MAX_COMPONENT_BYTES
        && part
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b" -_.@()+,[]{}".contains(&byte))
}

fn require_component(portable: bool, device: bool) -> Result<(), String> {
    if !portable || device {
        return Err("The package contains an unsafe or unsupported filename. Use portable ASCII names without .git or device names.".into());
    }
    Ok(())
}

#[derive(Default)]
pub(super) struct Names(BTreeMap<String, (String, bool, bool)>);

impl Names {
    pub(super) fn add(&mut self, text: &str, directory: bool) -> Result<(), String> {
        let parts: Vec<_> = text.split('/').collect();
        for end in 1..=parts.len() {
            let name = parts[..end].join("/");
            let key = name.to_ascii_lowercase();
            let explicit = end == parts.len();
            let is_dir = !explicit || directory;
            if let Some((old, old_dir, old_explicit)) = self.0.get_mut(&key) {
                if *old != name || !*old_dir || !is_dir || (explicit && *old_explicit) {
                    return Err("The package has duplicate or platform-colliding paths.".into());
                }
                *old_explicit |= explicit;
            } else {
                self.0.insert(key, (name, is_dir, explicit));
            }
        }
        Ok(())
    }
}
