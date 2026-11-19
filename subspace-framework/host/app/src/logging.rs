//! Opening the product's log file at startup: `<state dir>/logs/<binary>.log`
//! at the level the viewer last stored (`app.log-level`), or Info.

use std::path::Path;

use subspace_core::log::{self, Config, Level, Logger};
use subspace_core::state::State;
use subspace_dashboard::log_view::LEVEL_KEY;
use subspace_product::ProductContext;

use crate::product::ProductFiles;

/// The level the state file holds, or Info.
pub(crate) fn stored_level(state: &State) -> Level {
    state
        .get(LEVEL_KEY)
        .and_then(Level::parse)
        .unwrap_or(Level::Info)
}

/// The log directory beside the state file.
fn directory(state_file: &Path) -> Option<std::path::PathBuf> {
    Some(state_file.parent()?.join("logs"))
}

/// Opens the log and writes the first record. A log that cannot open is
/// reported as a notice; the app runs without a file.
pub(crate) fn start(files: &ProductFiles, context: &ProductContext) -> Option<String> {
    let (binary, version) = (context.product.binary_name.as_str(), &context.version);
    let level = stored_level(&State::load(&files.state));
    let directory = directory(&files.state)?;
    let result = log::init(Config::new(directory, binary, level));
    Logger::new("app").record(
        Level::Info,
        format!("started {binary} {version}"),
        &[("level", log::level().as_str())],
    );
    crate::startup_path::record();
    result.err()
}

#[cfg(test)]
mod tests {
    use subspace_core::log::Level;
    use subspace_core::state::State;

    use super::{directory, stored_level};

    #[test]
    fn the_stored_level_defaults_to_info_and_the_log_sits_beside_the_state_file() {
        let mut state = State::default();
        assert_eq!(stored_level(&state), Level::Info);
        state.set("app.log-level", "debug");
        assert_eq!(stored_level(&state), Level::Debug);
        state.set("app.log-level", "loud");
        assert_eq!(stored_level(&state), Level::Info);
        let dir = directory(std::path::Path::new("/tmp/subspace/state.json")).unwrap();
        assert_eq!(dir, std::path::Path::new("/tmp/subspace/logs"));
    }
}
