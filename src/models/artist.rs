use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Artist {
    pub id: String,
    pub name: String,
    pub disambiguation: String,
    pub score: i32,
}

impl Default for Artist {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            disambiguation: String::new(),
            score: 0,
        }
    }
}

impl Artist {
    pub fn display_name(&self) -> String {
        if self.disambiguation.is_empty() {
            self.name.clone()
        } else {
            format!("{} ({})", self.name, self.disambiguation)
        }
    }
}
