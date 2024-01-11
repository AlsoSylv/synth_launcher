use crate::worker_logic::{Response, TaggedResponse};
use launcher_core::types::{AssetIndex, AssetIndexJson, Library, Version, VersionJson};
use launcher_core::{AsyncLauncher, Error};
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

pub async fn get_asset_index(
    launcher_core: Arc<AsyncLauncher>,
    asset_index: AssetIndex,
    tag: Arc<Version>,
    path: Arc<PathBuf>,
) -> Response {
    let index = launcher_core
        .get_asset_index_json(&asset_index, &path.join("assets"))
        .await;
    Response::Tagged(TaggedResponse::AssetIndex(index), tag)
}

pub async fn get_version(
    launcher_core: Arc<AsyncLauncher>,
    version: Arc<Version>,
    path: Arc<PathBuf>,
) -> Response {
    let json = launcher_core
        .get_version_json(&version, &path.join("versions"))
        .await;
    Response::Version(json.map(Box::new))
}

pub async fn get_libraries(
    launcher_core: Arc<AsyncLauncher>,
    libs: Arc<[Library]>,
    path: Arc<PathBuf>,
    total: Arc<AtomicU64>,
    finished: Arc<AtomicU64>,
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
    Response::Tagged(TaggedResponse::Libraries(path), tag)
}

pub async fn get_jar(
    launcher_core: Arc<AsyncLauncher>,
    json: Arc<VersionJson>,
    path: Arc<PathBuf>,
    total: Arc<AtomicU64>,
    finished: Arc<AtomicU64>,
    tag: Arc<Version>,
) -> Response {
    let result = launcher_core
        .download_jar(&json, &path.join("versions"), &total, &finished)
        .await;
    Response::Tagged(TaggedResponse::Jar(result), tag)
}

pub async fn get_assets(
    launcher_core: Arc<AsyncLauncher>,
    index: Arc<AssetIndexJson>,
    path: Arc<PathBuf>,
    total: Arc<AtomicU64>,
    finished: Arc<AtomicU64>,
    tag: Arc<Version>,
) -> Response {
    let result = launcher_core
        .download_and_store_asset_index(&index, &path.join("assets"), &total, &finished)
        .await;
    Response::Tagged(TaggedResponse::Asset(result), tag)
}

pub async fn get_major_version_response(jvm: Arc<String>) -> Response {
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

pub fn get_vendor_major_version(jvm: &str) -> (String, u32) {
    let tmp = std::env::temp_dir();
    let checker_class_file = tmp.join("VersionPrinter.class");
    std::fs::write(checker_class_file, CHECKER_CLASS).unwrap();
    let io = std::process::Command::new(jvm)
        .env_clear()
        .current_dir(tmp)
        .args(["-DFile.Encoding=UTF-8", "VersionPrinter"])
        .output()
        .unwrap();

    let string = String::from_utf8_lossy(&io.stdout);

    let (version, name) = unsafe { string.split_once('\n').unwrap_unchecked() };

    let mut split = version.split('.');
    let next = split.next().unwrap();
    let version = if next == "1" {
        split.next().unwrap()
    } else {
        next
    };

    let name = name.to_string();
    let version = version.parse().unwrap_or(0);

    (name, version)
}
