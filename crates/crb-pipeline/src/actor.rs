use crate::{MessageToRoute, Pipeline, RuntimeGenerator};
use async_trait::async_trait;
use crb_actor::runtime::ActorRuntime;
use crb_actor::{Actor, Address};
use crb_runtime::{Interruptor, Runtime};
use std::marker::PhantomData;

// TODO: Implement
// - Metadata for all messages
// - Epochs (metadata)
// - Split (meta)
// - route_map
// - route_split
// - route_merge

pub struct ActorRuntimeGenerator<A> {
    _type: PhantomData<A>,
}

impl<A> ActorRuntimeGenerator<A>
where
    A: ConductedActor,
{
    pub fn new<M>() -> impl RuntimeGenerator<Input = M>
    where
        A: ConductedActor<Input = M>,
    {
        Self { _type: PhantomData }
    }
}

unsafe impl<A> Sync for ActorRuntimeGenerator<A> {}

impl<A> RuntimeGenerator for ActorRuntimeGenerator<A>
where
    A: ConductedActor,
{
    type Input = A::Input;

    fn generate(&self, pipeline: Address<Pipeline>, input: Self::Input) -> Box<dyn Runtime> {
        let actor = A::input(input);
        let runtime = ActorRuntime::new(actor);
        let conducted_runtime = ConductedActorRuntime::<A> { pipeline, runtime };
        Box::new(conducted_runtime)
    }
}

// TODO: Replace with flexible `From` and `Into` pair
pub trait ConductedActor: Actor<Context: Default> {
    type Input: Send;
    type Output: Clone + Sync + Send;

    fn input(input: Self::Input) -> Self;
    fn output(&mut self) -> Self::Output;
}

pub trait Stage<IN, OUT> {
    fn from_input(input: IN) -> Self;
    fn to_output(self) -> OUT;
}

impl<T, IN, OUT> Stage<IN, OUT> for T
where
    T: From<IN>,
    T: Into<OUT>,
{
    fn from_input(input: IN) -> Self {
        input.into()
    }

    fn to_output(self) -> OUT {
        self.into()
    }
}

pub struct ConductedActorRuntime<A: ConductedActor> {
    pipeline: Address<Pipeline>,
    runtime: ActorRuntime<A>,
}

#[async_trait]
impl<A> Runtime for ConductedActorRuntime<A>
where
    A: ConductedActor,
    A::Context: Default,
{
    fn get_interruptor(&mut self) -> Interruptor {
        self.runtime.get_interruptor()
    }

    async fn routine(&mut self) {
        self.runtime.routine().await;
        let message = self.runtime.actor.output();
        let msg = MessageToRoute::<A> { message };
        let res = self.pipeline.send(msg);
        self.runtime.failures.put(res);
    }
}
