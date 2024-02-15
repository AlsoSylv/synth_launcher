use crate::state::state_mut;
use launcher_core::account::auth::{
    authorization_token_response, minecraft_ownership_response, minecraft_profile_response,
    minecraft_response, xbox_response, xbox_security_token_response,
};
use launcher_core::account::types::{Account, DeviceCodeResponse};
use launcher_core::types::{AssetIndexJson, VersionJson, VersionManifest};
use launcher_core::{AsyncLauncher, Error};
use state::state;
use std::future::Future;
use std::path::PathBuf;
use std::ptr::null_mut;
use std::sync::{Once, OnceLock};
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

mod state {
    use launcher_core::account::types::DeviceCodeResponse;
    use launcher_core::types::{AssetIndexJson, VersionJson, VersionManifest};
    use std::cell::Cell;
    use std::mem::MaybeUninit;
    use std::path::PathBuf;
    use std::ptr::{addr_of, addr_of_mut};
    use tokio::sync::RwLock;

    pub struct State {
        pub version_manifest: MaybeUninit<RwLock<Option<VersionManifest>>>,
        pub selected_version: MaybeUninit<RwLock<Option<VersionJson>>>,
        pub asset_index: MaybeUninit<RwLock<Option<AssetIndexJson>>>,
        pub path: MaybeUninit<PathBuf>,
        pub device_code: Cell<Option<DeviceCodeResponse>>,
    }

    impl State {
        const fn new() -> Self {
            Self {
                version_manifest: MaybeUninit::uninit(),
                selected_version: MaybeUninit::uninit(),
                asset_index: MaybeUninit::uninit(),
                path: MaybeUninit::uninit(),
                device_code: Cell::new(None),
            }
        }

        pub fn init(&mut self, path_buf: PathBuf) {
            self.path.write(path_buf);
            self.version_manifest.write(empty_lock());
            self.selected_version.write(empty_lock());
            self.asset_index.write(empty_lock());
        }

        pub fn path(&self) -> &PathBuf {
            unsafe { state().path.assume_init_ref() }
        }

        pub fn version_manifest(&self) -> &RwLock<Option<VersionManifest>> {
            unsafe { self.version_manifest.assume_init_ref() }
        }

        pub fn selected_version(&self) -> &RwLock<Option<VersionJson>> {
            unsafe { self.selected_version.assume_init_ref() }
        }

        pub fn asset_index(&self) -> &RwLock<Option<AssetIndexJson>> {
            unsafe { self.asset_index.assume_init_ref() }
        }
    }

    fn empty_lock<T>() -> RwLock<Option<T>> {
        RwLock::new(None)
    }

    static mut STATE: State = State::new();

    pub fn state() -> &'static State {
        unsafe { &*addr_of!(STATE) }
    }

    pub unsafe fn state_mut() -> &'static mut State {
        unsafe { &mut *addr_of_mut!(STATE) }
    }
}

fn runtime() -> &'static Runtime {
    static LOCK: OnceLock<Runtime> = OnceLock::new();
    LOCK.get_or_init(|| Runtime::new().unwrap())
}

fn client() -> &'static reqwest::Client {
    static LOCK: OnceLock<reqwest::Client> = OnceLock::new();
    LOCK.get_or_init(|| reqwest::Client::new())
}

fn launcher() -> &'static AsyncLauncher {
    static LOCK: OnceLock<AsyncLauncher> = OnceLock::new();
    LOCK.get_or_init(|| AsyncLauncher::new(client().clone()))
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

#[repr(C)]
pub struct DeviceCode {
    pub user_code: OwnedStringWrapper,
    pub device_code: OwnedStringWrapper,
    pub verification_uri: OwnedStringWrapper,
    pub expires_in: u32,
    pub interval: u64,
    pub message: OwnedStringWrapper,
}

impl From<DeviceCodeResponse> for DeviceCode {
    fn from(value: DeviceCodeResponse) -> Self {
        DeviceCode {
            user_code: value.user_code.into(),
            device_code: value.device_code.into(),
            verification_uri: value.verification_uri.into(),
            expires_in: value.expires_in,
            interval: value.interval,
            message: value.message.into(),
        }
    }
}

pub struct TaskWrapper<T> {
    inner: Option<JoinHandle<T>>,
}

