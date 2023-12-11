use crate::worker_logic::Response;
use launcher_core::types::{AssetIndex, AssetIndexJson, Library, Version, VersionJson};
use launcher_core::{AsyncLauncher, Error};
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

pub async fn get_asset_index(
    launcher_core: Arc<AsyncLauncher>,
    asset_index: Arc<AssetIndex>,
    tag: Arc<Version>,
    path: &Path,
) -> Response {
    let index = launcher_core.get_asset_index_json(&asset_index, path).await;
    Response::AssetIndex(index, tag)
}

pub async fn get_version(
    launcher_core: Arc<AsyncLauncher>,
    version: Arc<Version>,
    path: &Path,
) -> Response {
    let json = launcher_core.get_version_json(&version, path).await;
    Response::Version(json.map(Box::new))
}

pub async fn get_libraries(
    launcher_core: Arc<AsyncLauncher>,
    libs: Arc<[Library]>,
    path: &Path,
    total: Arc<AtomicUsize>,
    finished: Arc<AtomicUsize>,
    tag: Arc<Version>,
) -> Response {
    let path = launcher_core
        .download_libraries_and_get_path(
            &libs,
            &path.join("libraries"),
            &path.join("natives"),
            &total,
            &finished,
        )
        .await;
    Response::Libraries(path, tag)
}

pub async fn get_jar(
    launcher_core: Arc<AsyncLauncher>,
    json: Arc<VersionJson>,
    path: &Path,
    total: Arc<AtomicUsize>,
    finished: Arc<AtomicUsize>,
    tag: Arc<Version>,
) -> Response {
    let result = launcher_core
        .download_jar(&json, path, &total, &finished)
        .await;
    Response::Jar(result, tag)
}

pub async fn get_assets(
    launcher_core: Arc<AsyncLauncher>,
    index: Arc<AssetIndexJson>,
    path: &Path,
    total: Arc<AtomicUsize>,
    finished: Arc<AtomicUsize>,
    tag: Arc<Version>,
) -> Response {
    let result = launcher_core
        .download_and_store_asset_index(&index, &path.join("assets"), &total, &finished)
        .await;
    Response::Asset(result, tag)
}

pub async fn get_major_version_response(jvm: Arc<str>) -> Response {
    Response::JavaMajorVersion(get_major_version(&jvm).await)
}

pub async fn get_default_version_response() -> Response {
    Response::DefaultJavaVersion(get_major_version("java").await)
}

/// Compiled Java byte-code to check for the current Java Version
const CHECKER_CLASS: &[u8] = include_bytes!("VersionPrinter.class");

/// Gets the Java Version of a JVM
async fn get_major_version(jvm: &str) -> Result<u32, Error> {
    let tmp = std::env::temp_dir();
    let checker_class_file = tmp.join("VersionPrinter.class");
    tokio::fs::write(checker_class_file, CHECKER_CLASS).await?;
    let process = std::process::Command::new(jvm)
        .current_dir(tmp)
        .arg("VersionPrinter")
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut io = process.stdout.expect("Wtf I hate it here");
    let mut string = String::new();
    io.read_to_string(&mut string)?;
    let mut split = string.split('.');
    let next = split.next().unwrap();
    let version = if next == "1" {
        split.next().unwrap()
    } else {
        next
    };

    Ok(version.parse().unwrap())
}
