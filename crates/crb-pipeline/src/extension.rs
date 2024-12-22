use crate::{DistributableMessage, InitialMessage, Pipeline};
use anyhow::Result;
use crb_actor::Address;

pub trait AddressExt {
    fn ingest<M>(&mut self, message: M) -> Result<()>
    where
        M: DistributableMessage;
}

impl AddressExt for Address<Pipeline> {
    fn ingest<M>(&mut self, message: M) -> Result<()>
    where
        M: DistributableMessage,
    {
        let message = InitialMessage::new(message);
        self.send(message)
    }
}