impl<T> TaskWrapper<T> {
    pub fn new<F>(t: F) -> Self
    where
        F: Future<Output = T> + Send + Sync + 'static,
        T: Send + Sync + 'static,
    {
        Self {
            inner: Some(runtime().spawn(t)),
        }
    }

    pub fn into_raw(self) -> *mut Self {
        Box::into_raw(Box::new(self))
    }
}

#[allow(dead_code)]
pub struct ManifestTaskWrapper(*mut ManifestTask);
type ManifestTask = TaskWrapper<Result<VersionManifest, Error>>;

/// # Safety
/// Because of type erasure, the compiler doesn't know if the pointer that's dereference is the right type
/// The solution to this is multiple types that the C code can hold
/// This checks for null, and panics, otherwise returning a mutable reference
unsafe fn read_task_mut<T>(task: *mut TaskWrapper<T>) -> &'static mut TaskWrapper<T> {
    assert!(!task.is_null());

    task.as_mut().unwrap_unchecked()
}

/// # Safety
/// Because of type erasure, the compiler doesn't know if the pointer that's dereference is the right type
/// The solution to this is multiple types that the C code can hold
/// This checks for null, and panics, otherwise returning a mutable reference
unsafe fn read_task_ref<T>(task: *const TaskWrapper<T>) -> &'static TaskWrapper<T> {
    assert!(!task.is_null());

    task.as_ref().unwrap_unchecked()
}

#[repr(C)]
pub struct RefStringWrapper {
    pub char_ptr: *const u8,
    pub len: usize,
}

#[repr(C)]
pub struct OwnedStringWrapper {
    pub char_ptr: *mut u8,
    pub len: usize,
    pub capacity: usize,
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
            capacity: value.capacity(),
            char_ptr: value.leak().as_mut_ptr(),
        }
    }
}

impl OwnedStringWrapper {
    fn empty() -> Self {
        OwnedStringWrapper {
            char_ptr: null_mut(),
            len: 0,
            capacity: 0,
        }
    }
}

#[no_mangle]
/// # Safety
/// Path needs to be a valid UTF-16
/// Len must be the len of the vector length, not the char length
pub unsafe extern "C" fn init(path: *const u16, len: usize) {
    static INIT: Once = Once::new();
    assert!(!path.is_null());

    INIT.call_once(|| {
        let path = String::from_utf16(std::slice::from_raw_parts(path, len)).unwrap();
        unsafe {
            state_mut().init(PathBuf::from(path).join("synth_launcher"));
        }
    })
}

#[no_mangle]
pub extern "C" fn get_version_manifest() -> *mut ManifestTaskWrapper {
    let task = TaskWrapper::new(async {
        launcher()
            .get_version_manifest(&state().path().join("versions"))
            .await
    });

    task.into_raw() as *mut ManifestTaskWrapper
}

#[no_mangle]
///# Safety
///# The task cannot be null, and has to be a manifest task.
///# The type cannot be checked by the Rust or C# compiler, and must instead be checked by the programmer.
pub unsafe extern "C" fn poll_manifest_task(task: *const ManifestTaskWrapper) -> bool {
    let task = read_task_ref(task as *const ManifestTask);
    task.inner.as_ref().unwrap().is_finished()
}

#[no_mangle]
/// This function consumes the task wrapper, dropping it, setting the manifest wrapper to a proper value
/// And then return a NativeReturn, specifying if it's a success or error
/// This is used to tell if this should be converted a C# exception
///
/// # Safety
/// # The task wrapper cannot be Null
/// # The manifest wrapper cannot be null
pub unsafe extern "C" fn await_version_manifest(task: *mut ManifestTaskWrapper) -> NativeReturn {
    let result = runtime()
        .block_on(
            read_task_mut(task as *mut ManifestTask)
                .inner
                .take()
                .unwrap(),
        )
        .unwrap();
    drop(Box::from_raw(task));

    match result {
        Ok(manifest) => {
            let mut lock = state().version_manifest().blocking_write();
            *lock = Some(manifest);
            drop(lock);
            NativeReturn::success()
        }
        Err(e) => e.into(),
    }
}

#[no_mangle]
/// # Safety
/// Task mut not be null
/// Attempting to cancel a finished task should result in a panic
pub unsafe extern "C" fn cancel_version_manifest(task: *mut ManifestTaskWrapper) {
    let task = read_task_mut(task as *mut ManifestTask);
    task.inner.take().unwrap().abort();
    unsafe { drop(Box::from_raw(task as *mut ManifestTask)) }
}

