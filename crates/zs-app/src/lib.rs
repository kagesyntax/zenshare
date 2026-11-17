//! The reusable half of the application: engine, paths, and their wiring.
//!
//! The desktop binary (`src/main.rs`) and the Android shared library
//! (`zs-android`) both sit on top of this. Nothing here knows what a window is,
//! which is why the same engine serves a desktop app and a phone.

#![deny(missing_docs)]

pub mod engine;
pub mod paths;
