//! Platform-independent recording model.

pub mod capture;
pub mod clock;
pub mod recorder;
pub mod session;
pub mod state;

#[cfg(test)]
mod tests;