#[no_mangle]
pub extern "C" fn get_latest_release() -> RefStringWrapper {
    let manifest = state().version_manifest().blocking_read();

    RefStringWrapper::from(&manifest.as_ref().unwrap().latest.release)
}

#[no_mangle]
pub extern "C" fn get_name(index: usize) -> RefStringWrapper {
    let manifest = state().version_manifest().blocking_read();

    RefStringWrapper::from(&manifest.as_ref().unwrap().versions[index].id)
}

#[no_mangle]
pub extern "C" fn get_manifest_len() -> usize {
    let manifest = state().version_manifest().blocking_read();

    manifest.as_ref().unwrap().versions.len()
}

#[no_mangle]
pub extern "C" fn is_manifest_null() -> bool {
    state().version_manifest().blocking_read().is_none()
}

#[no_mangle]
pub extern "C" fn get_type(index: usize) -> ReleaseType {
    let manifest = state().version_manifest().blocking_read();

    manifest.as_ref().unwrap().versions[index]
        .version_type
        .into()
}

#[no_mangle]
/// # Safety
/// # The owned string wrapper cannot have been mutated outside the rust code
pub unsafe extern "C" fn free_owned_string_wrapper(string_wrapper: OwnedStringWrapper) {
    drop(String::from_raw_parts(
        string_wrapper.char_ptr,
        string_wrapper.len,
        string_wrapper.capacity,
    ))
}

#[no_mangle]
pub extern "C" fn get_version_task(index: usize) -> *mut TaskWrapper<Result<VersionJson, Error>> {
    let task = TaskWrapper::new(async move {
        let manifest = state().version_manifest().read().await;
        if let Some(manifest) = &*manifest {
            let version = &manifest.versions[index];
            launcher()
                .get_version_json(version, &state().path().join("versions"))
                .await
        } else {
            panic!("Guh")
        }
    });

    task.into_raw()
}

