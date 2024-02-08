use std::path::Path;
use std::sync::OnceLock;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;
use launcher_core::AsyncLauncher;
use launcher_core::types::VersionManifest;

fn runtime() -> &'static Runtime {
    static LOCK: OnceLock<Runtime> = OnceLock::new();
    LOCK.get_or_init(|| {
        Runtime::new().unwrap()
    })
}

pub struct LauncherPointer {
    inner: AsyncLauncher
}

pub struct ManifestWrapper {
    inner: VersionManifest
}

pub struct TaskWrapper<T> {
    inner: Option<JoinHandle<T>>
}

#[repr(C)]
pub struct StringWrapper {
    pub char_ptr: *const u8,
    pub len: usize,
}

#[no_mangle]
pub extern "C" fn new_launcher() -> *const LauncherPointer {
    Box::leak(Box::new(LauncherPointer {
        inner: AsyncLauncher::new(reqwest::Client::new())
    }))
}

#[no_mangle]
///# Safety
/// No
pub unsafe extern "C" fn get_version_manifest(launcher: *const LauncherPointer) -> *mut TaskWrapper<VersionManifest> {
    let launcher = &(*launcher).inner;
    let rt = runtime();
    let task = rt.spawn(async {
        launcher.get_version_manifest(Path::new("./")).await.unwrap()
    });

    Box::leak(Box::new(TaskWrapper {
        inner: Some(task)
    }))
}

#[no_mangle]
///# Safety
/// No
pub unsafe extern "C" fn poll_manifest_task(task: *const TaskWrapper<VersionManifest>) -> bool {
    (*task).inner.as_ref().unwrap().is_finished()
}

#[no_mangle]
///# Safety
/// No
pub unsafe extern "C" fn get_manifest(task: *mut TaskWrapper<VersionManifest>) -> *const ManifestWrapper {
    let manifest = Box::leak(Box::new(ManifestWrapper {
        inner: runtime().block_on((*task).inner.take().unwrap()).unwrap()
    }));

    drop(Box::from_raw(task));

    manifest
}

#[no_mangle]
///# Safety
pub unsafe extern "C" fn get_latest_release(manifest: *const ManifestWrapper) -> StringWrapper {
    let manifest = &(*manifest).inner;

    StringWrapper {
        char_ptr: manifest.latest.release.as_ptr(),
        len: manifest.latest.release.len()
    }
}

#[no_mangle]
///# Safety
pub unsafe extern "C" fn get_manifest_len(manifest: *const ManifestWrapper) -> usize {
    let manifest = &(*manifest).inner;

    manifest.versions.len()
}

#[no_mangle]
///# Safety
pub unsafe extern "C" fn get_name(manifest: *const ManifestWrapper, index: usize) -> StringWrapper {
    let manifest = &(*manifest).inner;

    StringWrapper {
        char_ptr: manifest.versions[index].id.as_ptr(),
        len: manifest.versions[index].id.len()
    }
}
