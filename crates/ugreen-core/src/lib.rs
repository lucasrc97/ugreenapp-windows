//! Protocol and transport layer shared by the UGREEN CLI and GUI.

pub mod discovery;
pub mod protocol;
pub mod transport;

#[cfg(test)]
mod golden_tests;
#[cfg(test)]
mod real_captures;
