//! The logsource extractor for StrIEM events.
//!
//! StrIEM keeps an event's logsource in `metadata["logsource"]`, apart from the
//! log body. A [`LogsourceEvent`] joins the two. [`VectorLogSourceExtractor`]
//! reads the logsource back out of that join, for rsigma's conflict-based
//! `logsource_compatible` pruning.

use rsigma_eval::LogSourceExtractor;
use rsigma_parser::LogSource;

use crate::event::LogSourceEvent;

/// Gets each event's logsource from the metadata logsource of a
/// [`LogsourceEvent`].
///
/// Each dimension reads one sub-key of `metadata["logsource"]`. By default,
/// `product` reads `product`, and so on. For each dimension, the extractor uses
/// the metadata value first, then the static default, then leaves it unset. A
/// blank value is unset. An unset dimension is a wildcard for pruning, so an
/// event with no logsource evaluates against every rule (fail-open).
#[derive(Debug, Clone)]
pub struct VectorLogSourceExtractor {
    product_key: String,
    service_key: String,
    category_key: String,
    /// Extra dimensions, as `(logsource custom dimension, metadata sub-key)`.
    custom_keys: Vec<(String, String)>,
    defaults: LogSource,
}

impl Default for VectorLogSourceExtractor {
    fn default() -> Self {
        Self {
            product_key: "product".to_string(),
            service_key: "service".to_string(),
            category_key: "category".to_string(),
            custom_keys: Vec::new(),
            defaults: LogSource::default(),
        }
    }
}

impl VectorLogSourceExtractor {
    /// Makes an extractor that reads the standard `product`, `service`, and
    /// `category` sub-keys, with no static defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the metadata sub-key that each dimension reads.
    #[must_use]
    pub fn with_keys(
        mut self,
        product: impl Into<String>,
        service: impl Into<String>,
        category: impl Into<String>,
        custom: Vec<(String, String)>,
    ) -> Self {
        self.product_key = product.into();
        self.service_key = service.into();
        self.category_key = category.into();
        self.custom_keys = custom;
        self
    }

    /// Sets the static logsource. A dimension uses it when the event's metadata
    /// does not give that dimension.
    #[must_use]
    pub fn with_defaults(mut self, defaults: LogSource) -> Self {
        self.defaults = defaults;
        self
    }

    /// Gets one dimension: the metadata value, if it is not blank, else the
    /// default.
    fn dimension(
        event: &LogSourceEvent<'_>,
        key: &str,
        default: Option<&String>,
    ) -> Option<String> {
        event
            .logsource_dim(key)
            .filter(|v| !v.trim().is_empty())
            .map(str::to_string)
            .or_else(|| default.cloned())
    }
}

impl<'a> LogSourceExtractor<LogSourceEvent<'a>> for VectorLogSourceExtractor {
    fn extract(&self, event: &LogSourceEvent<'a>) -> LogSource {
        let mut logsource = LogSource {
            product: Self::dimension(event, &self.product_key, self.defaults.product.as_ref()),
            service: Self::dimension(event, &self.service_key, self.defaults.service.as_ref()),
            category: Self::dimension(event, &self.category_key, self.defaults.category.as_ref()),
            ..Default::default()
        };
        for (dim, key) in &self.custom_keys {
            if let Some(v) = Self::dimension(event, key, self.defaults.custom.get(dim)) {
                logsource.custom.insert(dim.clone(), v);
            }
        }
        // A default custom dimension with no configured key still applies.
        for (dim, v) in &self.defaults.custom {
            logsource.custom.entry(dim.clone()).or_insert_with(|| v.clone());
        }
        logsource
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ls(product: Option<&str>, category: Option<&str>) -> LogSource {
        LogSource {
            product: product.map(str::to_string),
            category: category.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn reads_the_metadata_logsource() {
        let data = json!({});
        let event = LogSourceEvent::new(&data, ls(Some("windows"), Some("process_creation")));
        let got = VectorLogSourceExtractor::new().extract(&event);
        assert_eq!(got.product.as_deref(), Some("windows"));
        assert_eq!(got.category.as_deref(), Some("process_creation"));
        assert_eq!(got.service, None);
    }

    #[test]
    fn metadata_wins_over_defaults_and_blank_is_unset() {
        let data = json!({});
        let event = LogSourceEvent::new(&data, ls(Some("linux"), Some("  ")));
        let got = VectorLogSourceExtractor::new()
            .with_defaults(ls(Some("windows"), Some("auth")))
            .extract(&event);
        assert_eq!(got.product.as_deref(), Some("linux"));
        assert_eq!(got.category.as_deref(), Some("auth"));
    }

    #[test]
    fn remapped_key_reads_a_custom_metadata_entry() {
        // `metadata.logsource.os` is not a standard key, so it lands in the
        // event logsource's custom map. `product=os` reads it from there.
        let data = json!({});
        let mut event_ls = LogSource::default();
        event_ls.custom.insert("os".to_string(), "windows".to_string());
        let event = LogSourceEvent::new(&data, event_ls);
        let got = VectorLogSourceExtractor::new()
            .with_keys("os", "service", "category", Vec::new())
            .extract(&event);
        assert_eq!(got.product.as_deref(), Some("windows"));
    }
}
