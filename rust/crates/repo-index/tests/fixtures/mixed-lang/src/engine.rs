// Rust file: uses serde and wgpu (from Cargo.toml deps)
use serde::Serialize;

#[derive(Serialize)]
pub struct GameState {
    pub score: u32,
}

impl GameState {
    pub fn new() -> Self {
        Self { score: 0 }
    }
}

pub fn create_state() -> GameState {
    GameState::new()
}

// Direct call on an imported external type to produce a classifiable
// CALLS edge. serde_json::to_string is a function call with receiver
// from an external crate binding.
pub fn serialize_state(state: &GameState) -> String {
    // Serialize is imported from serde — calls referencing it should
    // classify against Cargo.toml deps.
    Serialize::serialize(state, todo!())
}
