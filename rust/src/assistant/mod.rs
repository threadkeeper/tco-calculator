//! Host-owned assistant runtime.
//!
//! The application owns identity, owner scope, budgets, tool dispatch, and every side effect.
//! Microsoft Foundry supplies inference only, and its output remains untrusted until a typed
//! host component validates it.

pub mod budget;
pub mod context;
pub mod foundry;
pub mod help;
pub mod image;
pub mod model;
pub mod policy;
pub mod tools;
pub mod turn;
