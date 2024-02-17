use std::future::Future;
use tokio::task::JoinHandle;
use launcher_core::Error;
use launcher_core::types::VersionManifest;
use crate::{NativeReturn, runtime};

pub struct TaskWrapper<T> {
    pub inner: Option<JoinHandle<T>>,
}

impl<T> TaskWrapper<T> {
    pub fn new<F>(t: F) -> Self
        where
            F: Future<Output = T> + Send + 'static,
            T: Send + 'static,
    {
        Self {
            inner: Some(runtime().spawn(t)),
        }
    }

    pub fn into_raw(self) -> *mut Self {
        Box::into_raw(Box::new(self))
    }
}

/// This exists so that task types can be checked on the C# side of the codebase
pub struct ManifestTaskWrapper;
/// This exists so I can type cast easier
pub type ManifestTask = TaskWrapper<Result<VersionManifest, Error>>;

#[inline]
fn check_task_ptr<T>(task: *const TaskWrapper<T>) {
    assert!(!task.is_null());
    assert_eq!(task.align_offset(std::mem::align_of::<*const TaskWrapper<T>>()), 0);
}

/// Because of type erasure, the compiler doesn't know if the pointer that's dereference is the right type
/// The solution to this is multiple types that the C code can hold
/// This checks for null, and panics, otherwise returning a mutable reference
pub fn read_task_mut<T>(task: *mut TaskWrapper<T>) -> &'static mut TaskWrapper<T> {
    check_task_ptr(task);

    unsafe {
        task.as_mut().unwrap_unchecked()
    }
}

/// Because of type erasure, the compiler doesn't know if the pointer that's dereference is the right type
/// The solution to this is multiple types that the C code can hold
/// This checks for null, and panics, otherwise returning a mutable reference
pub fn read_task_ref<T>(task: *const TaskWrapper<T>) -> &'static TaskWrapper<T> {
    check_task_ptr(task);

    unsafe {
        task.as_ref().unwrap_unchecked()
    }
}

pub fn get_task<F, T>(f: F) -> *mut TaskWrapper<T> where F: Future<Output = T> + Send + 'static, T: Send + 'static {
    TaskWrapper::new(f).into_raw()
}

pub fn poll_task<T>(task: *const TaskWrapper<T>) -> bool where T: 'static {
    check_task_ptr(task);

    read_task_ref(task).inner.as_ref().unwrap().is_finished()
}

pub fn drop_task<T>(task: *mut TaskWrapper<T>) {
    check_task_ptr(task);

    unsafe {
        drop(Box::from_raw(task))
    }
}

pub fn await_task<T>(task: *mut TaskWrapper<T>, f: fn(inner: T) -> NativeReturn) -> NativeReturn where T: 'static {
    check_task_ptr(task);
    let task = read_task_mut(task);

    let inner = runtime().block_on(task.inner.take().unwrap()).unwrap();
    drop_task(task);

    f(inner)
}

pub fn cancel_task<T>(task: *mut TaskWrapper<T>) where T: 'static {
    check_task_ptr(task);
    read_task_mut(task).inner.take().unwrap().abort();
    unsafe {
        drop(Box::from_raw(task))
    }
}
