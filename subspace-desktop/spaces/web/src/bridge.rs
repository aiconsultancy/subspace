//! The board-owned bridge. Authority is the intersection of a manifest's
//! requests and native Settings grants, bound to its folder and manifest.

pub(crate) mod issues;
pub(crate) mod protocol;
pub(crate) mod runtime;
pub(crate) mod secrets;
#[cfg(test)]
mod tests;
pub(crate) mod theme;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use subspace_framework::core::config::web_boards::{WebBoard, WebBoardSource};
use subspace_framework::dashboard::settings::FieldKind;

use crate::app::{app_origin, same_origin};
use crate::install::ManagedSource;
use crate::manifest::{self, Manifest, Permission};
use issues::IssueCall;
use protocol::{Context, Message, Operation};

const MAX_BADGE_CHARS: usize = 8;
const MAX_CONTEXT_BYTES: usize = 8192;
const MAX_LABEL_CHARS: usize = 100;
const MAX_TITLE_CHARS: usize = 100;
const MAX_BODY_CHARS: usize = 500;
const NOTIFICATION_INTERVAL: Duration = Duration::from_secs(5);
const DENIED: &str = "Permission denied. Allow this action in the board's Subspace settings.";

pub(crate) struct Bridge {
    manifest: Manifest,
    root: PathBuf,
    identity: String,
    /// Whose the page's own secrets are: the app's install source.
    owner: secrets::Owner,
    pub origin: String,
    pub badge: Option<String>,
    pub context: Option<Context>,
    last_notification: Option<Instant>,
    secret_saves: secrets::Saves,
    document_epoch: Option<u64>,
    /// The broker's name for this board, the configured ID.
    pub board: &'static str,
    /// The theme last pushed to the page, so an unchanged one is not sent.
    pub pushed_theme: Option<Value>,
}

/// What the runtime finishes with the app after a call is allowed.
pub(crate) enum Effect {
    None,
    Notify(String),
    /// Answer with the theme in effect.
    Theme,
    /// Ask the `issues` contract.
    Issues(IssueCall),
    /// Save a checked `secrets.set`.
    SaveSecret(secrets::SecretWrite),
}

impl Bridge {
    /// The bridge of folder app `board`, with `managed` its recorded install
    /// source, if any.
    pub(crate) fn new(board: &WebBoard, managed: Option<&ManagedSource>) -> Option<Self> {
        let WebBoardSource::Folder(root) = &board.source else {
            return None;
        };
        let manifest = manifest::read(root).ok()??;
        let identity = manifest.identity(root);
        Some(Self {
            owner: secrets::Owner::new(&identity, &board.source, managed),
            identity,
            manifest,
            root: root.clone(),
            origin: app_origin(&board.id),
            badge: None,
            context: None,
            last_notification: None,
            secret_saves: secrets::Saves::default(),
            document_epoch: None,
            board: issues::board_id(&board.id),
            pushed_theme: None,
        })
    }

    /// Whether the manifest asks for `permission`, granted or not.
    pub(crate) fn requests(&self, permission: Permission) -> bool {
        self.manifest.permissions.contains(&permission)
    }

    pub(crate) fn allows(&self, permission: Permission, config: &WebBoard) -> bool {
        self.requests(permission)
            && config.source == WebBoardSource::Folder(self.root.clone())
            && config
                .permissions
                .get(&format!("{}.{}", self.identity, permission.name()))
                == Some(&true)
    }

    pub(crate) fn trusted(&self, source: &str, current: &str) -> bool {
        same_origin(source, &self.origin) && same_origin(current, &self.origin)
    }

