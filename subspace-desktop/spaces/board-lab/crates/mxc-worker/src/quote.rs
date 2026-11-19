use std::path::Path;

pub(crate) fn python_string(value: &str) -> String {
    format!(
        "'{}'",
        value
            .replace('\\', "\\\\")
            .replace('\'', "\\'")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
    )
}

pub(crate) fn command_line(executable: &Path, args: &[&str]) -> String {
    join_quoted(&command_parts(executable, args))
}

fn command_parts(executable: &Path, args: &[&str]) -> Vec<String> {
    let mut parts = Vec::with_capacity(args.len() + 1);
    parts.push(executable.to_string_lossy().into_owned());
    for arg in args {
        parts.push((*arg).to_owned());
    }
    parts
}

fn join_quoted(parts: &[String]) -> String {
    let mut out = String::new();
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        out.push('\'');
        out.push_str(&part.replace('\'', "'\\''"));
        out.push('\'');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::command_line;
    use super::python_string;
    use std::path::Path;

    #[test]
    fn command_line_quotes_executable_and_arguments() {
        assert_eq!(command_line(Path::new("/bin/echo"), &[]), "'/bin/echo'");
        assert_eq!(
            command_line(Path::new("/bin/echo"), &["a b", "it's"]),
            "'/bin/echo' 'a b' 'it'\\''s'"
        );
    }

    #[test]
    fn python_string_escapes_quotes_and_newlines() {
        assert_eq!(python_string(r"C:\tmp"), r"'C:\\tmp'");
        assert_eq!(python_string("it's"), r"'it\'s'");
        assert_eq!(python_string("a\nb\r"), "'a\\nb\\r'");
    }
}
