use crate::runtime::RoutineContext;
use anyhow::Error;
use async_trait::async_trait;
use crb_core::time::{sleep, timeout, Duration, Elapsed};
use crb_runtime::kit::{ManagedContext, RegistrationTaken};
use futures::stream::{Abortable, Aborted};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TaskError {
    #[error("task was aborted")]
    Aborted(#[from] Aborted),
    #[error("task was interrupted")]
    Interrupted,
    #[error("time for task execution elapsed")]
    Timeout(#[from] Elapsed),
    #[error("can't register a task: {0}")]
    Registration(#[from] RegistrationTaken),
    #[error("task failed: {0}")]
    Failed(#[from] Error),
}

#[async_trait]
pub trait Routine: Sized + Send + 'static {
    type Context: RoutineContext<Self>;
    type Output: Send;

    async fn routine(&mut self, ctx: &mut Self::Context) -> Result<(), Error> {
        let reg = ctx.session().controller().take_registration()?;
        // TODO: Get time limit from the context (and make it ajustable in real-time)
        let time_limit = self.time_limit().await;
        let fut = timeout(time_limit, self.basic_routine(ctx));
        Abortable::new(fut, reg).await???;
        Ok(())
    }

    async fn basic_routine(&mut self, ctx: &mut Self::Context) -> Result<(), Error> {
        let output = self.interruptable_routine(ctx).await;
        self.finalize(output, ctx).await?;
        Ok(())
    }

    async fn interruptable_routine(
        &mut self,
        ctx: &mut Self::Context,
    ) -> Result<Self::Output, TaskError> {
        while ctx.session().controller().is_active() {
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
        Err(TaskError::Interrupted)
    }

    async fn repeatable_routine(&mut self) -> Result<Option<Self::Output>, Error> {
        Ok(None)
    }

    async fn finalize(
        &mut self,
        output: Result<Self::Output, TaskError>,
        ctx: &mut Self::Context,
    ) -> Result<(), Error> {
        if let Some(mut finalizer) = ctx.session().take_finalizer() {
            finalizer.finalize(output).await?;
        };
        Ok(())
    }

    // TODO: Use context instead
    async fn time_limit(&mut self) -> Option<Duration> {
        None
    }

    async fn routine_wait(&mut self, _succeed: bool, ctx: &mut Self::Context) {
        let duration = ctx.session().interval();
        sleep(duration).await
    }
}