    pub(crate) fn call(
        &mut self,
        operation: &Operation,
        config: &WebBoard,
        notifications_on: bool,
        now: Instant,
    ) -> Result<(Value, Effect), &'static str> {
        if !self.manifest_current() {
            self.badge = None;
            self.context = None;
            return Err(
                "The app manifest changed. Restart Subspace, then review its permissions in Settings.",
            );
        }
        let required = match operation {
            Operation::Info => return Ok((self.info(config, notifications_on), Effect::None)),
            Operation::Theme => return Ok((Value::Null, Effect::Theme)),
            Operation::Settings => Permission::Settings,
            Operation::Badge(_) => Permission::Badge,
            Operation::Context(_) => Permission::Context,
            Operation::Notify(_) => Permission::Notifications,
            Operation::Secrets => Permission::Secrets,
            Operation::SecretsSet(_) => Permission::SaveSecrets,
            Operation::IssueScope | Operation::IssueResolve(_) | Operation::IssueOpen(_) => {
                Permission::Issues
            }
        };
        if !self.allows(required, config) {
            return Err(DENIED);
        }
        self.perform(operation, config, notifications_on, now)
    }

    // Revalidate only at the request boundary, never in a render path.
    fn manifest_current(&self) -> bool {
        manifest::read(&self.root).is_ok_and(|current| {
            current.is_some_and(|manifest| manifest.identity(&self.root) == self.identity)
        })
    }

    pub(crate) fn enter_document(&mut self, epoch: u64) {
        if self.document_epoch != Some(epoch) {
            self.badge = None;
            self.context = None;
            self.document_epoch = Some(epoch);
        }
    }

    fn perform(
        &mut self,
        operation: &Operation,
        config: &WebBoard,
        notifications_on: bool,
        now: Instant,
    ) -> Result<(Value, Effect), &'static str> {
        match operation {
            Operation::Settings => return Ok((self.settings(config), Effect::None)),
            Operation::Badge(badge) => {
                if badge
                    .as_ref()
                    .is_some_and(|badge| !bounded(badge, MAX_BADGE_CHARS))
                {
                    return Err(
                        "A badge must be at most 8 characters, without control characters.",
                    );
                }
                self.badge = badge.clone().filter(|badge| !badge.is_empty());
            }
            Operation::Context(context) => {
                if context.as_ref().is_some_and(|context| {
                    !bounded(&context.label, MAX_LABEL_CHARS)
                        || context.content.len() > MAX_CONTEXT_BYTES
                }) {
                    return Err(
                        "Context is too long: use a label up to 100 characters and content up to 8192 bytes.",
                    );
                }
                self.context = context.clone();
            }
            Operation::Notify(message) => return self.notification(message, notifications_on, now),
            Operation::Secrets | Operation::SecretsSet(_) => {
                return self.secrets(operation, config, now);
            }
            Operation::IssueScope | Operation::IssueResolve(_) | Operation::IssueOpen(_) => {
                return Ok((Value::Null, Effect::Issues(IssueCall::parse(operation)?)));
            }
            Operation::Info | Operation::Theme => {}
        }
        Ok((Value::Null, Effect::None))
    }

    /// `secrets.get`, or a checked and counted `secrets.set`.
    fn secrets(
        &mut self,
        operation: &Operation,
        config: &WebBoard,
        now: Instant,
    ) -> Result<(Value, Effect), &'static str> {
        let Operation::SecretsSet(write) = operation else {
            let value = secrets::readable(&self.manifest, &self.owner, config);
            return Ok((value, Effect::None));
        };
        secrets::check(&self.manifest, &self.owner, config, write)?;
        self.secret_saves.allow(now)?;
        Ok((Value::Null, Effect::SaveSecret(write.clone())))
    }

    fn notification(
        &mut self,
        message: &Message,
        on: bool,
        now: Instant,
    ) -> Result<(Value, Effect), &'static str> {
        if !on {
            return Err(
                "Notifications are off. Choose a notification channel in Subspace Settings.",
            );
        }
        if !bounded(&message.title, MAX_TITLE_CHARS) || !bounded(&message.body, MAX_BODY_CHARS) {
            return Err(
                "Use a notification title up to 100 characters and body up to 500 characters.",
            );
        }
        if self
            .last_notification
            .is_some_and(|last| now.saturating_duration_since(last) < NOTIFICATION_INTERVAL)
        {
            return Err("Too many notifications. Wait five seconds.");
        }
        self.last_notification = Some(now);
        Ok((
            Value::Null,
            Effect::Notify(format!("{}: {}", message.title, message.body)),
        ))
    }

    fn info(&self, config: &WebBoard, notifications_on: bool) -> Value {
        let allowed: Vec<_> = self
            .manifest
            .permissions
            .iter()
            .filter(|permission| self.allows(**permission, config))
            .map(|permission| permission.name())
            .collect();
        json!({"version": 1, "permissions": allowed,
            "notificationPermission": if notifications_on && self.allows(Permission::Notifications, config) { "granted" } else { "denied" }})
    }

    fn settings(&self, config: &WebBoard) -> Value {
        let values = self
            .manifest
            .settings
            .iter()
            .filter_map(|field| {
                if matches!(field.kind, FieldKind::Secret)
                    || config.secrets.contains_key(&field.key)
                {
                    return None;
                }
                let default = match &field.kind {
                    FieldKind::Text { default } | FieldKind::Url { default } => {
                        default.clone().map(Value::String)
                    }
                    FieldKind::Number { default } => default
                        .and_then(serde_json::Number::from_f64)
                        .map(Value::Number),
                    FieldKind::Boolean { default } => Some(Value::Bool(*default)),
                    FieldKind::Select { default, .. } => default.clone().map(Value::String),
                    FieldKind::Secret => None,
                };
                config
                    .settings
                    .get(&field.key)
                    .cloned()
                    .or(default)
                    .map(|value| (field.key.clone(), value))
            })
            .collect();
        Value::Object(values)
    }
}

fn bounded(text: &str, max: usize) -> bool {
    text.chars().count() <= max && !text.chars().any(char::is_control)
}
