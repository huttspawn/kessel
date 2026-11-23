//! Turns decoded game objects into typed knowledge, one module per domain.
//! Opens no database and holds no SQL; references it cannot resolve are returned unresolved.

pub mod ability;
pub mod achievement;
pub mod conquest;
pub mod discipline;
pub mod gsf;
pub mod item;
pub mod npc;
pub mod season;
pub mod talent;
pub mod venture;
