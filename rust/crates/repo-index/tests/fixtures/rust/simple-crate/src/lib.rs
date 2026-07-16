// Fixture: simple Rust crate for extractor testing.

use std::collections::HashMap;

pub struct Config {
    pub name: String,
    pub values: HashMap<String, i32>,
}

pub enum Status {
    Active,
    Inactive,
}

pub trait Processor {
    fn process(&self, input: &str) -> String;
}

impl Config {
    pub fn new(name: String) -> Self {
        Self {
            name,
            values: HashMap::new(),
        }
    }

    pub fn get_value(&self, key: &str) -> Option<&i32> {
        self.values.get(key)
    }
}

impl Processor for Config {
    fn process(&self, input: &str) -> String {
        format!("{}: {}", self.name, input)
    }
}

pub fn create_config(name: &str) -> Config {
    Config::new(name.to_string())
}

const MAX_SIZE: usize = 1024;

fn helper() -> Vec<String> {
    Vec::new()
}
