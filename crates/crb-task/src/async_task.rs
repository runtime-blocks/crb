use anyhow::{Error, Result};
use async_trait::async_trait;
use crb_core::JoinHandle;
use crb_runtime::kit::{Controller, Entrypoint, Failures, Interruptor, Runtime};
use derive_more::{Deref, DerefMut};
use futures::{stream::Abortable, Future};
use std::marker::PhantomData;

#[async_trait]
pub trait Task: Send + 'static {
    async fn controlled_routine(&mut self, ctrl: &mut Controller) -> Result<()> {
        let reg = ctrl.take_registration()?;
        let fut = self.routine();
        Abortable::new(fut, reg).await??;
        Ok(())
    }

    async fn routine(&mut self) -> Result<()> {
        Ok(())
    }
}

pub struct TaskRuntime<T> {
    pub task: Option<T>,
    pub controller: Controller,
    pub failures: Failures,
}

impl<T: Task> TaskRuntime<T> {
    pub fn new(task: T) -> Self {
        Self {
            task: Some(task),
            controller: Controller::default(),
            failures: Failures::default(),
        }
    }
}

#[async_trait]
impl<T> Runtime for TaskRuntime<T>
where
    T: Task,
{
    fn get_interruptor(&mut self) -> Interruptor {
        self.controller.interruptor.clone()
    }

    async fn routine(&mut self) {
        if let Some(mut task) = self.task.take() {
            let res = task.controlled_routine(&mut self.controller).await;
            self.failures.put(res);
        }
    }
}

#[derive(Deref, DerefMut)]
pub struct TypedTask<T> {
    #[deref]
    #[deref_mut]
    task: TypelessTask,
    _run: PhantomData<T>,
}

impl<T: Task> TypedTask<T> {
    pub fn spawn(task: T) -> Self {
        let mut runtime = TaskRuntime::new(task);
        let interruptor = runtime.get_interruptor();
        let handle = crb_core::spawn(runtime.entrypoint());
        let task = TypelessTask {
            interruptor,
            handle,
            cancel_on_drop: false,
        };
        Self {
            task,
            _run: PhantomData,
        }
    }
}

impl<T> From<TypedTask<T>> for TypelessTask {
    fn from(typed: TypedTask<T>) -> Self {
        typed.task
    }
}

pub struct TypelessTask {
    interruptor: Interruptor,
    handle: JoinHandle<()>,
    cancel_on_drop: bool,
}

impl TypelessTask {
    pub fn cancel_on_drop(&mut self, cancel: bool) {
        self.cancel_on_drop = cancel;
    }

    pub fn interrupt(&mut self) {
        self.interruptor.stop(true).ok();
    }
}

impl Drop for TypelessTask {
    fn drop(&mut self) {
        if self.cancel_on_drop {
            self.handle.abort();
        }
    }
}

impl TypelessTask {
    pub fn spawn<T>(fut: T) -> Self
    where
        T: Future<Output = Result<()>>,
        T: Send + 'static,
    {
        let task = FnTask { fut: Some(fut) };
        TypedTask::spawn(task).into()
    }
}

struct FnTask<T> {
    fut: Option<T>,
}

#[async_trait]
impl<T> Task for FnTask<T>
where
    T: Future<Output = Result<()>>,
    T: Send + 'static,
{
    async fn routine(&mut self) -> Result<()> {
        self.fut
            .take()
            .ok_or_else(|| Error::msg("Future has taken already"))?
            .await
    }
}
