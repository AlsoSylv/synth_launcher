use std::path::Path;
use std::ptr::null;
use std::sync::OnceLock;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;
use launcher_core::{AsyncLauncher, Error};
use launcher_core::types::VersionManifest;

mod state {
    use std::sync::Mutex;
    use launcher_core::types::VersionManifest;

    pub struct State {
        pub version_manifest: Mutex<Option<VersionManifest>>
    }

    impl State {
        const fn new() -> Self {
            Self {
                version_manifest: Mutex::new(None)
            }
        }
    }

    pub static STATE: State = State::new();
}

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

fn state() -> &'static state::State {
    &state::STATE
}

#[repr(C)]
pub struct NativeReturn {
    code: Code,
    error: OwnedStringWrapper
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
        let error: OwnedStringWrapper;

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

pub struct TaskWrapper<T> {
    inner: Option<JoinHandle<T>>
}

#[repr(C)]
pub struct RefStringWrapper {
    pub char_ptr: *const u8,
    pub len: usize,
}

#[repr(C)]
pub struct OwnedStringWrapper {
    pub char_ptr: *const u8,
    pub len: usize,
}

impl<'a> From<&'a str> for RefStringWrapper {
    fn from(value: &'a str) -> Self {
        RefStringWrapper {
            char_ptr: value.as_ptr(),
            len: value.len()
        }
    }
}

impl<'a> From<&'a String> for RefStringWrapper {
    fn from(value: &'a String) -> Self {
        Self::from(value.as_str())
    }
}

impl From<String> for OwnedStringWrapper {
    fn from(value: String) -> Self {
        OwnedStringWrapper {
            len: value.len(),
            char_ptr: value.leak().as_ptr(),
        }
    }
}

impl OwnedStringWrapper {
    fn empty() -> Self {
        OwnedStringWrapper {
            char_ptr: null(),
            len: 0,
        }
    }
}

#[no_mangle]
pub extern "C" fn is_manifest_null() -> bool {
    state().version_manifest.lock().unwrap().is_none()
}

#[no_mangle]
///# Safety
/// No
pub unsafe extern "C" fn get_version_manifest() -> *mut TaskWrapper<Result<VersionManifest, Error>> {
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
/// This function consumes the task wrapper, dropping it, setting the manifest wrapper to a proper value
/// And then return a NativeReturn, specifying if it's a success or error
/// This is used to tell if this should be converted a C# exception
///
/// # Safety
/// # The task wrapper cannot be Null
/// # The manifest wrapper cannot be null
pub unsafe extern "C" fn get_manifest(task: *mut TaskWrapper<Result<VersionManifest, Error>>) -> NativeReturn {
    let result = runtime().block_on((*task).inner.take().unwrap()).unwrap();
    drop(Box::from_raw(task));

    match result {
        Ok(manifest) => {
            let mut lock = state().version_manifest.lock().unwrap();
            *lock = Some(manifest);
            drop(lock);
            NativeReturn {
                code: Code::Success,
                error: OwnedStringWrapper::empty(),
            }
        }
        Err(e) => {
            e.into()
        }
    }
}

#[no_mangle]
/// # Safety
/// # Manifest Wrapper cannot equal null
pub unsafe extern "C" fn get_latest_release() -> RefStringWrapper {
    let manifest = state().version_manifest.lock().unwrap();

    RefStringWrapper::from(&manifest.as_ref().unwrap().latest.release)
}

#[no_mangle]
/// # Safety
/// # Manifest Wrapper cannot equal Null
pub unsafe extern "C" fn get_name(index: usize) -> RefStringWrapper {
    let manifest = state().version_manifest.lock().unwrap();

    RefStringWrapper::from(&manifest.as_ref().unwrap().versions[index].id)
}

#[no_mangle]
///# Safety
pub unsafe extern "C" fn get_manifest_len() -> usize {
    let manifest = state().version_manifest.lock().unwrap();

    manifest.as_ref().unwrap().versions.len()
}

#[no_mangle]
/// # Safety
/// The manifest wrapper cannot be null
pub unsafe extern "C" fn get_type(index: usize) -> ReleaseType {
    let manifest = state().version_manifest.lock().unwrap();

    manifest.as_ref().unwrap().versions[index].version_type.into()
}

#[no_mangle]
pub extern "C" fn free_string_wrapper(string_wrapper: OwnedStringWrapper) {
    drop(Box::from(std::ptr::slice_from_raw_parts(string_wrapper.char_ptr, string_wrapper.len)));
}
