use crate::context::{AgentContext, AgentSession};
use crate::runtime::{
    RunAgent,
    AgentState, Next, StatePerformer, Transition,
};
use crate::agent::{ Agent };
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

impl RunAgent<SyncFn> {
    pub fn new_sync<F: AnySyncFn>(func: F) -> Self {
        let task = SyncFn {
            func: Some(Box::new(func)),
        };
        Self::new(task)
    }
}

pub trait AnySyncFn: FnOnce() -> Result<()> + Send + 'static {}

impl<F> AnySyncFn for F where F: FnOnce() -> Result<()> + Send + 'static {}

struct SyncFn {
    func: Option<Box<dyn AnySyncFn>>,
}

impl Agent for SyncFn {
    type Context = AgentSession<Self>;
    // TODO: Get an output from Fut
    type Output = ();

    fn initialize(&mut self, _ctx: &mut Self::Context) -> Next<Self> {
        Next::do_sync(CallFn)
    }
}

struct CallFn;

impl DoSync<CallFn> for SyncFn {
    fn once(&mut self, _state: &mut CallFn) -> Result<Next<Self>> {
        let func = self.func.take().unwrap();
        func()?;
        Ok(Next::done())
    }
}
