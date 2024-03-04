use crate::{runtime, NativeReturn};
use std::future::Future;
use std::mem::ManuallyDrop;
use tokio::task::JoinHandle;

pub struct TaskWrapper<T> {
    pub inner: ManuallyDrop<JoinHandle<T>>,
}

impl<T> TaskWrapper<T> {
    pub fn new<F>(t: F) -> Self
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        Self {
            inner: ManuallyDrop::new(runtime().spawn(t)),
        }
    }

    pub fn into_raw(self) -> *mut Self {
        Box::into_raw(Box::new(self))
    }
}

#[inline]
fn check_task_ptr<T>(task: *const TaskWrapper<T>) {
    assert!(!task.is_null());
    assert_eq!(
        task.align_offset(std::mem::align_of::<*const TaskWrapper<T>>()),
        0
    );
}

pub fn get_task<F, T>(f: F) -> *mut TaskWrapper<T>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    TaskWrapper::new(f).into_raw()
}

pub fn poll_task<T>(raw_task: *const TaskWrapper<T>) -> bool
where
    T: 'static,
{
    check_task_ptr(raw_task);

    unsafe { raw_task.as_ref().unwrap().inner.is_finished() }
}

pub fn await_task<T, F: Fn(T) -> NativeReturn>(
    raw_task: *mut TaskWrapper<T>,
    f: F,
) -> NativeReturn {
    check_task_ptr(raw_task);
    let task = unsafe { Box::from_raw(raw_task) };

    let inner = runtime()
        .block_on(ManuallyDrop::into_inner(task.inner))
        .unwrap();

    f(inner)
}

pub fn cancel_task<T>(raw_task: *mut TaskWrapper<T>)
where
    T: 'static,
{
    check_task_ptr(raw_task);
    let task = unsafe { Box::from_raw(raw_task) };
    task.inner.abort();
}
