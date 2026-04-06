// Fixture for enrichment integration test.
// Contains known receiver types for rust-analyzer to resolve.

use std::collections::HashMap;

pub struct GameState {
    pub players: HashMap<String, Player>,
    pub round: u32,
}

pub struct Player {
    pub name: String,
    pub score: u32,
}

impl Player {
    pub fn new(name: String) -> Self {
        Self { name, score: 0 }
    }

    pub fn add_score(&mut self, points: u32) {
        self.score += points;
    }
}

impl GameState {
    pub fn new() -> Self {
        Self {
            players: HashMap::new(),
            round: 0,
        }
    }

    pub fn add_player(&mut self, name: String) {
        let player = Player::new(name.clone());
        self.players.insert(name, player);
    }

    pub fn get_scores(&self) -> Vec<(String, u32)> {
        self.players
            .iter()
            .map(|(name, player)| (name.clone(), player.score))
            .collect()
    }
}

// Free function with local variable receivers — the main enrichment target.
pub fn process_game(state: &mut GameState) {
    let scores = state.get_scores();
    let mut names: Vec<String> = scores.iter().map(|(n, _)| n.clone()).collect();
    names.sort();
    names.dedup();
}
