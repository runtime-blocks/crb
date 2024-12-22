use crate::runtime::ActorContext;
use anyhow::Error;
use async_trait::async_trait;
use crb_runtime::kit::ManagedContext;

#[async_trait]
pub trait Actor: Sized + Send + 'static {
    type Context: ActorContext<Self>;

    async fn initialize(&mut self, _ctx: &mut Self::Context) -> Result<(), Error> {
        Ok(())
    }

    async fn interrupt(&mut self, ctx: &mut Self::Context) -> Result<(), Error> {
        // Closes the channel
        ctx.session().shutdown();
        Ok(())
    }

    async fn event(&mut self, ctx: &mut Self::Context) -> Result<(), Error> {
        let envelope = ctx.session().joint().next_envelope();
        if let Some(envelope) = envelope.await {
            envelope.handle(self, ctx).await?;
        } else {
            // Terminates the runtime when the channel has drained
            ctx.session().controller().stop(false)?;
        }
        Ok(())
    }

    async fn finalize(&mut self, _ctx: &mut Self::Context) -> Result<(), Error> {
        Ok(())
    }
}