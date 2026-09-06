// Crate `b`: the crate imported by `a`.
pub mod util;

pub struct Thing {
    label: String,
}

impl Thing {
    pub fn new() -> Self {
        Self {
            label: "thing".to_string(),
        }
    }

    pub fn label(&self) -> &str {
        &self.label
    }
}
