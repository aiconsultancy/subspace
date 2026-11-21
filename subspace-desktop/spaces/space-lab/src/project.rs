//! Versioned native projects. This boundary has no GPUI state or execution authority.

mod disk;
mod input;
mod model;
mod output;
mod protocol;
mod records;
mod source;
mod state;
mod store;
mod validation;

pub use input::environment_key;
pub use model::{Component, ComponentKind, InputDeclaration, InputValue, ProjectDefinition};
pub use output::{ResultUpdate, ResultValue, TableData};
pub use protocol::{
    HostEvent, HostMessage, MessageContext, PROTOCOL_VERSION, WorkerEvent, WorkerMessage,
};
pub use records::{LoadedProject, ProjectRevision};
pub use source::validate_source_path;
pub use state::{LivePatch, LiveState};
pub use store::{ProjectStore, SourceSnapshot};

/// Discoverable JSON Schema documents; hosts resolve their stable `$id`s to
/// these local documents rather than fetching network content.
pub const DEFINITION_SCHEMA: &str = include_str!("project/definition-v1.schema.json");
pub const PROTOCOL_SCHEMA: &str = include_str!("project/protocol-v1.schema.json");
pub const EXAMPLE_DEFINITION: &str = include_str!("project/example-v1.json");

/// Alpha resource budgets, shared by source, protocol and presentation callers.
pub mod limits {
    pub const DEFINITION_BYTES: usize = 256 * 1024;
    pub const COMPONENTS: usize = 64;
    pub const DEPTH: usize = 16;
    pub const SOURCE_FILES: usize = 32;
    pub const FILE_BYTES: usize = 256 * 1024;
    pub const SOURCE_BYTES: usize = 2 * 1024 * 1024;
    pub const FRAME_BYTES: usize = 256 * 1024;
    pub const TABLE_ROWS: usize = 500;
    pub const TABLE_CELLS: usize = 8_000;
    pub const CHART_SERIES: usize = 32;
    pub const CHART_POINTS: usize = 500;
    pub const TEXT_BYTES: usize = 16 * 1024;
    pub const OPTIONS: usize = 500;
    pub const LABEL_BYTES: usize = 160;
    pub const INPUTS: usize = 64;
    pub const ACTIONS: usize = 64;
}

#[cfg(test)]
mod protocol_tests;
#[cfg(test)]
mod state_tests;
#[cfg(test)]
mod store_tests;
#[cfg(test)]
mod tests;
