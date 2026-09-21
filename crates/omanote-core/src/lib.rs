//! Omanote core: Joplin-compatible data model, E2EE, sync and local storage.

pub mod e2ee;
pub mod error;
pub mod item;

pub use error::{Error, Result};
pub mod api;
pub mod store;
pub mod sync;
