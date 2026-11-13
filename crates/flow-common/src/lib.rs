//! Shared building blocks for FlowDE daemons.
//!
//! Everything here is portable (builds and tests on macOS); only the runtime
//! behaviour of the daemons built on top is Linux specific.

pub mod config;
pub mod error;
pub mod events;
pub mod health;
pub mod ipc;
pub mod logging;
pub mod paths;
pub mod time;

pub use error::{FlowError, Result};

pub mod daemon;
