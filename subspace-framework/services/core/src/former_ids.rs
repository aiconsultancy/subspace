//! Saved data under a Space's former ids. A product that renames a Space
//! lists the old id in the Space's `former_ids`; whatever was saved under it
//! is read as the Space's own. The rename happens in memory, on what was just
//! read, each time: the config parser, the Settings editor and the state
//! loader apply it, and the per-Space settings file is looked up under a
//! former id when the Space has none of its own. Nothing is written for it:
//! the files lose their old keys only when the app next saves them.
//!
//! Where both ids hold a value, the Space's own wins and the old one is
//! dropped from memory. The loaders read the former ids the start-up funnel
//! [`install`]s, once, before the first load. Anything parsed before then
//! sees no former ids.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde_json::{Map, Value};
use subspace_product::ValidatedProduct;

/// Former Space id to the Space's id.
pub type Renames = BTreeMap<String, String>;

static INSTALLED: RwLock<Renames> = RwLock::new(Renames::new());

/// The former ids `product` declares, each with its Space's id.
pub fn declared(product: &ValidatedProduct) -> Renames {
    product
        .boards
        .iter()
        .flat_map(|board| {
            board
                .former_ids
                .iter()
                .map(move |former| (former.as_str().to_owned(), board.id.as_str().to_owned()))
        })
        .collect()
}

/// Makes every later config, Settings and state read in this process map
/// `renames`. The start-up funnel calls it once, before the first load.
pub fn install(renames: Renames) {
    if let Ok(mut installed) = INSTALLED.write() {
        *installed = renames;
    }
}

/// The installed former ids; none before [`install`].
pub(crate) fn installed() -> Renames {
    INSTALLED
        .read()
        .map(|renames| renames.clone())
        .unwrap_or_default()
}

/// How a saved key names a Space.
#[derive(Clone, Copy)]
enum Shape {
    /// The whole key is the id, such as `mcp.groups.<id>`.
    Whole,
    /// The whole key, without surrounding white space, is the id, as the
    /// parser reads `boards`.
    Trimmed,
    /// `<prefix><id>.<rest>`, such as `board.<id>.<feature>`.
    Prefixed(&'static str),
}

/// The config file's objects whose keys name Spaces, by their paths, with
/// the keys in them the Framework owns, which never move and which nothing
/// moves onto: the Application's shared MCP group is beside the boards' own.
const CONFIG_KEYS: &[(&[&str], Shape, &[&str])] = &[
    (&["boards"], Shape::Trimmed, &[]),
    (&["features"], Shape::Prefixed("board."), &[]),
    (
        &["mcp", "groups"],
        Shape::Whole,
        &[subspace_product::MCP_APPLICATION_GROUP],
    ),
    (&["commands", "aliases"], Shape::Whole, &[]),
];

/// The state file's key shapes, in the order they are tried. The namespaces
/// besides a Space's own (`board`, `notifications`, and `app` for
/// [`ACTIVE_SPACE`]) are among [`subspace_product::SAVED_NAMES`], which no
/// former id may be.
const STATE_KEYS: &[Shape] = &[
    Shape::Prefixed(""),
    Shape::Prefixed("board."),
    Shape::Prefixed("notifications."),
];

/// The state key that holds the active Space's id.
const ACTIVE_SPACE: &str = "app.dashboard";

/// State keys the Framework owns inside a namespace a Space also uses. They
/// never move, and nothing moves onto them.
const FRAMEWORK_KEYS: &[&str] = &[crate::credentials::GITHUB_ACCOUNT_KEY];

/// `key` as `shape` compares it.
fn normal(key: &str, shape: Shape) -> &str {
    match shape {
        Shape::Trimmed => key.trim(),
        Shape::Whole | Shape::Prefixed(_) => key,
    }
}

/// `key` under the Space's id when its shape names a former id.
fn renamed(key: &str, renames: &Renames, shape: Shape) -> Option<String> {
    match shape {
        Shape::Whole | Shape::Trimmed => renames.get(normal(key, shape)).cloned(),
        Shape::Prefixed(prefix) => {
            let (id, rest) = key.strip_prefix(prefix)?.split_once('.')?;
            renames.get(id).map(|to| format!("{prefix}{to}.{rest}"))
        }
    }
}

/// Each of `keys` that names a former id, with the key it moves to.
fn moves<'a>(
    keys: impl IntoIterator<Item = &'a String>,
    renames: &Renames,
    shapes: &[Shape],
) -> Vec<(String, String)> {
    keys.into_iter()
        .filter_map(|key| {
            let to = shapes
                .iter()
                .find_map(|shape| renamed(key, renames, *shape))?;
            Some((key.clone(), to))
        })
        .collect()
}

