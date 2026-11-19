use super::{FILE_LIMIT, WORKSPACE_LIMIT, next_id};
use crate::ContextRef;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
#[cfg(unix)]
const PRIVATE_DIRECTORY_MODE: u32 = 0o700;
#[cfg(unix)]
const PRIVATE_FILE_MODE: u32 = 0o600;

pub(super) fn private_directory(path: &Path) -> Result<(), String> {
    if fs::symlink_metadata(path).is_ok_and(|metadata| !metadata.is_dir()) {
        return Err("Copilot workspace directories must not be symbolic links or files.".into());
    }
    fs::create_dir_all(path).map_err(|_| "Cannot create private Copilot workspace.".to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(PRIVATE_DIRECTORY_MODE))
            .map_err(|_| "Cannot protect Copilot workspace permissions.".to_string())?;
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn copy_attachment(source: &Path, workspace: &Path) -> Result<ContextRef, String> {
    copy_attachment_checked(source, workspace, contains_secret)
}

pub(super) fn copy_attachment_checked(
    source: &Path,
    workspace: &Path,
    has_secret: impl Fn(&str) -> bool,
) -> Result<ContextRef, String> {
    let name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Attachment needs a UTF-8 file name.".to_string())?;
    reject_sensitive_path(source, &has_secret)?;
    let bytes = read_regular_file(source, FILE_LIMIT, "UTF-8 text")?;
    let content =
        String::from_utf8(bytes).map_err(|_| "Attach a readable UTF-8 text file.".to_string())?;
    reject_attachment(has_secret(&content))?;
    let destination = store_attachment(workspace, name, &content)?;
    Ok(ContextRef {
        key: format!("file:{}", destination.display()),
        label: name.into(),
        content,
    })
}

fn reject_attachment(unsafe_content: bool) -> Result<(), String> {
    if unsafe_content {
        return Err(
            "Credential/config files or credential-like content cannot be attached.".into(),
        );
    }
    Ok(())
}

pub(super) fn reject_sensitive_path(
    source: &Path,
    has_secret: impl Fn(&str) -> bool,
) -> Result<(), String> {
    let name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Attachment needs a UTF-8 file name.".to_string())?;
    let canonical = source
        .canonicalize()
        .map_err(|_| "Cannot resolve the attachment.".to_string())?;
    reject_attachment(sensitive_path(source) || sensitive_path(&canonical) || has_secret(name))
}

pub(super) fn read_regular_file(source: &Path, limit: u64, kind: &str) -> Result<Vec<u8>, String> {
    let metadata =
        fs::symlink_metadata(source).map_err(|_| "Cannot read the attachment.".to_string())?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(format!(
            "Attach a regular {kind} file within the size limit; symbolic links are not accepted."
        ));
    }
    let file = fs::File::open(source).map_err(|_| format!("Attach a readable {kind} file."))?;
    let opened = file
        .metadata()
        .map_err(|_| "Cannot inspect the opened attachment.".to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.dev() != opened.dev() || metadata.ino() != opened.ino() {
            return Err("Attachment changed while opening it. Try again.".into());
        }
    }
    if !opened.is_file() || opened.len() > limit {
        return Err("Attachment changed or exceeds its size limit.".into());
    }
    read_limited(file, opened.len(), limit, kind)
}

pub(super) fn read_limited(
    file: fs::File,
    expected_len: u64,
    limit: u64,
    kind: &str,
) -> Result<Vec<u8>, String> {
    let capacity = usize::try_from(expected_len.min(limit)).unwrap_or_default();
    let mut content = Vec::with_capacity(capacity);
    file.take(limit + 1)
        .read_to_end(&mut content)
        .map_err(|_| format!("Attach a readable {kind} file."))?;
    if content.len() as u64 > limit {
        return Err("Attachment grew beyond its size limit while being read.".into());
    }
    Ok(content)
}

fn store_attachment(workspace: &Path, name: &str, content: &str) -> Result<PathBuf, String> {
    private_directory(workspace)?;
    let attachments = workspace.join("attachments");
    private_directory(&attachments)?;
    validate_storage(storage_total(&attachments)?, content.len() as u64)?;
    let destination = attachments.join(format!("{}-{}-{name}", std::process::id(), next_id()));
    write_private_attachment(&destination, content)?;
    Ok(destination)
}

fn storage_total(attachments: &Path) -> Result<u64, String> {
    fs::read_dir(attachments)
        .map_err(|_| "Cannot inspect attachment storage.".to_string())?
        .try_fold(0u64, |total, entry| {
            let entry = entry?;
            Ok::<_, std::io::Error>(total.saturating_add(entry.metadata()?.len()))
        })
        .map_err(|_| "Cannot inspect attachment storage.".to_string())
}

fn validate_storage(total: u64, additional: u64) -> Result<(), String> {
    if total.saturating_add(additional) > WORKSPACE_LIMIT {
        return Err("Attachment storage exceeds 20 MiB. Remove old workspace attachments before adding more.".into());
    }
    Ok(())
}

fn write_private_attachment(destination: &Path, content: &str) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(PRIVATE_FILE_MODE);
    }
    let mut file = options
        .open(destination)
        .map_err(|_| "Cannot create private attachment copy.".to_string())?;
    file.write_all(content.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|_| "Cannot save the attachment copy.".to_string())
}

pub(super) fn sensitive_path(path: &Path) -> bool {
    path.components().any(|component| {
        let value = component.as_os_str().to_string_lossy().to_ascii_lowercase();
        matches!(
            value.as_str(),
            ".ssh"
                | ".aws"
                | ".azure"
                | ".gnupg"
                | ".netrc"
                | ".npmrc"
                | ".pypirc"
                | "credentials"
                | "copilot.json"
                | "config.json"
                | "hosts.yml"
                | "id_rsa"
                | "id_ed25519"
        ) || value.starts_with(".env")
            || value.ends_with(".pem")
            || value.ends_with(".key")
            || value.ends_with(".p12")
            || value.ends_with(".pfx")
    })
}

pub(super) fn contains_secret(text: &str) -> bool {
    let text: String = text
        .chars()
        .filter(|character| !character.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    [
        "github_pat_",
        "ghp_",
        "gho_",
        "ghu_",
        "-----begin",
        "authorization:bearer",
        "authorization:basic",
        "\"token\":",
        "\"accesstoken\":",
        "\"apikey\":",
        "\"api_key\":",
        "\"password\":",
        "access_token=",
        "api_key=",
        "password=",
        "token:",
        "password:",
        "apikey:",
    ]
    .iter()
    .any(|marker| text.contains(marker))
}
