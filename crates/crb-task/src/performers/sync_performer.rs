use crate::hybryd_task::{
    DoHybrid, HybrydSession, HybrydState, HybrydTask, NextState, StatePerformer, Transition,
};
use anyhow::{Error, Result};
use async_trait::async_trait;
use crb_runtime::kit::Interruptor;
use std::marker::PhantomData;
use tokio::task::spawn_blocking;

impl<T> NextState<T>
where
    T: HybrydTask,
{
    pub fn do_sync<S>(state: S) -> Self
    where
        T: SyncActivity<S>,
        S: HybrydState,
    {
        let performer = SyncPerformer {
            _task: PhantomData,
            state: Some(state),
        };
        Self::new(performer)
    }
}

pub trait SyncActivity<S>: HybrydTask {
    fn perform(&mut self, mut state: S, interruptor: Interruptor) -> Result<NextState<Self>> {
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
        Ok(NextState::interrupt(None))
    }

    fn many(&mut self, state: &mut S) -> Result<Option<NextState<Self>>> {
        self.once(state).map(Some)
    }

    fn once(&mut self, _state: &mut S) -> Result<NextState<Self>> {
        Ok(NextState::done())
    }

    fn repair(&mut self, err: Error) -> Result<(), Error> {
        Err(err)
    }

    fn fallback(&mut self, err: Error) -> NextState<Self> {
        NextState::fail(err)
    }
}

struct SyncPerformer<T, S> {
    _task: PhantomData<T>,
    state: Option<S>,
}

#[async_trait]
impl<T, S> StatePerformer<T> for SyncPerformer<T, S>
where
    T: SyncActivity<S>,
    S: HybrydState,
{
    async fn perform(&mut self, mut task: T, session: &mut HybrydSession) -> Transition<T> {
        let interruptor = session.controller.interruptor.clone();
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

    async fn fallback(&mut self, mut task: T, err: Error) -> (T, NextState<T>) {
        let next_state = task.fallback(err);
        (task, next_state)
    }
}

impl DoHybrid<SyncFn> {
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

impl HybrydTask for SyncFn {
    fn initial_state(&mut self) -> NextState<Self> {
        NextState::do_sync(CallFn)
    }
}

struct CallFn;

impl SyncActivity<CallFn> for SyncFn {
    fn once(&mut self, _state: &mut CallFn) -> Result<NextState<Self>> {
        let func = self.func.take().unwrap();
        func()?;
        Ok(NextState::done())
    }
}