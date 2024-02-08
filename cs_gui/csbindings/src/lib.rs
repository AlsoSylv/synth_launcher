use std::path::Path;
use std::ptr::{null, slice_from_raw_parts};
use std::sync::OnceLock;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;
use launcher_core::{AsyncLauncher, Error};
use launcher_core::types::VersionManifest;

fn runtime() -> &'static Runtime {
    static LOCK: OnceLock<Runtime> = OnceLock::new();
    LOCK.get_or_init(|| {
        Runtime::new().unwrap()
    })
}

fn launcher() -> &'static AsyncLauncher {
    static LOCK:OnceLock<AsyncLauncher> = OnceLock::new();
    LOCK.get_or_init(||
        AsyncLauncher::new(reqwest::Client::new())
    )
}

#[repr(C)]
pub struct NativeReturn {
    code: Code,
    error: StringWrapper
}

#[repr(C)]
pub enum Code {
    Success,
    RequestError,
    IOError,
    SerdeError,
}

impl From<Error> for NativeReturn {
    fn from(value: Error) -> Self {
        let code: Code;
        let error: StringWrapper;

        match value {
            Error::Reqwest(e) => {
                code = Code::RequestError;
                error = e.to_string().into();
            }
            Error::Tokio(e) => {
                code = Code::IOError;
                error = e.to_string().into();
            }
            Error::SerdeJson(e) => {
                code = Code::SerdeError;
                error = e.to_string().into();
            }
        }

        Self {
            code,
            error,
        }
    }
}

#[repr(C)]
pub enum ReleaseType {
    OldAlpha,
    OldBeta,
    Release,
    Snapshot,
}

impl From<launcher_core::types::Type> for ReleaseType {
    fn from(value: launcher_core::types::Type) -> Self {
        match value {
            launcher_core::types::Type::OldAlpha => { ReleaseType::OldAlpha }
            launcher_core::types::Type::OldBeta => { ReleaseType::OldBeta }
            launcher_core::types::Type::Release => { ReleaseType::Release }
            launcher_core::types::Type::Snapshot => { ReleaseType::Snapshot }
        }
    }
}


pub struct ManifestWrapper {
    inner: Option<VersionManifest>
}

pub struct TaskWrapper<T> {
    inner: Option<JoinHandle<T>>
}

#[repr(C)]
pub struct StringWrapper {
    pub char_ptr: *const u16,
    pub len: usize,
}

impl From<String> for StringWrapper {
    fn from(value: String) -> Self {
        let utf_16_buffer: Box<[u16]> = value.encode_utf16().collect();
        let leaked = Box::leak(utf_16_buffer);

        StringWrapper {
            char_ptr: leaked.as_ptr(),
            len: leaked.len()
        }
    }
}

impl StringWrapper {
    fn empty() -> Self {
        StringWrapper {
            char_ptr: null(),
            len: 0,
        }
    }
}

#[no_mangle]
///# Safety
/// No
pub unsafe extern "C" fn get_version_manifest() -> *mut TaskWrapper<Result<VersionManifest, launcher_core::Error>> {
    let launcher = launcher();
    let rt = runtime();
    let task = rt.spawn(async {
        launcher.get_version_manifest(Path::new("./")).await
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
pub extern "C" fn get_manifest_wrapper() -> *mut ManifestWrapper {
    Box::leak(Box::new(ManifestWrapper {
        inner: None
    }))
}

#[no_mangle]
///# Safety
/// The task wrapper cannot be null, otherwise this is UB
pub unsafe extern "C" fn get_manifest(task: *mut TaskWrapper<Result<VersionManifest, launcher_core::Error>>, manifest_wrapper: *mut ManifestWrapper) -> NativeReturn {
    let result = runtime().block_on((*task).inner.take().unwrap()).unwrap();
    match result {
        Ok(manifest) => {
            (*manifest_wrapper).inner = Some(manifest);
            drop(Box::from_raw(task));
            NativeReturn {
                code: Code::Success,
                error: StringWrapper::empty(),
            }
        }
        Err(e) => {
            e.into()
        }
    }
}

#[no_mangle]
///# Safety
pub unsafe extern "C" fn get_latest_release(manifest: *const ManifestWrapper) -> StringWrapper {
    let manifest = &(*manifest).inner;

    let utf_16_buffer: Box<[u16]> = manifest.as_ref().unwrap().latest.release.encode_utf16().collect();
    let leaked = Box::leak(utf_16_buffer);

    StringWrapper {
        char_ptr: leaked.as_ptr(),
        len: leaked.len()
    }
}

#[no_mangle]
///# Safety
pub unsafe extern "C" fn get_manifest_len(manifest: *const ManifestWrapper) -> usize {
    let manifest = &(*manifest).inner;

    manifest.as_ref().unwrap().versions.len()
}

#[no_mangle]
///# Safety
pub unsafe extern "C" fn get_name(manifest: *const ManifestWrapper, index: usize) -> StringWrapper {
    let manifest = &(*manifest).inner;

    let utf_16_buffer: Box<[u16]> = manifest.as_ref().unwrap().versions[index].id.encode_utf16().collect();
    let leaked = Box::leak(utf_16_buffer);

    StringWrapper {
        char_ptr: leaked.as_ptr(),
        len: leaked.len()
    }
}

#[no_mangle]
/// # Safety
/// The manifest wrapper cannot be null
pub unsafe extern "C" fn get_type(manifest_wrapper: *const ManifestWrapper, index: usize) -> ReleaseType {
    (*manifest_wrapper).inner.as_ref().unwrap().versions[index].version_type.into()
}

#[no_mangle]
pub extern "C" fn free_string_wrapper(string_wrapper: StringWrapper) {
    drop(Box::from(slice_from_raw_parts(string_wrapper.char_ptr, string_wrapper.len)));
}
