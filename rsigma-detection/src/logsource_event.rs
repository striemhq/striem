//! An [`Event`] view that pairs a log-data payload with a separately-sourced
//! [`LogSource`].
//!
//! In StrIEM's model the event **body** (the raw vendor log) and its
//! **logsource** live apart: the body is in `Event.data`, but the logsource is
//! carried in `Event.metadata["logsource"]`. rsigma, by contrast, derives the
//! logsource from the *evaluated event's own fields* via a
//! [`LogSourceExtractor`](rsigma_eval::LogSourceExtractor), and that extractor
//! is the only route to rsigma's conflict-based (`logsource_compatible`)
//! pruning — the plain `Event::data` never carries `product`/`service`/etc.
//!
//! [`LogsourceEvent`] bridges the two. It answers field lookups for the
//! reserved [`LS_PREFIX`] namespace from the supplied `LogSource`, so the
//! extractor (configured to read `__rsigma_ls.product` and friends) recovers
//! the metadata logsource — while **every other operation delegates to the log
//! data only**. Keyword search, bloom pre-filtering, timestamp extraction, and
//! `to_json()` (used for correlation storage and `include_event`) therefore see
//! exactly the vendor log and never the injected logsource, so a `|keyword`
//! rule cannot falsely match on a logsource value like `"windows"`.

use std::borrow::Cow;

use rsigma_eval::event::{Event, EventValue, JsonEvent};
use rsigma_parser::LogSource;
use serde_json::Value;

/// Reserved field-path prefix under which the supplied logsource is exposed to
/// the extractor. No real Sigma rule references it, and it is invisible to
/// keyword search (see the module docs), so it cannot collide with, or leak
/// into, detection matching.
pub const LS_PREFIX: &str = "__rsigma_ls.";

/// A log-data event augmented with an out-of-band [`LogSource`].
///
/// Matching, keyword search, and serialization act on `data` alone; only the
/// [`LS_PREFIX`] field namespace surfaces the logsource, for the extractor.
pub struct LogsourceEvent<'a> {
    data: JsonEvent<'a>,
    logsource: LogSource,
}

impl<'a> LogsourceEvent<'a> {
    /// Wrap a borrowed data payload with the logsource resolved from metadata.
    pub fn new(data: &'a Value, logsource: LogSource) -> Self {
        Self {
            data: JsonEvent::borrow(data),
            logsource,
        }
    }

    /// Resolve a reserved-namespace dimension (the part after [`LS_PREFIX`])
    /// against the supplied logsource. `product`/`service`/`category` map to
    /// the standard fields; anything else reads a custom dimension.
    fn logsource_dim(&self, dim: &str) -> Option<&str> {
        match dim {
            "product" => self.logsource.product.as_deref(),
            "service" => self.logsource.service.as_deref(),
            "category" => self.logsource.category.as_deref(),
            other => self.logsource.custom.get(other).map(String::as_str),
        }
    }
}

impl Event for LogsourceEvent<'_> {
    fn get_field(&self, path: &str) -> Option<EventValue<'_>> {
        if let Some(dim) = path.strip_prefix(LS_PREFIX) {
            return self
                .logsource_dim(dim)
                .map(|v| EventValue::Str(Cow::Borrowed(v)));
        }
        self.data.get_field(path)
    }

    fn any_string_value(&self, pred: &dyn Fn(&str) -> bool) -> bool {
        // Delegate to the data only: the logsource must not participate in
        // keyword matching.
        self.data.any_string_value(pred)
    }

    fn all_string_values(&self) -> Vec<Cow<'_, str>> {
        self.data.all_string_values()
    }

    fn to_json(&self) -> Value {
        // Correlation storage and `include_event` capture the log body, not the
        // synthetic logsource fields.
        self.data.to_json()
    }

    fn field_keys(&self) -> Vec<Cow<'_, str>> {
        self.data.field_keys()
    }
}

/// Parse the logsource carried in an event's metadata into a [`LogSource`].
///
/// StrIEM stores it at `metadata["logsource"]` as an object like
/// `{"product": "windows", "service": "sysmon"}`. A missing or malformed entry
/// yields an empty logsource, which the engine treats as fail-open (every rule
/// is eligible).
pub fn logsource_from_metadata(
    metadata: &std::collections::HashMap<String, Value>,
) -> LogSource {
    // `rsigma_parser::LogSource` is Serialize-only, so build it by hand from
    // the metadata object. Standard keys map to their fields; any other string
    // key becomes a custom dimension (mirroring the `#[serde(flatten)]` shape).
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
        let ev = LogsourceEvent::new(&data, ls(Some("windows"), Some("sysmon")));
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
        let ev = LogsourceEvent::new(&data, ls(Some("windows"), None));

        // Real data field resolves.
        assert_eq!(ev.get_field("CommandLine").unwrap().as_str().as_deref(), Some("whoami"));
        // Keyword search sees the data but NOT the logsource value "windows".
        assert!(ev.any_string_value(&|s| s == "whoami"));
        assert!(!ev.any_string_value(&|s| s == "windows"));
        // Serialization is the data only.
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
