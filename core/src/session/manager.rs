use std::collections::HashMap;
use uuid::Uuid;

#[allow(dead_code)]
pub struct SessionManager {
    sessions: HashMap<Uuid, ()>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }
}
