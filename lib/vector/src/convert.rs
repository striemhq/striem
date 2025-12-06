use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

use crate::event as vector_event;

use striem_common::event::Event;

impl From<vector_event::Log> for Event {
    fn from(mut event: vector_event::Log) -> Self {
        let data: Value = event
            .value
            .take()
            .map(|v| match v.into() {
                Value::String(ref s) => {
                    serde_json::from_str(s).unwrap_or_else(|_| Value::String(s.to_string()))
                }
                v => v,
            })
            .unwrap_or_else(|| {
                event
                    .fields
                    .into_iter()
                    .map(|(k, v)| (k, Into::<Value>::into(v)))
                    .collect::<serde_json::Map<_, _>>()
                    .into()
            });

        let mut metadata_full = event.metadata_full.take().unwrap_or_default();

        let mut metadata = match metadata_full.value.unwrap_or_default().kind {
            Some(vector_event::value::Kind::Map(map)) => map
                .fields
                .into_iter()
                .map(|(k, v)| (k, Into::<Value>::into(v)))
                .collect(),
            _ => HashMap::new(),
        };
        if let Some(t) = metadata_full.source_type.take() {
            metadata.insert("source_type".to_string(), t.into());
        }
        if let Some(i) = metadata_full.source_id.take() {
            metadata.insert("source_id".to_string(), i.into());
        }

        if let Some(ts) = metadata
            .remove("vector")
            .and_then(|v| match v {
                Value::Object(mut obj) => Some(obj.remove("ingest_timestamp")),
                _ => None,
            })
            .flatten()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
        {
            metadata.insert("timestamp".to_string(), ts.to_string().into());
        };

        let id: Uuid =
            match TryInto::<&[u8; 16]>::try_into(metadata_full.source_event_id.as_slice()) {
                Ok(id) => Uuid::from_slice(id).unwrap_or_else(|_| Uuid::now_v7()),
                Err(_) => Uuid::now_v7(),
            };

        metadata
            .entry("correlation_uid".to_string())
            .or_insert_with(|| id.to_string().into());

        Event { id, data, metadata }
    }
}

impl From<Event> for vector_event::Log {
    fn from(mut val: Event) -> Self {
        let fields = val
            .data
            .as_object()
            .map(|d| {
                d.to_owned()
                    .into_iter()
                    .map(|(k, v)| (k, v.into()))
                    .collect::<HashMap<String, vector_event::Value>>()
            })
            .unwrap_or_default();

        val.metadata
            .entry("correlation_uid".to_string())
            .or_insert_with(|| val.id.to_string().into());

        if let Some(ts) = val
            .metadata
            .get("timestamp")
            .and_then(|v| v.as_str())
            .and_then(|ts| serde_json::to_value(ts).ok())
        {
            val.metadata.entry("vector".to_string()).and_modify(|e| {
                e.as_object_mut()
                    .and_then(|o| o.insert("ingest_timestamp".to_string(), ts.clone()));
            });
        }

        let metadata_full = vector_event::Metadata {
            source_event_id: val.id.as_bytes().to_vec(),
            source_type: val
                .metadata
                .remove("source_type")
                .and_then(|v| v.as_str().map(|s| s.to_string())),
            source_id: val
                .metadata
                .remove("source_id")
                .and_then(|v| v.as_str().map(|s| s.to_string())),
            value: Some((&val.metadata).into()),
            ..Default::default()
        };

        vector_event::Log {
            fields,
            metadata_full: Some(metadata_full),
            ..Default::default()
        }
    }
}

impl From<&Event> for vector_event::Log {
    fn from(val: &Event) -> Self {
        let fields = val
            .data
            .as_object()
            .map(|d| {
                d.to_owned()
                    .into_iter()
                    .map(|(k, v)| (k, v.into()))
                    .collect::<HashMap<String, vector_event::Value>>()
            })
            .unwrap_or_default();

        let metadata_full = vector_event::Metadata {
            source_event_id: val.id.as_bytes().to_vec(),
            value: Some((&val.metadata).into()),
            ..Default::default()
        };

        vector_event::Log {
            fields,
            metadata_full: Some(metadata_full),
            ..Default::default()
        }
    }
}

impl From<vector_event::Value> for Value {
    fn from(val: vector_event::Value) -> Self {
        match val.kind {
            Some(vector_event::value::Kind::RawBytes(s)) => {
                Value::String(String::from_utf8_lossy(&s).to_string())
            }
            Some(vector_event::value::Kind::Boolean(b)) => Value::Bool(b),
            Some(vector_event::value::Kind::Integer(i)) => {
                Value::Number(serde_json::Number::from(i))
            }
            Some(vector_event::value::Kind::Float(d)) => {
                Value::Number(serde_json::Number::from_f64(d).unwrap())
            }
            Some(vector_event::value::Kind::Map(m)) => {
                let map: serde_json::Map<String, Value> =
                    m.fields.into_iter().map(|(k, v)| (k, v.into())).collect();
                Value::Object(map)
            }
            Some(vector_event::value::Kind::Array(a)) => {
                let array: Vec<Value> = a.items.into_iter().map(|i| i.into()).collect();
                Value::Array(array)
            }
            Some(vector_event::value::Kind::Null(_)) => Value::Null,
            Some(vector_event::value::Kind::Timestamp(t)) => Value::String(t.to_string()),
            None => Value::Null,
        }
    }
}

impl From<Value> for vector_event::Value {
    fn from(value: Value) -> Self {
        match value {
            Value::String(s) => vector_event::Value {
                kind: Some(vector_event::value::Kind::RawBytes(s.into_bytes())),
            },
            Value::Bool(b) => vector_event::Value {
                kind: Some(vector_event::value::Kind::Boolean(b)),
            },
            Value::Number(n) => {
                if n.is_i64() {
                    vector_event::Value {
                        kind: Some(vector_event::value::Kind::Integer(n.as_i64().unwrap())),
                    }
                } else {
                    vector_event::Value {
                        kind: Some(vector_event::value::Kind::Float(n.as_f64().unwrap())),
                    }
                }
            }
            Value::Object(m) => {
                let fields: std::collections::HashMap<String, vector_event::Value> =
                    m.into_iter().map(|(k, v)| (k, v.into())).collect();
                vector_event::Value {
                    kind: Some(vector_event::value::Kind::Map(vector_event::ValueMap {
                        fields,
                    })),
                }
            }
            Value::Array(a) => {
                let items: Vec<vector_event::Value> = a.into_iter().map(|i| i.into()).collect();
                vector_event::Value {
                    kind: Some(vector_event::value::Kind::Array(vector_event::ValueArray {
                        items,
                    })),
                }
            }
            Value::Null => vector_event::Value {
                kind: Some(vector_event::value::Kind::Null(0)),
            },
        }
    }
}

impl From<&HashMap<String, serde_json::Value>> for vector_event::Value {
    fn from(map: &HashMap<String, serde_json::Value>) -> Self {
        let value = map
            .iter()
            .map(|(k, v)| (k.clone(), v.clone().into()))
            .collect::<HashMap<String, vector_event::Value>>();
        vector_event::Value {
            kind: Some(vector_event::value::Kind::Map(vector_event::ValueMap {
                fields: value,
            })),
        }
    }
}
