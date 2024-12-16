//! A context for composable blocks.

use crate::interruptor::Controller;

/// A commont methods of all contexts and spans for tracing and logging.
///
/// The have provide a reference to a label.
pub trait Context: Send {
    /// An address to interact with the context.
    type Address: Send + Clone;

    // TODO: A label that used for logging all events around the context.
    // fn label(&self) -> &Label;

    /// A reference to an address.
    fn address(&self) -> &Self::Address;
}

/// The main features of composable block's context.
///
/// It could be interrupted and contains a method to check a life status of a composable block.
pub trait ManagedContext: Context {
    fn controller(&mut self) -> &mut Controller;
    /// Marks a context as interrupted.
    fn shutdown(&mut self);
}
