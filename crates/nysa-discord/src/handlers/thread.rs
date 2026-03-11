use crate::models::ThreadState;
use uuid::Uuid;

pub struct ThreadManager {
    placeholder: (),
}

impl ThreadManager {
    pub fn new() -> Self {
        Self { placeholder: () }
    }
}

impl Default for ThreadManager {
    fn default() -> Self {
        Self::new()
    }
}
