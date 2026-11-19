//! Pure validation for environment identifiers, versions, and dependencies.
use super::{EnvironmentSpec, RuntimeKind};
use anyhow::{Result, bail};
const MAX_ID_LENGTH: usize = 48;
const MAX_TITLE_LENGTH: usize = 120;
const MAX_PACKAGE_LENGTH: usize = 128;
const MAX_VERSION_LENGTH: usize = 32;
const MAX_PACKAGES: usize = 32;
const MAX_TOOLS: usize = 16;
const REQUIRED_VERSION_PARTS: usize = 3;
#[derive(Clone, Copy)]
pub(super) struct RuntimeLayout {
    pub package_manager: &'static str,
    pub tool: &'static str,
    pub executable: &'static str,
}

const PYTHON_LAYOUT: RuntimeLayout = RuntimeLayout {
    package_manager: "uv",
    tool: "python",
    executable: ".venv/bin/python",
};
const NODE_LAYOUT: RuntimeLayout = RuntimeLayout {
    package_manager: "pnpm",
    tool: "node",
    executable: ".mise-bin/node",
};
const BUN_LAYOUT: RuntimeLayout = RuntimeLayout {
    package_manager: "bun",
    tool: "bun",
    executable: ".mise-bin/bun",
};
const SHELL_LAYOUT: RuntimeLayout = RuntimeLayout {
    package_manager: "mise",
    tool: "jq",
    executable: ".mise-bin/jq",
};

pub(super) fn runtime_layout(runtime: &RuntimeKind) -> RuntimeLayout {
    if matches!(runtime, RuntimeKind::Python) {
        PYTHON_LAYOUT
    } else if matches!(runtime, RuntimeKind::Node) {
        NODE_LAYOUT
    } else if matches!(runtime, RuntimeKind::Bun) {
        BUN_LAYOUT
    } else {
        SHELL_LAYOUT
    }
}
pub(super) fn validate_spec(spec: &EnvironmentSpec) -> Result<()> {
    validate_identity(spec)?;
    validate_version(&spec.version)?;
    validate_runtime(spec)?;
    validate_dependencies(spec)?;
    Ok(())
}

fn validate_identity(spec: &EnvironmentSpec) -> Result<()> {
    if !valid_id(&spec.id) {
        bail!("Environment ID must be a safe 1–48 character token");
    }
    if spec.title.is_empty()
        || spec.title.len() > MAX_TITLE_LENGTH
        || spec.title.chars().any(char::is_control)
    {
        bail!("Invalid environment title");
    }
    Ok(())
}

fn validate_runtime(spec: &EnvironmentSpec) -> Result<()> {
    let manager = runtime_package_manager(&spec.runtime);
    if spec.package_manager != manager {
        bail!("This prototype uses {manager} for {:?}", spec.runtime);
    }
    Ok(())
}

fn validate_dependencies(spec: &EnvironmentSpec) -> Result<()> {
    if spec.packages.len() > MAX_PACKAGES || spec.tools.len() > MAX_TOOLS {
        bail!("Environment dependency list too large");
    }
    for package in &spec.packages {
        validate_package(package)?;
    }
    if !spec.tools.is_empty() && !(spec.runtime == RuntimeKind::Shell && spec.tools == ["jq"]) {
        bail!("Add extra CLI tools through the explicit add_tool operation after provisioning");
    }
    Ok(())
}
fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_LENGTH
        && value.as_bytes()[0].is_ascii_alphabetic()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}
fn runtime_package_manager(runtime: &RuntimeKind) -> &str {
    runtime_layout(runtime).package_manager
}

pub(super) fn runtime_tool_name(runtime: &RuntimeKind) -> &'static str {
    runtime_layout(runtime).tool
}
pub(super) fn validate_version(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > MAX_VERSION_LENGTH
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        bail!("invalid runtime version");
    }
    let pieces = value
        .split('-')
        .next()
        .unwrap_or_default()
        .split('.')
        .collect::<Vec<_>>();
    if pieces.len() != REQUIRED_VERSION_PARTS
        || pieces
            .iter()
            .any(|piece| piece.is_empty() || !piece.bytes().all(|byte| byte.is_ascii_digit()))
    {
        bail!("Use an exact major.minor.patch runtime version");
    }
    Ok(())
}
pub(super) fn validate_package(value: &str) -> Result<()> {
    if !valid_package_shape(value) {
        bail!("invalid package name");
    }
    Ok(())
}

fn valid_package_shape(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_PACKAGE_LENGTH
        && !value.contains("://")
        && !value.contains(':')
        && !value.starts_with(['-', '/', '.'])
        && !value.contains("..")
        && (!value.contains('/') || value.starts_with('@'))
        && !value.chars().any(forbidden_package_character)
}

fn forbidden_package_character(character: char) -> bool {
    character.is_whitespace()
        || matches!(
            character,
            '"' | '\\' | '\n' | '\r' | ';' | '|' | '&' | '$' | '`'
        )
}
#[cfg(test)]
mod tests {
    use super::{validate_package, validate_version};
    #[test]
    fn versions_require_three_numeric_components() {
        assert!(validate_version("3.12.8").is_ok());
        assert!(validate_version("3.12").is_err());
    }
    #[test]
    fn packages_reject_interpreter_syntax() {
        assert!(validate_package("polars==1.17.1").is_ok());
        assert!(validate_package("https://example.invalid/pkg").is_err());
    }
}
