use internal::*;
mod internal;
use csmacros::dotnetfunction;
use error::Error;
use instances::{Instance, Jvm};
use launcher_core::account::auth::{
    authorization_token_response, minecraft_profile_response, minecraft_response,
    refresh_token_response, xbox_response, xbox_security_token_response,
};
use launcher_core::account::types::{
    Account, AuthorizationTokenResponse, DeviceCodeResponse, MinecraftAuthenticationResponse,
    Profile,
};
use launcher_core::types::{AssetIndexJson, Version, VersionJson, VersionManifest};
use launcher_core::{account, AsyncLauncher};
use serde::{Deserialize, Serialize};
use state::State;
use std::fmt::Display;
use std::path::PathBuf;
use std::ptr::null_mut;
use std::slice;
use std::sync::atomic::AtomicU64;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};
use tasks::{await_task, cancel_task, get_task, poll_task};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::runtime::Runtime;

use crate::internal::tasks::await_result_task;
pub use tasks::TaskWrapper;

#[derive(Default, Deserialize, Serialize, Debug)]
pub struct LauncherData {
    jvms: Vec<Jvm>,
    accounts: Vec<AccRefreshPair>,
    instances: Vec<Instance>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct AccRefreshPair {
    account: Account,
    refresh_token: String,
}

pub fn runtime() -> &'static Runtime {
    static LOCK: OnceLock<Runtime> = OnceLock::new();
    LOCK.get_or_init(|| Runtime::new().unwrap())
}

fn client() -> &'static reqwest::Client {
    static LOCK: OnceLock<reqwest::Client> = OnceLock::new();
    LOCK.get_or_init(reqwest::Client::new)
}

fn launcher() -> &'static AsyncLauncher {
    static LOCK: OnceLock<AsyncLauncher> = OnceLock::new();
    LOCK.get_or_init(|| AsyncLauncher::new(client().clone()))
}

/// This exists so that task types can be checked on the C# side of the codebase
pub struct ManifestTaskWrapper;
/// This exists so I can type cast easier
pub type ManifestTask = TaskWrapper<Result<VersionManifest, Error>>;

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
    ProfileError,
    JvmError,
    TomlDe,
}

impl From<Error> for NativeReturn {
    fn from(value: Error) -> Self {
        let (code, e): (_, &dyn Display) = match &value {
            Error::Reqwest(e) => (Code::RequestError, e),
            Error::Tokio(e) => (Code::IOError, e),
            Error::SerdeJson(e) => (Code::SerdeError, e),
            Error::Profile(e) => (Code::ProfileError, e),
            Error::TomlDe(e) => (Code::TomlDe, e),
        };

        Self {
            code,
            error: e.to_string().into(),
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
            launcher_core::types::Type::OldAlpha => ReleaseType::OldAlpha,
            launcher_core::types::Type::OldBeta => ReleaseType::OldBeta,
            launcher_core::types::Type::Release => ReleaseType::Release,
            launcher_core::types::Type::Snapshot => ReleaseType::Snapshot,
        }
    }
}

pub struct VersionErased;

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

#[dotnetfunction]
pub unsafe fn new_rust_state(raw_path: String) -> *mut State {
    if let Ok(path) = raw_path {
        let path = PathBuf::from(path).join("synth_launcher");
        Box::leak(Box::new(State::new(path)))
    } else {
        null_mut()
    }
}

#[dotnetfunction]
/// # Safety
pub unsafe fn get_version_manifest(state: *mut State) -> *mut ManifestTaskWrapper {
    let state = &*state;
    get_task(async {
        Ok::<_, Error>(
            launcher()
                .get_version_manifest(&state.path.join("versions"))
                .await?,
        )
    }) as _
}

#[dotnetfunction]
///# Safety
///# The task cannot be null, and has to be a manifest task.
///# The type cannot be checked by the Rust or C# compiler, and must instead be checked by the programmer.
pub fn poll_manifest_task(raw_task: *const ManifestTaskWrapper) -> bool {
    poll_task(raw_task as *const ManifestTask)
}

#[dotnetfunction]
/// This function consumes the task wrapper, dropping it, setting the manifest wrapper to a proper value
/// And then return a NativeReturn, specifying if it's a success or error
/// This is used to tell if this should be converted a C# exception
///
/// # Safety
/// # The task wrapper cannot be Null
/// # The manifest wrapper cannot be null
pub unsafe fn await_version_manifest(
    state: *mut State,
    raw_task: *mut ManifestTaskWrapper,
) -> NativeReturn {
    await_result_task(raw_task as *mut ManifestTask, |inner| {
        let state = &*state;
        let mut lock = state.version_manifest.blocking_write();
        *lock = Box::leak(Box::new(Some(inner)));
        drop(lock);
        NativeReturn::success()
    })
}

