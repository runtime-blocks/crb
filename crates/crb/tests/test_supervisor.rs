use anyhow::Result;
use crb::agent::{Agent, AgentSession, ManagedContext, Next, Standalone, SupervisorSession};
use crb::superagent::{Relation, Supervisor};

#[derive(Default)]
struct TestSupervisor {
    respawned_once: bool,
}

impl Standalone for TestSupervisor {}

impl Supervisor for TestSupervisor {
    type GroupBy = ();

    fn finished(&mut self, _rel: &Relation<Self>, ctx: &mut Self::Context) {
        if !self.respawned_once {
            self.respawned_once = true;
            ctx.spawn_agent(Child, ());
        } else if ctx.tracker.is_empty() {
            println!("Supervisor: I'm alone 🥹");
            ctx.shutdown();
        }
    }
}

impl Agent for TestSupervisor {
    type Context = SupervisorSession<Self>;
    type Output = ();

    fn initialize(&mut self, ctx: &mut Self::Context) -> Next<Self> {
        ctx.spawn_agent(Child, ());
        Next::events()
    }
}

struct Child;

impl Agent for Child {
    type Context = AgentSession<Self>;
    type Output = ();

    fn initialize(&mut self, ctx: &mut Self::Context) -> Next<Self> {
        println!("A child: has been spawned!");
        ctx.shutdown();
        Next::events()
    }
}

#[tokio::test]
async fn test_supervisor() -> Result<()> {
    let mut addr = TestSupervisor::default().spawn();
    addr.join().await?;
    Ok(())
}
