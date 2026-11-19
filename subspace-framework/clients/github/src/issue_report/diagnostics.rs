use super::{Error, ErrorKind};

const MAX_IDENTITY_BYTES: usize = 128;

pub use subspace_core::diagnostics::IssueDiagnostics as Diagnostics;

pub(super) fn lines(diagnostics: &Diagnostics) -> Result<Vec<String>, Error> {
    let fields = [
        ("Product", Some(diagnostics.product_id())),
        ("App version", Some(diagnostics.app_version())),
        ("Build channel", diagnostics.channel()),
        ("Commit", diagnostics.commit()),
        ("OS", diagnostics.os()),
        ("Architecture", diagnostics.architecture()),
        ("Active board", diagnostics.active_board()),
        ("Theme", diagnostics.theme_id()),
        ("Icon size", diagnostics.icon_size()),
    ];
    let mut lines: Vec<String> = fields
        .into_iter()
        .filter_map(|(name, value)| value.map(|value| validated_line(name, value)))
        .collect::<Result<_, Error>>()?;
    if let Some(paths) = diagnostics.paths() {
        lines.extend(
            paths
                .fields()
                .map(|(name, value)| format!("- {name}: {value}")),
        );
    }
    Ok(lines)
}

fn validated_line(name: &str, value: &str) -> Result<String, Error> {
    if value.is_empty()
        || value.len() > MAX_IDENTITY_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.+".contains(&byte))
    {
        return Err(ErrorKind::InvalidDiagnostics.into());
    }
    Ok(format!("- {name}: {value}"))
}