#[dotnetfunction]
/// # Safety
/// Task mut not be null
/// Attempting to cancel a finished task should result in a panic
pub unsafe fn cancel_version_manifest(task: *mut ManifestTaskWrapper) {
    cancel_task(task as *mut ManifestTask)
}

#[dotnetfunction]
/// # Safety
pub unsafe fn get_latest_release(state: *mut State) -> RefStringWrapper {
    let manifest = state.as_ref().unwrap().version_manifest.blocking_read();

    RefStringWrapper::from(&manifest.as_ref().unwrap().latest.release)
}

#[dotnetfunction]
/// # Safety
pub unsafe fn get_name(state: *mut State, index: usize) -> RefStringWrapper {
    let manifest = state.as_ref().unwrap().version_manifest.blocking_read();

    RefStringWrapper::from(&manifest.as_ref().unwrap().versions[index].id)
}

#[dotnetfunction]
/// # Safety
pub unsafe fn get_manifest_len(state: *mut State) -> usize {
    let manifest = state.as_ref().unwrap().version_manifest.blocking_read();

    manifest.as_ref().unwrap().versions.len()
}

#[dotnetfunction]
/// # Safety
pub unsafe fn is_manifest_null(state: *mut State) -> bool {
    state
        .as_ref()
        .unwrap()
        .version_manifest
        .blocking_read()
        .is_none()
}

#[dotnetfunction]
/// # Safety
/// # The owned string wrapper cannot have been mutated outside the rust code
pub unsafe fn free_owned_string_wrapper(string_wrapper: OwnedStringWrapper) {
    drop(String::from_raw_parts(
        string_wrapper.char_ptr,
        string_wrapper.len,
        string_wrapper.capacity,
    ))
}

#[no_mangle]
/// # Safety
/// # State cannot be null, index cannot be greater than mainfest len
/// # The lifetime of this pointer is the same as the version manifest
pub unsafe extern "C" fn get_version(state: *const State, index: usize) -> *const VersionErased {
    let state = unsafe { &*state };
    &state
        .version_manifest
        .blocking_read()
        .as_ref()
        .unwrap()
        .versions[index] as *const Version as *const _
}

