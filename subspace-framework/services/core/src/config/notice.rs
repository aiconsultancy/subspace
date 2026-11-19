use std::fmt;
use std::path::PathBuf;

/// A problem in the config file, shown to Zack. `key` is the JSON path of
/// the value, such as `web.search.endpoint`, or empty for the whole file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub kind: NoticeKind,
    pub file: PathBuf,
    pub key: String,
    pub message: String,
}

impl fmt::Display for Notice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let file = self.file.display();
        match self.key.as_str() {
            "" => write!(f, "{file}: {}", self.message),
            key => write!(f, "{file}: {key}: {}", self.message),
        }
    }
}
