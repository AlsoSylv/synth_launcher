use launcher_core::types::{AssetIndexJson, VersionJson, VersionManifest};
use launcher_core::{AsyncLauncher, Error};
use std::path::PathBuf;
use std::ptr::null;
use std::sync::OnceLock;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

mod state {
    use launcher_core::types::{AssetIndexJson, VersionJson, VersionManifest};
    use std::path::PathBuf;
    use std::sync::OnceLock;

    pub struct State {
        pub version_manifest: OnceLock<tokio::sync::RwLock<Option<VersionManifest>>>,
        pub selected_version: OnceLock<tokio::sync::RwLock<Option<VersionJson>>>,
        pub asset_index: OnceLock<tokio::sync::RwLock<Option<AssetIndexJson>>>,
        pub path: OnceLock<PathBuf>,
    }

    impl State {
        const fn new() -> Self {
            Self {
                version_manifest: OnceLock::new(),
                selected_version: OnceLock::new(),
                asset_index: OnceLock::new(),
                path: OnceLock::new(),
            }
        }

        pub fn init(&self, path_buf: PathBuf) {
            self.path.get_or_init(|| path_buf);
            self.selected_version.get_or_init(empty_lock);
            self.version_manifest.get_or_init(empty_lock);
            self.asset_index.get_or_init(empty_lock);
        }
    }

    fn empty_lock<T>() -> tokio::sync::RwLock<Option<T>> {
        tokio::sync::RwLock::new(None)
    }

    pub static STATE: State = State::new();
}

fn runtime() -> &'static Runtime {
    static LOCK: OnceLock<Runtime> = OnceLock::new();
    LOCK.get_or_init(|| Runtime::new().unwrap())
}

fn launcher() -> &'static AsyncLauncher {
    static LOCK: OnceLock<AsyncLauncher> = OnceLock::new();
    LOCK.get_or_init(|| AsyncLauncher::new(reqwest::Client::new()))
}

fn state() -> &'static state::State {
    &state::STATE
}

#[repr(C)]
pub struct NativeReturn {
    code: Code,
    error: OwnedStringWrapper,
}

impl NativeReturn {
    fn success() -> Self {
        Self {
            code: Code::Success,
            error: OwnedStringWrapper::empty(),
        }
    }
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

        Self { code, error }
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
            launcher_core::types::Type::OldAlpha => ReleaseType::OldAlpha,
            launcher_core::types::Type::OldBeta => ReleaseType::OldBeta,
            launcher_core::types::Type::Release => ReleaseType::Release,
            launcher_core::types::Type::Snapshot => ReleaseType::Snapshot,
        }
    }
}

