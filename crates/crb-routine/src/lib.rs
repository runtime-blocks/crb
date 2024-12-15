mod runtime;

use anyhow::Error;
use async_trait::async_trait;
use crb_core::{
    time::{sleep, timeout, Duration},
    watch,
};
use crb_runtime::context::Context;
use crb_runtime::interruptor::BasicController;
use futures::{
    future::{select, Either},
    stream::{Abortable, Aborted},
    FutureExt,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TaskError {
    #[error("task was aborted")]
    Aborted(#[from] Aborted),
    #[error("task was interrupted")]
    Interrupted,
    #[error("time for task execution elapsed")]
    Timeout,
    #[error("task failed: {0}")]
    Failed(#[from] Error),
}

#[async_trait]
pub trait Routine: Sized + Send + 'static {
    type Context: Context + AsMut<TaskContext>;
    type Output: Send;

    async fn routine(&mut self, ctx: &mut Self::Context) -> Result<Self::Output, Error> {
        let reg = ctx.as_mut().controller.take_registration()?;
        // TODO: Get time limit from the context (and make it ajustable in real-time)
        let time_limit = self.time_limit().await;
        let fut = timeout(time_limit, self.interruptable_routine(ctx));
        let output = Abortable::new(fut, reg).await???;
        Ok(output)
    }

    async fn interruptable_routine(
        &mut self,
        ctx: &mut Self::Context,
    ) -> Result<Self::Output, Error> {
        while ctx.as_mut().controller.is_active() {
            let routine_result = self.repeatable_routine().await;
            match routine_result {
                Ok(Some(output)) => {
                    return Ok(output);
                }
                Ok(None) => {
                    self.routine_wait(true, ctx).await;
                }
                Err(err) => {
                    // TODO: Report about the error
                    self.routine_wait(false, ctx).await;
                }
            }
        }
        Err(TaskError::Interrupted.into())
    }

    async fn repeatable_routine(&mut self) -> Result<Option<Self::Output>, Error> {
        Ok(None)
    }

    // TODO: Use context instead
    async fn time_limit(&mut self) -> Option<Duration> {
        None
    }

    async fn routine_wait(&mut self, _succeed: bool, ctx: &mut Self::Context) {
        let duration = ctx.as_mut().interval;
        sleep(duration).await
    }

    async fn finalize(&mut self, result: Result<Self::Output, TaskError>) -> Result<(), Error> {
        result?;
        Ok(())
    }
}

pub struct TaskContext {
    controller: BasicController,
    /// Interval between repeatable routine calls
    interval: Duration,
}

impl TaskContext {
    /// Set repeat interval.
    pub fn set_interval(&mut self, interval: Duration) {
        self.interval = interval;
    }
}