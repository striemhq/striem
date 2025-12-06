use std::collections::HashMap;

use serde_json::Value;
use sigmars::event::{Event as SigmaEvent, LogSource, RefEvent as SigmaRefEvent};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct Event {
    pub id: Uuid,
    pub data: Value,
    pub metadata: HashMap<String, Value>,
}
impl Default for Event {
    fn default() -> Self {
        Event {
            id: Uuid::now_v7(),
            data: Value::default(),
            metadata: HashMap::default(),
        }
    }
}
impl From<Value> for Event {
    fn from(data: Value) -> Self {
        Event {
            id: Uuid::now_v7(),
            data,
            metadata: HashMap::new(),
        }
    }
}

impl From<(Value, HashMap<String, Value>)> for Event {
    fn from(data: (Value, HashMap<String, Value>)) -> Self {
        Event {
            id: Uuid::now_v7(),
            data: data.0,
            metadata: data.1,
        }
    }
}

impl From<&Value> for Event {
    fn from(data: &Value) -> Self {
        Event {
            id: Uuid::now_v7(),
            data: data.clone(),
            metadata: HashMap::new(),
        }
    }
}

impl From<SigmaEvent> for Event {
    fn from(event: SigmaEvent) -> Self {
        let id: Uuid = event
            .metadata
            .get("id")
            .and_then(|id| id.as_str())
            .and_then(|id| Uuid::parse_str(id).ok())
            .unwrap_or_else(Uuid::now_v7);

        Event {
            id,
            data: event.data,
            metadata: HashMap::new(),
        }
    }
}

impl From<Event> for SigmaEvent {
    fn from(val: Event) -> Self {
        let logsource: LogSource = match val.metadata.get("logsource") {
            Some(logsource) => logsource.into(),
            None => LogSource::default(),
        };
        SigmaEvent {
            data: val.data,
            metadata: val.metadata,
            logsource,
        }
    }
}

impl<'a> From<&'a Event> for SigmaRefEvent<'a> {
    fn from<'r>(val: &'a Event) -> Self {
        let logsource: LogSource = match val.metadata.get("logsource") {
            Some(logsource) => logsource.into(),
            None => LogSource::default(),
        };
        SigmaRefEvent {
            data: &val.data,
            metadata: &val.metadata,
            logsource,
        }
    }
}