#[no_mangle]
///# Safety
///# The task cannot be null, and has to be a version task.
///# The type cannot be checked by the Rust or C# compiler, and must instead be checked by the programmer.
pub unsafe extern "C" fn poll_version_task(
    task: *mut TaskWrapper<Result<VersionJson, Error>>,
) -> bool {
    let task = read_task_mut(task);
    task.inner.as_ref().unwrap().is_finished()
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
                let mut writer = state().selected_version().blocking_write();
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
/// This will drop a version task regardless of completion, this is only used when cancelling
pub extern "C" fn cancel_version_task(raw_task: *mut TaskWrapper<Result<VersionJson, Error>>) {
    if raw_task.is_null() {
        panic!("Null Pointer Exception")
    }
    unsafe { (*raw_task).inner.take().unwrap().abort() }
    unsafe { drop(Box::from_raw(raw_task)) }
}

#[no_mangle]
pub extern "C" fn get_asset_index() -> *mut TaskWrapper<Result<AssetIndexJson, Error>> {
    let launcher = launcher();
    let task = TaskWrapper::new(async move {
        let version = state().selected_version();
        let path = state().path();
        let tmp = version.read().await;
        let version = tmp.as_ref().unwrap();
        launcher
            .get_asset_index_json(&version.asset_index, path)
            .await
    });

    task.into_raw()
}

#[no_mangle]
pub extern "C" fn poll_asset_index(
    task_wrapper: *mut TaskWrapper<Result<AssetIndexJson, Error>>,
) -> bool {
    let task_ref = unsafe { task_wrapper.as_ref() };
    if let Some(task) = task_ref {
        task.inner.as_ref().unwrap().is_finished()
    } else {
        panic!("Null Pointer Exception");
    }
}

#[no_mangle]
pub extern "C" fn await_asset_index(
    task_wrapper: *mut TaskWrapper<Result<AssetIndexJson, Error>>,
) -> NativeReturn {
    let task = unsafe { task_wrapper.as_mut() };

    if let Some(task) = task {
        let version = runtime().block_on(task.inner.take().unwrap()).unwrap();
        unsafe { drop(Box::from_raw(task_wrapper)) }
        match version {
            Ok(version) => {
                let mut writer = state().asset_index().blocking_write();
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
pub extern "C" fn cancel_asset_index(
    task_wrapper: *mut TaskWrapper<Result<AssetIndexJson, Error>>,
) {
    if task_wrapper.is_null() {
        panic!("Null Pointer Exception")
    }
    unsafe { read_task_mut(task_wrapper).inner.take().unwrap().abort() }
    unsafe { drop(Box::from_raw(task_wrapper)) }
}

pub const CLIENT_ID: &str = "04bc8538-fc3c-4490-9e61-a2b3f4cbcf5c";

#[no_mangle]
pub extern "C" fn get_device_response() -> *mut TaskWrapper<Result<DeviceCodeResponse, Error>> {
    let task = TaskWrapper::new(async {
        launcher_core::account::auth::device_response(client(), CLIENT_ID).await
    });

    task.into_raw()
}

#[no_mangle]
pub unsafe extern "C" fn poll_device_response(
    raw_task: *mut TaskWrapper<Result<DeviceCodeResponse, Error>>,
) -> bool {
    let task = read_task_ref(raw_task);

    task.inner.as_ref().unwrap().is_finished()
}

#[no_mangle]
pub unsafe extern "C" fn await_device_response(
    raw_task: *mut TaskWrapper<Result<DeviceCodeResponse, Error>>,
) -> NativeReturn {
    let task = read_task_mut(raw_task);

    let inner = runtime().block_on(task.inner.take().unwrap()).unwrap();

    match inner {
        Ok(response) => {
            state().device_code.set(Some(response));

            NativeReturn::success()
        }
        Err(e) => e.into(),
    }
}

#[no_mangle]
pub extern "C" fn get_user_code() -> RefStringWrapper {
    if let Some(code) = unsafe { &*state().device_code.as_ptr() } {
        code.user_code.as_str().into()
    } else {
        panic!()
    }
}

#[no_mangle]
pub extern "C" fn get_url() -> RefStringWrapper {
    if let Some(code) = unsafe { &*state().device_code.as_ptr() } {
        code.verification_uri.as_str().into()
    } else {
        panic!()
    }
}

#[no_mangle]
pub extern "C" fn start_auth_loop() -> *mut TaskWrapper<Result<Account, Error>> {
    let task = TaskWrapper::new(async {
        let device_response = unsafe { &*state().device_code.as_ptr() }.as_ref().unwrap();
        auth_loop(device_response).await
    });

    task.into_raw()
}

#[no_mangle]
pub unsafe extern "C" fn poll_auth_loop(
    raw_task: *mut TaskWrapper<Result<Account, Error>>,
) -> bool {
    let task = read_task_ref(raw_task);

    task.inner.as_ref().unwrap().is_finished()
}

#[no_mangle]
pub unsafe extern "C" fn await_auth_loop(
    raw_task: *mut TaskWrapper<Result<Account, Error>>,
) -> NativeReturn {
    let task = read_task_mut(raw_task);

    let inner = runtime().block_on(task.inner.take().unwrap()).unwrap();

    match inner {
        Ok(_response) => {
            todo!("Store Account In Memory");

            NativeReturn::success()
        }
        Err(e) => e.into(),
    }
}

async fn auth_loop(device_response: &DeviceCodeResponse) -> Result<Account, Error> {
    let auth_res = loop {
        let device_code = &device_response.device_code;
        let auth_hook = authorization_token_response(client(), device_code, CLIENT_ID).await;
        if let Ok(t) = auth_hook {
            break t;
        }
    };

    let xbox_response = xbox_response(client(), &auth_res.access_token).await?;

    let xbox_secure_token_res =
        xbox_security_token_response(client(), &xbox_response.token).await?;

    let claims = &xbox_secure_token_res.display_claims;
    let token = &xbox_secure_token_res.token;
    let mc_res = minecraft_response(claims, token, client()).await?;

    let ownership_check = minecraft_ownership_response(&mc_res.access_token, client()).await?;

    if ownership_check.items.is_empty() {
        todo!("Is this worth checking?")
    }

    let profile = minecraft_profile_response(&mc_res.access_token, client()).await?;

    use std::time::SystemTime;

    let expires_in = Duration::from_secs(auth_res.expires_in);
    let system_time = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();
    let combined_duration = system_time + expires_in;

    let account = Account {
        active: true,
        expiry: combined_duration.as_secs(),
        access_token: mc_res.access_token,
        profile,
    };

    Ok(account)
}