pub fn get_version_safe(state: &'static State, index: usize) -> &'static Version {
    &state
        .version_manifest
        .blocking_read()
        .as_ref()
        .unwrap()
        .versions[index]
}

#[no_mangle]
/// # Safety
/// # version cannot be null and must point to a valid version
pub unsafe extern "C" fn version_name(version: *const VersionErased) -> RefStringWrapper {
    let version = &*(version as *const Version);
    version.id.as_str().into()
}

#[no_mangle]
/// # Safety
/// # version cannot be null and must point to a valid version
pub unsafe extern "C" fn version_type(version: *const VersionErased) -> ReleaseType {
    let version = &*(version as *const Version);
    version.version_type.into()
}

#[no_mangle]
/// # Safety
pub unsafe extern "C" fn get_version_task(
    state: *mut State,
    version: *const VersionErased,
) -> *mut TaskWrapper<Result<VersionJson, Error>> {
    let state = &*state;
    let version = &*(version as *const Version);
    get_task(async move {
        Ok(launcher()
            .get_version_json(version, &state.path.join("versions"))
            .await?)
    })
}

#[no_mangle]
///# Safety
///# The task cannot be null, and has to be a version task.
///# The type cannot be checked by the Rust or C# compiler, and must instead be checked by the programmer.
pub unsafe extern "C" fn poll_version_task(
    raw_task: *const TaskWrapper<Result<VersionJson, Error>>,
) -> bool {
    poll_task(raw_task)
}

#[no_mangle]
/// # Safety
pub unsafe extern "C" fn await_version_task(
    state: *mut State,
    raw_task: *mut TaskWrapper<Result<VersionJson, Error>>,
) -> NativeReturn {
    let state = &*state;
    await_result_task(raw_task, |inner| {
        let mut writer = state.selected_version.blocking_write();
        *writer = Some(inner);
        drop(writer);
        NativeReturn::success()
    })
}

#[no_mangle]
/// # Safety
/// This will drop a version task regardless of completion, this is only used when cancelling
pub unsafe extern "C" fn cancel_version_task(
    raw_task: *mut TaskWrapper<Result<VersionJson, Error>>,
) {
    cancel_task(raw_task)
}

#[no_mangle]
/// # Safety
pub unsafe extern "C" fn get_asset_index(
    state: *mut State,
) -> *mut TaskWrapper<Result<AssetIndexJson, Error>> {
    let state = &*state;
    get_task(async move {
        let version = &state.selected_version;
        let path = &state.path.join("assets");
        let tmp = version.read().await;
        let version = tmp.as_ref().unwrap();
        Ok(launcher()
            .get_asset_index_json(&version.asset_index, path)
            .await?)
    })
}

#[no_mangle]
/// # Safety
pub extern "C" fn poll_asset_index(
    raw_task: *const TaskWrapper<Result<AssetIndexJson, Error>>,
) -> bool {
    poll_task(raw_task)
}

#[no_mangle]
/// # Safety
pub unsafe extern "C" fn await_asset_index(
    state: *mut State,
    raw_task: *mut TaskWrapper<Result<AssetIndexJson, Error>>,
) -> NativeReturn {
    let state = &*state;
    await_result_task(raw_task, |inner| {
        let mut writer = state.asset_index.blocking_write();
        *writer = Some(inner);
        drop(writer);
        NativeReturn::success()
    })
}

#[no_mangle]
/// # Safety
pub unsafe extern "C" fn cancel_asset_index(
    raw_task: *mut TaskWrapper<Result<AssetIndexJson, Error>>,
) {
    cancel_task(raw_task)
}

#[no_mangle]
/// # Safety
/// Total and Finished will be treated like atomics
pub unsafe extern "C" fn get_libraries(
    state: *mut State,
    total: *mut u64,
    finished: *mut u64,
) -> *mut TaskWrapper<Result<String, Error>> {
    let state = &*state;
    let total = AtomicU64::from_ptr(total);
    let finished = AtomicU64::from_ptr(finished);
    get_task(async move {
        let binding = state.selected_version.read().await;
        let version = binding.as_ref().unwrap();
        Ok(launcher()
            .download_libraries_and_get_path(
                version.libraries(),
                &state.path.join("libraries"),
                &state.path.join("natives"),
                total,
                finished,
            )
            .await?)
    })
}

#[no_mangle]
pub extern "C" fn poll_libraries(raw_task: *const TaskWrapper<Result<String, Error>>) -> bool {
    poll_task(raw_task)
}

#[no_mangle]
/// # Safety
pub unsafe extern "C" fn await_libraries(
    state: *mut State,
    raw_task: *mut TaskWrapper<Result<String, Error>>,
) -> NativeReturn {
    await_result_task(raw_task, |inner| {
        let state = &mut *state;
        state.class_path = Some(inner);
        NativeReturn::success()
    })
}

#[no_mangle]
pub extern "C" fn cancel_libraries(raw_task: *mut TaskWrapper<Result<(), Error>>) {
    cancel_task(raw_task)
}

#[no_mangle]
/// # Safety
/// # Total and Finished will be treated like atomics
pub unsafe extern "C" fn get_assets(
    state: *mut State,
    total: *mut u64,
    finished: *mut u64,
) -> *mut TaskWrapper<Result<(), Error>> {
    let state = &*state;
    let total = AtomicU64::from_ptr(total);
    let finished = AtomicU64::from_ptr(finished);
    get_task(async move {
        let binding = state.asset_index.read().await;
        let asset_index = binding.as_ref().unwrap();
        Ok(launcher()
            .download_and_store_asset_index(
                asset_index,
                &state.path.join("assets"),
                total,
                finished,
            )
            .await?)
    })
}

#[no_mangle]
pub extern "C" fn poll_assets(raw_task: *const TaskWrapper<Result<(), Error>>) -> bool {
    poll_task(raw_task)
}

#[no_mangle]
pub extern "C" fn await_assets(raw_task: *mut TaskWrapper<Result<(), Error>>) -> NativeReturn {
    await_result_task(raw_task, |_| NativeReturn::success())
}

#[no_mangle]
pub extern "C" fn cancel_assets(raw_task: *mut TaskWrapper<Result<(), Error>>) {
    cancel_task(raw_task)
}

#[no_mangle]
/// # Safety
pub unsafe extern "C" fn get_jar(
    state: *mut State,
    total: *mut u64,
    finished: *mut u64,
) -> *mut TaskWrapper<Result<String, Error>> {
    let state = &*state;
    let total = AtomicU64::from_ptr(total);
    let finished = AtomicU64::from_ptr(finished);
    get_task(async move {
        let binding = &state.selected_version.read().await;
        let version = binding.as_ref().unwrap();
        Ok(launcher()
            .download_jar(version, &state.path.join("versions"), total, finished)
            .await?)
    })
}

#[no_mangle]
pub extern "C" fn poll_jar(raw_task: *mut TaskWrapper<Result<String, Error>>) -> bool {
    poll_task(raw_task)
}

#[no_mangle]
/// # Safety
pub unsafe extern "C" fn await_jar(
    state: *mut State,
    raw_task: *mut TaskWrapper<Result<String, Error>>,
) -> NativeReturn {
    await_result_task(raw_task, |inner| {
        (*state).jar_path = Some(inner);
        NativeReturn::success()
    })
}

#[no_mangle]
pub extern "C" fn cancel_jar(raw_task: *mut TaskWrapper<Result<String, Error>>) {
    cancel_task(raw_task)
}

#[no_mangle]
pub unsafe extern "C" fn play(
    state: *const State,
    data: *const LauncherData,
    jvm_index: usize,
    acc_index: usize,
) {
    let state = &*state;
    let guard = &*data;
    let jvm = &guard.jvms[jvm_index];
    let acc = &guard.accounts[acc_index];
    let guard = state.selected_version.blocking_read();
    let version_json = guard.as_ref().unwrap();
    let directory = &state.path;
    let class_path = state.class_path.as_ref().unwrap();
    let jar_path = state.jar_path.as_ref().unwrap();
    launcher_core::launch_game(
        &jvm.path,
        version_json,
        directory,
        &directory.join("assets"),
        &acc.account,
        CLIENT_ID,
        "",
        "synth_launcher",
        "0",
        &format!("{class_path}{jar_path}"),
    );
}

#[no_mangle]
pub unsafe extern "C" fn play_default_jvm(
    state: *const State,
    data: *const LauncherData,
    acc_index: usize,
) {
    let state = &*state;
    let guard = &*data;
    let acc = &guard.accounts[acc_index];
    let guard = state.selected_version.blocking_read();
    let version_json = guard.as_ref().unwrap();
    let directory = &state.path;
    let class_path = state.class_path.as_ref().unwrap();
    let jar_path = state.jar_path.as_ref().unwrap();
    launcher_core::launch_game(
        "java",
        version_json,
        directory,
        &directory.join("assets"),
        &acc.account,
        CLIENT_ID,
        "",
        "synth_launcher",
        "0",
        &format!("{class_path}{jar_path}"),
    );
}

pub const CLIENT_ID: &str = "04bc8538-fc3c-4490-9e61-a2b3f4cbcf5c";

#[no_mangle]
pub extern "C" fn get_device_response() -> *mut TaskWrapper<Result<DeviceCodeResponse, Error>> {
    get_task(async { Ok(account::auth::device_response(client(), CLIENT_ID).await?) })
}

#[no_mangle]
/// # Safety
pub unsafe extern "C" fn poll_device_response(
    raw_task: *const TaskWrapper<Result<DeviceCodeResponse, Error>>,
) -> bool {
    poll_task(raw_task)
}

#[no_mangle]
/// # Safety
pub unsafe extern "C" fn await_device_response(
    state: *mut State,
    raw_task: *mut TaskWrapper<Result<DeviceCodeResponse, Error>>,
) -> NativeReturn {
    await_result_task(raw_task, |inner| {
        let state = &mut *state;
        state.device_code = Some(inner);

        NativeReturn::success()
    })
}

#[no_mangle]
/// # Safety
pub unsafe extern "C" fn get_user_code(state: *mut State) -> RefStringWrapper {
    let state = &*state;
    if let Some(code) = &state.device_code {
        code.user_code.as_str().into()
    } else {
        panic!()
    }
}

#[no_mangle]
/// # Safety
pub unsafe extern "C" fn get_url(state: *mut State) -> RefStringWrapper {
    let state = &*state;
    if let Some(code) = &state.device_code {
        code.verification_uri.as_str().into()
    } else {
        panic!()
    }
}

#[no_mangle]
/// # Safety
pub unsafe extern "C" fn start_auth_loop(
    state: *mut State,
) -> *mut TaskWrapper<Result<AccRefreshPair, Error>> {
    let state = &*state;
    get_task(async {
        let device_response = state.device_code.as_ref().unwrap();
        let auth_res = loop {
            let device_code = &device_response.device_code;
            let auth_hook = authorization_token_response(client(), device_code, CLIENT_ID).await;
            if let Ok(t) = auth_hook {
                break t;
            }
        };
        auth(auth_res).await
    })
}

#[no_mangle]
/// # Safety
pub extern "C" fn poll_auth_loop(
    raw_task: *mut TaskWrapper<Result<AccRefreshPair, Error>>,
) -> bool {
    poll_task(raw_task)
}

#[no_mangle]
/// # Safety
pub unsafe extern "C" fn await_auth_loop(
    state: *const State,
    data: *mut LauncherData,
    raw_task: *mut TaskWrapper<Result<AccRefreshPair, Error>>,
) -> NativeReturn {
    await_result_task(raw_task, |inner| {
        let data = &mut *data;
        for account in &mut data.accounts {
            if account.account.profile.id == inner.account.profile.id {
                *account = inner;
                return NativeReturn::success();
            }
        }

        data.accounts.push(inner);
        if let Err(e) = std::fs::write(
            (*state).path.join("launcher_data.toml"),
            toml::to_string_pretty(&data).unwrap().as_bytes(),
        ) {
            return Error::from(e).into();
        };
        NativeReturn::success()
    })
}

#[no_mangle]
/// # Safety
pub unsafe extern "C" fn cancel_auth_loop(
    raw_task: *mut TaskWrapper<Result<AccRefreshPair, Error>>,
) {
    cancel_task(raw_task)
}

#[no_mangle]
/// # Safety
pub unsafe extern "C" fn try_refresh(
    data: *const LauncherData,
    index: usize,
) -> *mut TaskWrapper<Result<(AccRefreshPair, usize), Error>> {
    let data = &*data;
    get_task(async move {
        let guard = data;
        let profile = &guard.accounts[index];

        let refresh = refresh_token_response(client(), &profile.refresh_token, CLIENT_ID).await?;
        auth(refresh).await.map(|a| (a, index))
    })
}

#[no_mangle]
pub extern "C" fn poll_refresh(
    raw_task: *mut TaskWrapper<Result<(AccRefreshPair, usize), Error>>,
) -> bool {
    poll_task(raw_task)
}

#[no_mangle]
/// # Safety
pub unsafe extern "C" fn await_refresh(
    state: *const State,
    data: *mut LauncherData,
    raw_task: *mut TaskWrapper<Result<(AccRefreshPair, usize), Error>>,
) -> NativeReturn {
    await_result_task(raw_task, |(inner, idx)| {
        let data = &mut *data;
        let state = &*state;
        data.accounts[idx] = inner;
        std::fs::write(
            state.path.join("launcher_data.toml"),
            toml::to_string_pretty(&data).unwrap().as_bytes(),
        )
        .unwrap();
        NativeReturn::success()
    })
}

#[no_mangle]
/// # Safety
pub unsafe extern "C" fn accounts_len(data: *mut LauncherData) -> usize {
    (*data).accounts.len()
}

#[no_mangle]
/// # Safety
pub unsafe extern "C" fn remove_account(data: *mut LauncherData, index: usize) {
    (*data).accounts.remove(index);
}

#[no_mangle]
/// # Safety
pub unsafe extern "C" fn get_account_name(
    data: *mut LauncherData,
    index: usize,
) -> RefStringWrapper {
    (*data).accounts[index].account.profile.name.as_str().into()
}

#[no_mangle]
/// # Safety
pub unsafe extern "C" fn needs_refresh(data: *mut LauncherData, index: usize) -> bool {
    let data = &*data;
    data.accounts[index].account.expiry
        <= SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs()
}

#[no_mangle]
/// # Safety
pub unsafe extern "C" fn jvm_len(data: *mut LauncherData) -> usize {
    (*data).jvms.len()
}

#[no_mangle]
/// # Safety
pub unsafe extern "C" fn jvm_name(data: *mut LauncherData, index: usize) -> RefStringWrapper {
    (*data).jvms[index].name.as_str().into()
}

#[no_mangle]
/// # Safety
pub unsafe extern "C" fn add_jvm(
    data: *mut LauncherData,
    ptr: *const u16,
    len: usize,
) -> NativeReturn {
    assert_eq!(ptr.align_offset(std::mem::align_of::<&[u16]>()), 0);
    let string = String::from_utf16(slice::from_raw_parts(ptr, len)).unwrap();
    match get_vendor_major_version(&string) {
        Ok((vendor, version)) => {
            (*data).jvms.push(Jvm {
                path: string,
                name: format!("{vendor} {version}"),
            });
            NativeReturn::success()
        }
        Err(e) => e.into(),
    }
}

#[no_mangle]
/// # Safety
pub unsafe extern "C" fn remove_jvm(data: *mut LauncherData, index: usize) {
    (*data).jvms.remove(index);
}

pub enum JvmError {
    Io(std::io::Error),
    Fail(String),
}

impl From<JvmError> for NativeReturn {
    fn from(value: JvmError) -> Self {
        let code = Code::JvmError;

        let str: &dyn Display = match &value {
            JvmError::Io(e) => e,
            JvmError::Fail(e) => e,
        };

        NativeReturn {
            code,
            error: str.to_string().into(),
        }
    }
}

impl From<std::io::Error> for JvmError {
    fn from(value: std::io::Error) -> Self {
        JvmError::Io(value)
    }
}

fn get_vendor_major_version(jvm: &str) -> Result<(String, u32), JvmError> {
    /// Compiled Java byte-code to check for the current Java Version
    /// Source can be found in VersionPrinter.java
    const CHECKER_CLASS: &[u8] = include_bytes!("VersionPrinter.class");

    let tmp = std::env::temp_dir();
    let checker_class_file = tmp.join("VersionPrinter.class");
    std::fs::write(checker_class_file, CHECKER_CLASS).unwrap();
    let io = std::process::Command::new(jvm)
        .env_clear()
        .current_dir(tmp)
        .args(["-DFile.Encoding=UTF-8", "VersionPrinter"])
        .output()?;

    if !io.stderr.is_empty() {
        return Err(JvmError::Fail(String::from_utf8(io.stderr).unwrap()));
    }

    if !io.status.success() {
        return Err(JvmError::Fail(io.status.to_string()));
    }

    let string = String::from_utf8(io.stdout).unwrap();

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

    Ok((name, version))
}

async fn auth(auth_res: AuthorizationTokenResponse) -> Result<AccRefreshPair, Error> {
    let xbox_response = xbox_response(client(), &auth_res.access_token).await?;

    let xbox_secure_token_res =
        xbox_security_token_response(client(), &xbox_response.token).await?;

    let claims = &xbox_secure_token_res.display_claims;
    let token = &xbox_secure_token_res.token;
    let mc_res = minecraft_response(claims, token, client()).await?;

    // This is literally not worth checking lol, the next endpoint will do it but better
    // let ownership_check = minecraft_ownership_response(&mc_res.access_token, client()).await?;

    let profile = minecraft_profile_response(&mc_res.access_token, client()).await?;

    Ok(profile_to_account(profile, auth_res, mc_res))
}

fn profile_to_account(
    profile: Profile,
    auth_res: AuthorizationTokenResponse,
    mc_res: MinecraftAuthenticationResponse,
) -> AccRefreshPair {
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

    AccRefreshPair {
        account,
        refresh_token: auth_res.refresh_token,
    }
}

#[no_mangle]
unsafe extern "C" fn read_data(
    state: *const State,
) -> *mut TaskWrapper<Result<LauncherData, Error>> {
    let state = &*state;
    get_task(async {
        let path = state.path.join("launcher_data.toml");
        let exists = path.exists() && path.is_file();
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(path)
            .await?;
        if exists {
            let mut string = String::with_capacity(file.metadata().await?.len() as usize);
            file.read_to_string(&mut string).await?;
            Ok(toml::from_str(&string)?)
        } else {
            let default = LauncherData::default();
            file.write_all(toml::to_string_pretty(&default).unwrap().as_bytes())
                .await?;
            Ok(default)
        }
    })
}

#[no_mangle]
unsafe extern "C" fn poll_data(raw_task: *mut TaskWrapper<Result<LauncherData, Error>>) -> bool {
    poll_task(raw_task)
}

/// If this is a success, we smuggle the pointer through the error
#[no_mangle]
unsafe extern "C" fn await_data(
    raw_task: *mut TaskWrapper<Result<LauncherData, Error>>,
) -> NativeReturn {
    await_task(raw_task, |inner| match inner {
        Ok(v) => NativeReturn {
            code: Code::Success,
            error: OwnedStringWrapper {
                char_ptr: Box::into_raw(Box::new(v)) as *mut _,
                len: 0,
                capacity: 0,
            },
        },
        Err(e) => e.into(),
    })
}