pub struct TaskWrapper<T> {
    inner: Option<JoinHandle<T>>,
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
            len: value.len(),
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
/// # Safety
/// Path needs to be a valid UTF-16
/// Len must be the len of the vector length, not the char length
pub extern "C" fn init(path: *const u16, len: usize) {
    let path = String::from_utf16(unsafe { std::slice::from_raw_parts(path, len) }).unwrap();
    state().init(PathBuf::from(path).join("synth_launcher"));
}

#[no_mangle]
pub extern "C" fn is_manifest_null() -> bool {
    state()
        .version_manifest
        .get()
        .unwrap()
        .blocking_read()
        .is_none()
}

#[no_mangle]
pub extern "C" fn get_version_manifest() -> *mut TaskWrapper<Result<VersionManifest, Error>> {
    let launcher = launcher();
    let rt = runtime();
    let task = rt.spawn(async {
        launcher
            .get_version_manifest(&state().path.get().unwrap().join("versions"))
            .await
    });

    Box::leak(Box::new(TaskWrapper { inner: Some(task) }))
}

#[no_mangle]
///# Safety
/// No
pub extern "C" fn poll_manifest_task(task: *const TaskWrapper<VersionManifest>) -> bool {
    // SAFETY
    // This pointer comes from C#, and is confirmed the correct value
    unsafe {
        if let Some(task) = task.as_ref() {
            task.inner.as_ref().unwrap().is_finished()
        } else {
            panic!("Null Pointer exception")
        }
    }
}

#[no_mangle]
/// This function consumes the task wrapper, dropping it, setting the manifest wrapper to a proper value
/// And then return a NativeReturn, specifying if it's a success or error
/// This is used to tell if this should be converted a C# exception
///
/// # Safety
/// # The task wrapper cannot be Null
/// # The manifest wrapper cannot be null
pub unsafe extern "C" fn get_manifest(
    task: *mut TaskWrapper<Result<VersionManifest, Error>>,
) -> NativeReturn {
    let result = runtime().block_on((*task).inner.take().unwrap()).unwrap();
    drop(Box::from_raw(task));

    match result {
        Ok(manifest) => {
            let mut lock = state().version_manifest.get().unwrap().blocking_write();
            *lock = Some(manifest);
            drop(lock);
            NativeReturn::success()
        }
        Err(e) => e.into(),
    }
}

#[no_mangle]
pub extern "C" fn get_latest_release() -> RefStringWrapper {
    let manifest = state().version_manifest.get().unwrap().blocking_read();

    RefStringWrapper::from(&manifest.as_ref().unwrap().latest.release)
}

#[no_mangle]
pub extern "C" fn get_name(index: usize) -> RefStringWrapper {
    let manifest = state().version_manifest.get().unwrap().blocking_read();

    RefStringWrapper::from(&manifest.as_ref().unwrap().versions[index].id)
}

#[no_mangle]
pub extern "C" fn get_manifest_len() -> usize {
    let manifest = state().version_manifest.get().unwrap().blocking_read();

    manifest.as_ref().unwrap().versions.len()
}

#[no_mangle]
pub extern "C" fn get_type(index: usize) -> ReleaseType {
    let manifest = state().version_manifest.get().unwrap().blocking_read();

    manifest.as_ref().unwrap().versions[index]
        .version_type
        .into()
}

#[no_mangle]
pub extern "C" fn free_string_wrapper(string_wrapper: OwnedStringWrapper) {
    drop(Box::from(std::ptr::slice_from_raw_parts(
        string_wrapper.char_ptr,
        string_wrapper.len,
    )));
}

#[no_mangle]
pub extern "C" fn get_version(index: usize) -> *mut TaskWrapper<Result<VersionJson, Error>> {
    let task = runtime().spawn(async move {
        let manifest = state().version_manifest.get().unwrap().read().await;
        if let Some(manifest) = &*manifest {
            let version = &manifest.versions[index];
            launcher()
                .get_version_json(version, &state().path.get().unwrap().join("versions"))
                .await
        } else {
            panic!("Guh")
        }
    });

    Box::leak(Box::new(TaskWrapper { inner: Some(task) }))
}

#[no_mangle]
pub extern "C" fn is_version_task_finished(
    task: *mut TaskWrapper<Result<VersionJson, Error>>,
) -> bool {
    let task = unsafe { task.as_ref() };
    if let Some(task) = task {
        task.inner.as_ref().unwrap().is_finished()
    } else {
        panic!("Null Pointer Exception")
    }
}

#[no_mangle]
pub extern "C" fn await_version_task(
    raw_task: *mut TaskWrapper<Result<VersionJson, Error>>,
) -> NativeReturn {
    let task = unsafe { raw_task.as_mut() };

    if let Some(task) = task {
        let version = runtime().block_on(task.inner.take().unwrap()).unwrap();
        unsafe { drop(Box::from_raw(raw_task)) }
        match version {
            Ok(version) => {
                let mut writer = state().selected_version.get().unwrap().blocking_write();
                *writer = Some(version);
                drop(writer);
                NativeReturn::success()
            }
            Err(e) => e.into(),
        }
    } else {
        panic!("Null Pointer Exception")
    }
}

#[no_mangle]
pub extern "C" fn get_asset_index() -> *mut TaskWrapper<Result<AssetIndexJson, Error>> {
    if let Some(version) = state().selected_version.get() {
        let launcher = launcher();
        let task = runtime().spawn(async move {
            let path = state().path.get().unwrap();
            let tmp = version.read().await;
            let version = tmp.as_ref().unwrap();
            launcher.get_asset_index_json(&version.asset_index, path).await
        });

        Box::leak(Box::new(TaskWrapper {
            inner: Some(task)
        }))
    } else {
        panic!("Null Pointer Exception");
    }
}

#[no_mangle]
pub extern "C" fn poll_asset_index(task_wrapper: *mut TaskWrapper<Result<AssetIndexJson, Error>>) -> bool {
    let task_ref = unsafe { task_wrapper.as_ref() };
    if let Some(task) = task_ref {
        task.inner.as_ref().unwrap().is_finished()
    } else {
        panic!("Null Pointer Exception");
    }
}

#[no_mangle]
pub extern "C" fn await_asset_index(task_wrapper: *mut TaskWrapper<Result<AssetIndexJson, Error>>) -> NativeReturn {
    let task = unsafe { task_wrapper.as_mut() };

    if let Some(task) = task {
        let version = runtime().block_on(task.inner.take().unwrap()).unwrap();
        unsafe { drop(Box::from_raw(task_wrapper)) }
        match version {
            Ok(version) => {
                let mut writer = state().asset_index.get().unwrap().blocking_write();
                *writer = Some(version);
                drop(writer);
                NativeReturn::success()
            }
            Err(e) => e.into(),
        }
    } else {
        panic!("Null Pointer Exception")
    }
}

#[no_mangle]
pub extern "C" fn cancel_asset_index(task_wrapper: *mut TaskWrapper<Result<AssetIndexJson, Error>>) {
    if task_wrapper.is_null() {
        panic!("Null Pointer Exception")
    }
    unsafe { (*task_wrapper).inner.take().unwrap().abort() }
    unsafe { drop(Box::from_raw(task_wrapper)) }
}

#[no_mangle]
/// This will drop a version task regardless of completion, this is only used when cancelling
pub extern "C" fn cleanup_version_task(raw_task: *mut TaskWrapper<Result<VersionJson, Error>>) {
    if raw_task.is_null() {
        panic!("Null Pointer Exception")
    }
    unsafe { (*raw_task).inner.take().unwrap().abort() }
    unsafe { drop(Box::from_raw(raw_task)) }
}
