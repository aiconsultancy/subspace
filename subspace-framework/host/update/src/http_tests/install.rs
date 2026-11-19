use crate::archive::{Wanted, plain_asset_name};
use crate::{Installed, Job, Release};

use super::{
    BINARY, NEWEST, RUNNING, Route, TAG_PREFIX, archive, asset_url, checksum_line, first_page,
    installed_executable, local_api, plain_asset, release_route, release_with_assets, second_page,
    serve, updater_against,
};

#[test]
fn a_local_api_serves_check_and_a_verified_plain_archive_install() {
    let (listener, base) = local_api();
    let (asset, zip) = plain_asset();
    let body = archive(zip, NEWEST);
    let routes = vec![
        release_route(&base, &asset),
        Route::new("GET /assets/1 ", body.clone()),
        Route::new("GET /assets/2 ", checksum_line(&asset, &body)),
        Route::new("page=1 ", first_page()),
        Route::new("page=2 ", second_page(&base, &asset)),
    ];
    let server = serve(listener, routes, 5);

    let updater = updater_against(&base);
    let release = updater
        .check()
        .expect("the check reaches the fixture API")
        .newer()
        .cloned()
        .expect("9.0.0 is newer than 1.0.0");
    assert_eq!(release, release_with_assets(&base, &asset));

    let directory = tempfile::tempdir().expect("a temporary installation folder");
    let executable = installed_executable(directory.path(), zip);
    let installed = updater
        .install_at(&release, &executable)
        .expect("the install completes");
    assert_eq!(
        installed,
        Installed {
            previous_version: RUNNING.to_owned(),
            version: NEWEST.to_owned(),
            path: executable.clone(),
        }
    );
    assert_eq!(
        std::fs::read(&executable).unwrap(),
        format!("new {BINARY}").as_bytes()
    );
    server.join().expect("the fixture server finishes");
}

#[test]
fn a_windows_release_zip_installs_its_exe_entry_on_every_host() {
    let (listener, base) = local_api();
    let asset = plain_asset_name(BINARY, NEWEST, "x86_64-pc-windows-msvc", true);
    let body = archive(true, NEWEST);
    let routes = vec![
        Route::new("GET /assets/1 ", body.clone()),
        Route::new("GET /assets/2 ", checksum_line(&asset, &body)),
    ];
    let server = serve(listener, routes, 2);
    let updater = updater_against(&base);
    let release = release_with_assets(&base, &asset);
    let directory = tempfile::tempdir().expect("a temporary installation folder");
    let executable = installed_executable(directory.path(), true);
    let installed = Job {
        app: &updater.app,
        client: updater.client().expect("the fixed-token fixture client"),
        release: &release,
        executable: &executable,
    }
    .run(&Wanted::Plain {
        name: asset,
        zip: true,
    })
    .expect("a Windows release installs its .exe entry");
    assert_eq!(installed.version, NEWEST);
    assert_eq!(
        std::fs::read(&executable).expect("read the installed executable"),
        format!("new {BINARY}").as_bytes()
    );
    server.join().expect("the fixture server finished");
}

#[test]
fn a_checksum_mismatch_installs_nothing() {
    let (listener, base) = local_api();
    let (asset, zip) = plain_asset();
    let body = archive(zip, NEWEST);
    let routes = vec![
        release_route(&base, &asset),
        Route::new("GET /assets/1 ", body),
        Route::new("GET /assets/2 ", checksum_line(&asset, b"other bytes")),
    ];
    let server = serve(listener, routes, 3);

    let directory = tempfile::tempdir().expect("a temporary installation folder");
    let executable = installed_executable(directory.path(), zip);
    let error = updater_against(&base)
        .install_at(&release_with_assets(&base, &asset), &executable)
        .unwrap_err();
    assert!(error.to_string().contains("checksum mismatch"), "{error}");
    assert_eq!(std::fs::read(&executable).unwrap(), b"old binary");
    server.join().expect("the fixture server finishes");
}

#[test]
fn a_release_without_the_targets_asset_installs_nothing() {
    let (listener, base) = local_api();
    let server = serve(
        listener,
        vec![Route::new(
            "/releases/tags/subspace-desktop-v9.0.0 ",
            serde_json::json!({"tag_name": format!("{TAG_PREFIX}{NEWEST}")}).to_string(),
        )],
        1,
    );
    let directory = tempfile::tempdir().expect("a temporary installation folder");
    let executable = installed_executable(directory.path(), cfg!(windows));
    let release = Release {
        assets: Default::default(),
        ..release_with_assets("http://127.0.0.1:1", "unused")
    };
    let error = updater_against(&base)
        .install_at(&release, &executable)
        .unwrap_err();
    assert!(error.to_string().contains("has no"), "{error}");
    assert_eq!(std::fs::read(&executable).unwrap(), b"old binary");
    server.join().expect("the fixture server finishes");
}

#[test]
fn installation_refreshes_assets_missing_from_a_cached_update_check() {
    let (listener, base) = local_api();
    let (asset, zip) = plain_asset();
    let body = archive(zip, NEWEST);
    let server = serve(
        listener,
        vec![
            release_route(&base, &asset),
            Route::new("GET /assets/1 ", body.clone()),
            Route::new("GET /assets/2 ", checksum_line(&asset, &body)),
        ],
        3,
    );
    let directory = tempfile::tempdir().unwrap();
    let executable = installed_executable(directory.path(), zip);
    let cached = Release {
        assets: Default::default(),
        ..release_with_assets(&base, &asset)
    };
    let installed = updater_against(&base)
        .install_at(&cached, &executable)
        .expect("install uses the selected version's refreshed assets");
    assert_eq!(installed.version, NEWEST);
    assert_eq!(
        std::fs::read(&executable).unwrap(),
        format!("new {BINARY}").as_bytes()
    );
    server.join().unwrap();
}

#[test]
fn a_missing_checksum_is_reported_before_downloading_or_replacing_the_binary() {
    let (listener, base) = local_api();
    let (asset, zip) = plain_asset();
    let metadata = serde_json::json!({
        "tag_name": format!("{TAG_PREFIX}{NEWEST}"),
        "assets": [{"name": asset, "url": asset_url(&base, 1)}],
    });
    let server = serve(
        listener,
        vec![Route::new(
            "/releases/tags/subspace-desktop-v9.0.0 ",
            metadata.to_string(),
        )],
        1,
    );
    let directory = tempfile::tempdir().unwrap();
    let executable = installed_executable(directory.path(), zip);
    let cached = release_with_assets(&base, &asset);
    let error = updater_against(&base)
        .install_at(&cached, &executable)
        .unwrap_err();
    assert!(error.to_string().contains("archive or checksum"), "{error}");
    assert_eq!(std::fs::read(&executable).unwrap(), b"old binary");
    server.join().unwrap();
}
