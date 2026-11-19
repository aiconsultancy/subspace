#[cfg(test)]
mod auth_tests;
mod discovery;
mod persistence;
#[cfg(test)]
mod prepare_tests;
mod request;
#[cfg(test)]
mod weather_tests;

use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::{App, AppContext as _, Context};
use subspace_framework::client_github::{BoundRead, Client};
use subspace_framework::core::credentials::Credentials;
use subspace_framework::core::settings::SettingsFile;
use subspace_framework::dashboard::{Services, prefs};

use super::{Installed, Manager, OfferedUpdate, Pending, config_path, selected_account, services};
use crate::install::{self, ImportRequest, ManagedSource};

pub(super) fn installed(cx: &App) -> Vec<Installed> {
    let Some(services) = cx.try_global::<Services>() else {
        return Vec::new();
    };
    let file = SettingsFile::open(&services.config_file).ok();
    services
        .config
        .web_boards
        .iter()
        .map(|board| {
            let metadata = file
                .as_ref()
                .and_then(|file| file.web_board(&board.id))
                .and_then(|entry| entry.get("managedSource").cloned())
                .and_then(|value| serde_json::from_value::<ManagedSource>(value).ok());
            let tracked = matches!(
                metadata,
                Some(ManagedSource::Repository { .. } | ManagedSource::Release { .. })
            );
            Installed {
                id: board.id.clone(),
                title: board.title.clone(),
                source: source_label(&board.source, metadata.as_ref()),
                version: source_version(&board.source),
                tracked,
            }
        })
        .collect()
}

fn source_label(
    source: &subspace_framework::core::config::web_boards::WebBoardSource,
    metadata: Option<&ManagedSource>,
) -> String {
    use subspace_framework::core::config::web_boards::WebBoardSource;
    match metadata {
        Some(ManagedSource::Repository {
            repository,
            board_path,
            commit,
            ..
        }) => format!(
            "GitHub · {repository} / {board_path} · {}",
            commit.chars().take(10).collect::<String>()
        ),
        Some(ManagedSource::Release {
            repository,
            tag,
            asset_name,
            ..
        }) => format!("GitHub · {repository} · {tag} · {asset_name}"),
        Some(ManagedSource::Zip { original, .. }) => format!("ZIP · {}", original.display()),
        Some(ManagedSource::Folder { original, .. }) => format!("Folder · {}", original.display()),
        _ => match source {
            WebBoardSource::Url(url) => url.clone(),
            WebBoardSource::Folder(path) => format!("Folder · {}", path.display()),
        },
    }
}

fn source_version(
    source: &subspace_framework::core::config::web_boards::WebBoardSource,
) -> Option<String> {
    let subspace_framework::core::config::web_boards::WebBoardSource::Folder(path) = source else {
        return None;
    };
    crate::manifest::read(path)
        .ok()
        .flatten()
        .map(|manifest| manifest.version)
}

struct Preparation {
    request: ImportRequest,
    id: String,
    title: String,
    replacing: bool,
    file: SettingsFile,
}

impl Manager {
    fn begin(&mut self, cx: &mut Context<Self>) -> u64 {
        self.sync_account(cx);
        self.capture_form(cx);
        self.pending = None;
        self.offer = None;
        self.error = None;
        self.message = None;
        self.busy = true;
        let ticket = self.epoch.ticket();
        cx.notify();
        ticket
    }

    pub(super) fn validate(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let result = request::from_editor(self, cx);
        match result {
            Ok(preparation) => self.prepare(preparation, cx),
            Err(error) => self.fail(error, cx),
        }
    }

    fn prepare(&mut self, preparation: Preparation, cx: &mut Context<Self>) {
        let services = match services(cx) {
            Ok(services) => services,
            Err(error) => {
                self.fail(error, cx);
                return;
            }
        };
        let credentials = services.credentials.clone();
        let store = prefs::state_dir(cx)
            .unwrap_or_else(|| {
                services
                    .config_file
                    .parent()
                    .unwrap_or(std::path::Path::new("."))
                    .to_owned()
            })
            .join("web-board-apps");
        let ticket = self.begin(cx);
        let account = selected_account(cx);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    prepare(
                        preparation,
                        &store,
                        Client::new(credentials),
                        account,
                        ticket,
                    )
                })
                .await;
            _ = this.update(cx, |this, cx| {
                this.sync_account(cx);
                if !this.epoch.accepts(ticket) {
                    return;
                }
                this.busy = false;
                match result {
                    Ok(pending) => this.pending = Some(pending),
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn check(&mut self, id: String, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let path = match config_path(cx) {
            Ok(path) => path,
            Err(error) => {
                self.fail(error, cx);
                return;
            }
        };
        let Some(services) = cx.try_global::<Services>() else {
            return;
        };
        let credentials = services.credentials.clone();
        let account = selected_account(cx);
        let ticket = self.begin(cx);
        cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { check(path, id, credentials, account, ticket) }).await;
            _ = this.update(cx, |this, cx| {
                this.sync_account(cx);
                if !this.epoch.accepts(ticket) { return }
                this.busy = false;
                match result {
                    Ok(Some(offer)) => this.offer = Some(offer),
                    Ok(None) => this.message = Some("No newer eligible version was found.".into()),
                    Err(error) => this.error = Some(format!("Updates are unavailable for this account or source. {error} Installed files are unchanged.")),
                }
                cx.notify();
            });
        }).detach();
    }

    pub(super) fn prepare_update(&mut self, cx: &mut Context<Self>) {
        self.sync_account(cx);
        let Some(offer) = self.offer.take() else {
            return;
        };
        if !self.epoch.accepts(offer.ticket) {
            return;
        }
        self.prepare(
            Preparation {
                request: offer.offer.request(),
                id: offer.id,
                title: offer.title,
                replacing: true,
                file: offer.file,
            },
            cx,
        );
    }
}

fn prepare(
    preparation: Preparation,
    store: &std::path::Path,
    client: Client,
    account: Option<String>,
    ticket: u64,
) -> Result<Pending, String> {
    let bound = if request::is_github(&preparation.request) {
        Some(bind(client, account)?)
    } else {
        None
    };
    let prepared = install::prepare(preparation.request, store, bound.as_ref())?;
    Ok(Pending {
        id: preparation.id,
        title: preparation.title,
        file: preparation.file,
        prepared,
        ticket,
        replacing: preparation.replacing,
    })
}

fn bind(client: Client, selected: Option<String>) -> Result<BoundRead, String> {
    let login = client
        .accounts()
        .map_err(|error| error.to_string())?
        .pick(selected.as_deref())
        .ok_or("Select a GitHub account in Settings first.")?;
    client.bind_read(&login).map_err(|error| error.to_string())
}

fn check(
    path: PathBuf,
    id: String,
    credentials: Arc<Credentials>,
    account: Option<String>,
    ticket: u64,
) -> Result<Option<OfferedUpdate>, String> {
    let file = SettingsFile::open(&path).map_err(|error| error.to_string())?;
    let entry = file.web_board(&id).ok_or("This board has been removed.")?;
    let metadata = entry
        .get("managedSource")
        .cloned()
        .ok_or("This board has no tracked GitHub source.")?;
    let source = serde_json::from_value::<ManagedSource>(metadata)
        .map_err(|_| "The source record is invalid.")?;
    let title = entry
        .get("title")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(&id)
        .to_owned();
    let bound = bind(Client::new(credentials), account)?;
    Ok(
        install::check_update(&source, &bound)?.map(|offer| OfferedUpdate {
            id,
            title,
            offer,
            ticket,
            file,
        }),
    )
}
