//! Governance handlers for daemon requests.
//!
//! LEGACY-CONTRACT-MIGRATION-1C: Governance family handlers.
//!
//! Module structure:
//! - `assess` — quality policy assessment (WRITE operation)
//! - `violations` — architectural violations (declared + discovered)

mod assess;
mod violations;

#[cfg(test)]
mod tests;

pub use assess::handle_assess;
pub use violations::handle_violations;
