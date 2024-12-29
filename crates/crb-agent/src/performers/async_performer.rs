use crate::context::{AgentContext, AgentSession};
use crate::runtime::{
    RunAgent,
    AgentState, Next, StatePerformer, Transition,
};
use crate::agent::{Agent, Output};
use anyhow::{Error, Result};
use async_trait::async_trait;
use crb_runtime::kit::Interruptor;
use futures::Future;
use std::marker::PhantomData;

impl<T> Next<T>
where
    T: Agent,
{
    pub fn do_async<S>(state: S) -> Self
    where
        T: DoAsync<S>,
        S: AgentState,
    {
        let performer = AsyncPerformer {
            _task: PhantomData,
            state: Some(state),
        };
        Self::new(performer)
    }
}

#[async_trait]
pub trait DoAsync<S: Send + 'static>: Agent {
    async fn perform(&mut self, mut state: S, interruptor: Interruptor) -> Result<Next<Self>> {
        while interruptor.is_active() {
            let result = self.many(&mut state).await;
            match result {
                Ok(Some(state)) => {
                    return Ok(state);
                }
                Ok(None) => {}
                Err(_) => {}
            }
        }
        Ok(Next::interrupt(None))
    }

    async fn many(&mut self, state: &mut S) -> Result<Option<Next<Self>>> {
        self.once(state).await.map(Some)
    }

    async fn once(&mut self, _state: &mut S) -> Result<Next<Self>> {
        Ok(Next::done())
    }

    async fn repair(&mut self, err: Error) -> Result<(), Error> {
        Err(err)
    }

    async fn fallback(&mut self, err: Error) -> Next<Self> {
        Next::fail(err)
    }
}

struct AsyncPerformer<T, S> {
    _task: PhantomData<T>,
    state: Option<S>,
}

#[async_trait]
impl<T, S> StatePerformer<T> for AsyncPerformer<T, S>
where
    T: DoAsync<S>,
    S: AgentState,
{
    async fn perform(&mut self, mut task: T, ctx: &mut T::Context) -> Transition<T> {
        let interruptor = ctx.session().controller.interruptor.clone();
        let state = self.state.take().unwrap();
        let next_state = task.perform(state, interruptor).await;
        Transition::Next(task, next_state)
    }

    async fn fallback(&mut self, mut task: T, err: Error) -> (T, Next<T>) {
        let next_state = task.fallback(err).await;
        (task, next_state)
    }
}

impl<T> RunAgent<AsyncFn<T>>
where
    T: Output,
{
    pub fn new_async<F: AnyAsyncFut<T>>(fut: F) -> Self {
        let task = AsyncFn::<T> {
            fut: Some(Box::new(fut)),
            output: None,
        };
        Self::new(task)
    }
}

pub trait AnyAsyncFut<T>: Future<Output = Result<T>> + Send + 'static {}

impl<F, T> AnyAsyncFut<T> for F where F: Future<Output = Result<T>> + Send + 'static {}

struct AsyncFn<T> {
    fut: Option<Box<dyn AnyAsyncFut<T>>>,
    output: Option<T>,
}

impl<T> Agent for AsyncFn<T>
where
    T: Output,
{
    type Context = AgentSession<Self>;
    // TODO: Get an output from Fn
    type Output = T;

    fn initialize(&mut self, _ctx: &mut Self::Context) -> Next<Self> {
        Next::do_async(CallFn)
    }

    fn finalize(mut self, _ctx: &mut Self::Context) -> Self::Output {
        self.output.unwrap_or_default()
    }
}

struct CallFn;

#[async_trait]
impl<T> DoAsync<CallFn> for AsyncFn<T>
where
    T: Output,
{
    async fn once(&mut self, _state: &mut CallFn) -> Result<Next<Self>> {
        let fut = self.fut.take().unwrap();
        let pinned_fut = Box::into_pin(fut);
        let output = pinned_fut.await?;
        self.output = Some(output);
        Ok(Next::done())
    }
}
