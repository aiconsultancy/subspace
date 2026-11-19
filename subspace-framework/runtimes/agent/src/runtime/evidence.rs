use serde_json::Value;

const HOSTED_EVIDENCE_LIMIT: usize = 8;

/// Formats provider-hosted results as data, never as instructions.
pub(super) fn hosted_evidence(data: &Value) -> Option<String> {
    let mut rows = Vec::new();
    for source in data["citations"]["sources"]
        .as_array()
        .into_iter()
        .flatten()
    {
        add_source(&mut rows, source);
    }
    for item in data["serverTools"]["items"]
        .as_array()
        .into_iter()
        .flatten()
    {
        add_source(&mut rows, item);
    }
    format_rows(rows)
}

fn add_source(rows: &mut Vec<String>, source: &Value) {
    let Some(url) = source["url"].as_str().filter(|url| !url.is_empty()) else {
        return;
    };
    let title = source["title"]
        .as_str()
        .or_else(|| source["name"].as_str())
        .unwrap_or("source");
    rows.push(format!("{title}: {url}"));
}

fn format_rows(mut rows: Vec<String>) -> Option<String> {
    (!rows.is_empty()).then(|| {
        rows.sort();
        rows.dedup();
        rows.truncate(HOSTED_EVIDENCE_LIMIT);
        rows.into_iter()
            .map(|row| format!("- {row}"))
            .collect::<Vec<_>>()
            .join("\n")
    })
}
