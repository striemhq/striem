use serde_json::{Map, Value};
pub mod event;

pub mod prelude;

pub use prelude::*;

#[derive(Debug, Clone)]
pub enum SysMessage {
    Update(Box<Map<String, Value>>),
    Reload,
    Shutdown,
}
