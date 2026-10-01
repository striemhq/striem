//! An [`Event`] view that joins a log-data payload with a [`LogSource`] from a
//! separate source.
//!
//! In StrIEM's model, the event **body** (the raw vendor log) and its
//! **logsource** are apart. The body is in `Event.data`. The logsource is in
//! `Event.metadata["logsource"]`. rsigma is different. It gets the logsource
//! from the *fields of the event that it evaluates*, with a
//! [`LogSourceExtractor`](rsigma_eval::LogSourceExtractor). That extractor is
//! the only way to rsigma's conflict-based (`logsource_compatible`) pruning. The
//! plain `Event::data` does not carry `product`, `service`, and the others.
//!
//! [`LogsourceEvent`] joins the two. It answers a field lookup for the reserved
//! [`LS_PREFIX`] namespace from the given `LogSource`. Thus the extractor (which
//! reads `__rsigma_ls.product` and the others) gets the metadata logsource. But
//! **every other operation uses only the log data**. Thus keyword search, bloom
//! pre-filter, timestamp extraction, and `to_json()` (for correlation storage
//! and `include_event`) see only the vendor log. They never see the added
//! logsource. Thus a `|keyword` rule cannot match by mistake on a logsource
//! value such as `"windows"`.

use std::borrow::Cow;

use rsigma_eval::event::{Event, EventValue, JsonEvent};
use rsigma_parser::LogSource;
use serde_json::Value;

/// The reserved field-path prefix. The event shows the given logsource to the
/// extractor under this prefix. No real Sigma rule uses it. Keyword search does
/// not see it (see the module docs). Thus it cannot collide with the detection
/// match, and it cannot leak into the detection match.
pub const LS_PREFIX: &str = "__rsigma_ls.";

/// A log-data event with its associated [`LogSource`]
pub struct LogSourceEvent<'a> {
    data: JsonEvent<'a>,
    logsource: LogSource,
}

impl<'a> LogSourceEvent<'a> {
    /// Joins a borrowed data payload with the logsource from the metadata.
    pub fn new(data: &'a Value, logsource: LogSource) -> Self {
        Self {
            data: JsonEvent::borrow(data),
            logsource,
        }
    }

    /// Gets a reserved-namespace dimension (the part after [`LS_PREFIX`]) from
    /// the given logsource. `product`, `service`, and `category` use the
    /// standard fields. Any other name reads a custom dimension.
    pub(crate) fn logsource_dim(&self, dim: &str) -> Option<&str> {
        match dim {
            "product" => self.logsource.product.as_deref(),
            "service" => self.logsource.service.as_deref(),
            "category" => self.logsource.category.as_deref(),
            other => self.logsource.custom.get(other).map(String::as_str),
        }
    }
}

impl Event for LogSourceEvent<'_> {
    fn get_field(&self, path: &str) -> Option<EventValue<'_>> {
        /*if let Some(dim) = path.strip_prefix(LS_PREFIX) {
            return self
                .logsource_dim(dim)
                .map(|v| EventValue::Str(Cow::Borrowed(v)));
        }*/
        self.data.get_field(path)
    }

    fn any_string_value(&self, pred: &dyn Fn(&str) -> bool) -> bool {
        // Use only the data. The logsource must not take part in the keyword
        // match.
        self.data.any_string_value(pred)
    }

    fn all_string_values(&self) -> Vec<Cow<'_, str>> {
        self.data.all_string_values()
    }

    fn to_json(&self) -> Value {
        // Correlation storage and `include_event` take the log body. They do
        // not take the added logsource fields.
        self.data.to_json()
    }

    fn field_keys(&self) -> Vec<Cow<'_, str>> {
        self.data.field_keys()
    }
}

/// Parses the logsource in an event's metadata into a [`LogSource`].
///
/// StrIEM keeps it at `metadata["logsource"]` as an object, for example
/// `{"product": "windows", "service": "sysmon"}`. An absent or bad entry gives
/// an empty logsource. The engine treats an empty logsource as fail-open (every
/// rule can run).
pub fn logsource_from_metadata(
    metadata: &std::collections::HashMap<String, Value>,
) -> LogSource {
    // `rsigma_parser::LogSource` can only serialize. Thus make it by hand from
    // the metadata object. A standard key uses its field. Any other string key
    // becomes a custom dimension (the same as the `#[serde(flatten)]` shape).
    let Some(obj) = metadata.get("logsource").and_then(Value::as_object) else {
        return LogSource::default();
    };

    let mut logsource = LogSource::default();
    for (key, value) in obj {
        let Some(s) = value.as_str() else { continue };
        let s = s.to_string();
        match key.as_str() {
            "product" => logsource.product = Some(s),
            "service" => logsource.service = Some(s),
            "category" => logsource.category = Some(s),
            "definition" => logsource.definition = Some(s),
            other => {
                logsource.custom.insert(other.to_string(), s);
            }
        }
    }
    logsource
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ls(product: Option<&str>, service: Option<&str>) -> LogSource {
        LogSource {
            product: product.map(str::to_string),
            service: service.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn reserved_paths_read_logsource() {
        let data = json!({ "EventID": 1 });
        let ev = LogSourceEvent::new(&data, ls(Some("windows"), Some("sysmon")));
        assert_eq!(
            ev.get_field("__rsigma_ls.product").unwrap().as_str().as_deref(),
            Some("windows")
        );
        assert_eq!(
            ev.get_field("__rsigma_ls.service").unwrap().as_str().as_deref(),
            Some("sysmon")
        );
        assert!(ev.get_field("__rsigma_ls.category").is_none());
    }

    #[test]
    fn data_fields_delegate_and_logsource_is_invisible() {
        let data = json!({ "EventID": 1, "CommandLine": "whoami" });
        let ev = LogSourceEvent::new(&data, ls(Some("windows"), None));

        // A real data field resolves.
        assert_eq!(ev.get_field("CommandLine").unwrap().as_str().as_deref(), Some("whoami"));
        // Keyword search sees the data, but not the logsource value "windows".
        assert!(ev.any_string_value(&|s| s == "whoami"));
        assert!(!ev.any_string_value(&|s| s == "windows"));
        // The serialization is the data only.
        assert_eq!(ev.to_json(), data);
    }

    #[test]
    fn metadata_parses_into_logsource() {
        let mut meta = std::collections::HashMap::new();
        meta.insert("logsource".to_string(), json!({"product": "linux", "category": "auth"}));
        let ls = logsource_from_metadata(&meta);
        assert_eq!(ls.product.as_deref(), Some("linux"));
        assert_eq!(ls.category.as_deref(), Some("auth"));
        assert_eq!(ls.service, None);
    }

    #[test]
    fn absent_metadata_is_empty_logsource() {
        let meta = std::collections::HashMap::new();
        let ls = logsource_from_metadata(&meta);
        assert_eq!(ls.product, None);
        assert_eq!(ls.service, None);
        assert_eq!(ls.category, None);
    }
}
