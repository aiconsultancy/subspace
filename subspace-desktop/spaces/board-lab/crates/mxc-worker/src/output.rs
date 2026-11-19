pub(crate) fn format_output(stdout: &[u8], stderr: &[u8]) -> String {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);
    match (stdout.is_empty(), stderr.is_empty()) {
        (true, true) => String::new(),
        (false, true) => stdout.into_owned(),
        (true, false) => format!("stderr: {stderr}"),
        (false, false) => format!("stdout: {stdout}\nstderr: {stderr}"),
    }
}

#[cfg(test)]
mod tests {
    use super::format_output;

    #[test]
    fn format_output_joins_streams() {
        assert_eq!(format_output(b"", b""), "");
        assert_eq!(format_output(b"out", b""), "out");
        assert_eq!(format_output(b"", b"err"), "stderr: err");
        assert_eq!(format_output(b"out", b"err"), "stdout: out\nstderr: err");
    }
}