/// `moves` without those from or onto one of the `owned` keys.
fn unowned(moves: Vec<(String, String)>, owned: &[&str]) -> Vec<(String, String)> {
    let is_owned = |key: &str| owned.contains(&key);
    moves
        .into_iter()
        .filter(|(from, to)| !is_owned(from) && !is_owned(to))
        .collect()
}

/// The state file's keys that name a former id, with their new keys. The
/// Framework's own keys stay where they are.
pub(crate) fn state_moves<'a>(
    keys: impl IntoIterator<Item = &'a String>,
    renames: &Renames,
) -> Vec<(String, String)> {
    unowned(moves(keys, renames, STATE_KEYS), FRAMEWORK_KEYS)
}

/// The Space id the state file's `key` should hold instead of `value`.
pub(crate) fn renamed_value(key: &str, value: &str, renames: &Renames) -> Option<String> {
    (key == ACTIVE_SPACE)
        .then(|| renames.get(value).cloned())
        .flatten()
}

/// Moves each of `object`'s keys of `shape` that names a former id, unless a
/// key that `shape` reads as the Space's own exists, which wins. Nothing
/// moves from or onto an `owned` key. True when anything changed.
fn rename_keys(
    object: &mut Map<String, Value>,
    renames: &Renames,
    (shape, owned): (Shape, &[&str]),
) -> bool {
    let moves = unowned(moves(object.keys(), renames, &[shape]), owned);
    for (from, to) in &moves {
        if let Some(value) = object.remove(from)
            && !object.keys().any(|key| normal(key, shape) == to)
        {
            object.insert(to.clone(), value);
        }
    }
    !moves.is_empty()
}

/// The object at `path` under `root`, when every step is an object.
fn object_at<'a>(
    root: &'a mut Map<String, Value>,
    path: &[&str],
) -> Option<&'a mut Map<String, Value>> {
    path.iter()
        .try_fold(root, |object, key| object.get_mut(*key)?.as_object_mut())
}

/// Renames `boardOrder`'s former ids. An entry whose Space is already
/// listed is dropped, so no Space is listed twice.
fn rename_order(root: &mut Map<String, Value>, renames: &Renames) -> bool {
    let Some(Value::Array(order)) = root.get_mut("boardOrder") else {
        return false;
    };
    let listed: BTreeSet<String> = order
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    let mut added = BTreeSet::new();
    let renamed: Vec<Value> = order
        .iter()
        .filter_map(
            |entry| match entry.as_str().and_then(|id| renames.get(id)) {
                Some(to) => (!listed.contains(to) && added.insert(to.clone()))
                    .then(|| Value::String(to.clone())),
                None => Some(entry.clone()),
            },
        )
        .collect();
    let changed = renamed != *order;
    *order = renamed;
    changed
}

/// Renames the former ids in the config file's `root`: `boards`,
/// `boardOrder`, `board.<id>.*` features, `mcp.groups` and
/// `commands.aliases`. True when anything changed.
pub fn rename_config(root: &mut Map<String, Value>, renames: &Renames) -> bool {
    let mut changed = rename_order(root, renames);
    for &(path, shape, owned) in CONFIG_KEYS {
        if let Some(object) = object_at(root, path) {
            changed |= rename_keys(object, renames, (shape, owned));
        }
    }
    changed
}

/// Space `id`'s settings file in `dir`: its own unless that is known not to
/// exist, then the first of its former ids' that exists, else its own. A
/// file that cannot be looked up, such as one in a folder the user cannot
/// search, stays the Space's, so its reader reports the error.
pub(crate) fn board_file(dir: &Path, id: &str, renames: &Renames) -> PathBuf {
    let file = |id: &str| dir.join(id).join("config.json");
    let own = file(id);
    if !matches!(own.try_exists(), Ok(false)) {
        return own;
    }
    renames
        .iter()
        .filter(|(_, to)| to.as_str() == id)
        .map(|(from, _)| file(from))
        .find(|former| former.exists())
        .unwrap_or(own)
}

#[cfg(test)]
mod tests;
