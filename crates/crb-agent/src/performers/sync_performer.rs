use crate::context::{AgentContext, AgentSession};
use crate::runtime::{
    RunAgent,
    AgentState, Next, StatePerformer, Transition,
};
use crate::agent::{Agent, Output};
use anyhow::{Error, Result};
use async_trait::async_trait;
use crb_runtime::kit::Interruptor;
use std::marker::PhantomData;
use tokio::task::spawn_blocking;

impl<T> Next<T>
where
    T: Agent,
{
    pub fn do_sync<S>(state: S) -> Self
    where
        T: DoSync<S>,
        S: AgentState,
    {
        let performer = SyncPerformer {
            _task: PhantomData,
            state: Some(state),
        };
        Self::new(performer)
    }
}

pub trait DoSync<S>: Agent {
    fn perform(&mut self, mut state: S, interruptor: Interruptor) -> Result<Next<Self>> {
        while interruptor.is_active() {
            let result = self.many(&mut state);
            match result {
                Ok(Some(state)) => {
                    return Ok(state);
                }
                Ok(None) => {}
                Err(err) => {
                    self.repair(err)?;
                }
            }
        }
        Ok(Next::interrupt(None))
    }

    fn many(&mut self, state: &mut S) -> Result<Option<Next<Self>>> {
        self.once(state).map(Some)
    }

    fn once(&mut self, _state: &mut S) -> Result<Next<Self>> {
        Ok(Next::done())
    }

    fn repair(&mut self, err: Error) -> Result<(), Error> {
        Err(err)
    }

    fn fallback(&mut self, err: Error) -> Next<Self> {
        Next::fail(err)
    }
}

struct SyncPerformer<T, S> {
    _task: PhantomData<T>,
    state: Option<S>,
}

#[async_trait]
impl<T, S> StatePerformer<T> for SyncPerformer<T, S>
where
    T: DoSync<S>,
    S: AgentState,
{
    async fn perform(&mut self, mut task: T, ctx: &mut T::Context) -> Transition<T> {
        let interruptor = ctx.session().controller.interruptor.clone();
        let state = self.state.take().unwrap();
        let handle = spawn_blocking(move || {
            let next_state = task.perform(state, interruptor);
            Transition::Next(task, next_state)
        });
        match handle.await {
            Ok(transition) => transition,
            Err(err) => Transition::Crashed(err.into()),
        }
    }

    async fn fallback(&mut self, mut task: T, err: Error) -> (T, Next<T>) {
        let next_state = task.fallback(err);
        (task, next_state)
    }
}

impl<T: Output> RunAgent<SyncFn<T>> {
    pub fn new_sync<F: AnySyncFn<T>>(func: F) -> Self {
        let task = SyncFn::<T> {
            func: Some(Box::new(func)),
            output: None,
        };
        Self::new(task)
    }
}

pub trait AnySyncFn<T>: FnOnce() -> T + Send + 'static {}

impl<F, T> AnySyncFn<T> for F where F: FnOnce() -> T + Send + 'static {}

struct SyncFn<T> {
    func: Option<Box<dyn AnySyncFn<T>>>,
    output: Option<T>,
}

impl<T: Output> Agent for SyncFn<T> {
    type Context = AgentSession<Self>;
    type Output = T;

    fn initialize(&mut self, _ctx: &mut Self::Context) -> Next<Self> {
        Next::do_sync(CallFn)
    }

    fn finalize(self, _ctx: &mut Self::Context) -> Self::Output {
        self.output.unwrap_or_default()
    }
}

struct CallFn;

impl<T: Output> DoSync<CallFn> for SyncFn<T> {
    fn once(&mut self, _state: &mut CallFn) -> Result<Next<Self>> {
        let func = self.func.take().unwrap();
        let output = func();
        self.output = Some(output);
        Ok(Next::done())
    }
}
