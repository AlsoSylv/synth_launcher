use launcher_core::account::types::DeviceCodeResponse;
use launcher_core::types::{AssetIndexJson, VersionJson, VersionManifest};
use std::path::PathBuf;
use tokio::sync::RwLock;

pub struct State {
    pub version_manifest: RwLock<Option<VersionManifest>>,
    pub selected_version: RwLock<Option<VersionJson>>,
    pub asset_index: RwLock<Option<AssetIndexJson>>,
    pub class_path: Option<String>,
    pub jar_path: Option<String>,
    pub path: PathBuf,
    pub device_code: Option<DeviceCodeResponse>,
}

impl State {
    pub fn new(path_buf: PathBuf) -> Self {
        Self {
            version_manifest: empty_lock(),
            selected_version: empty_lock(),
            asset_index: empty_lock(),
            class_path: None,
            jar_path: None,
            path: path_buf,
            device_code: None,
        }
    }
}

fn empty_lock<T>() -> RwLock<Option<T>> {
    RwLock::new(None)
}