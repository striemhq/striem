//! Runtime configuration for the detection service.
//!
//! The service is small on purpose. It needs to know four things: where to
//! listen for the Vector input, where to forward the findings (this is
//! optional), which rules to load, and how to get each event's logsource for
//! the conflict-pruning path.

use std::net::SocketAddr;
use std::path::PathBuf;

use rsigma_parser::LogSource;

use crate::logsource::VectorLogSourceExtractor;

/// The full service configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// The address that the Vector input gRPC server binds to.
    pub input: SocketAddr,
    /// The downstream Vector endpoint that the service forwards the findings to
    /// (for example, `http://vector:6000`). `None` stops the output; the service
    /// then logs the findings.
    pub output: Option<String>,
    /// The directory or file of Sigma rules to load.
    pub rules: PathBuf,
    /// The count of events to collect before one engine evaluation call.
    pub batch_size: usize,
    /// The logsource-conflict pruning configuration (the `logsource_compatible`
    /// path). It is on by default. It is the reason for this service.
    pub logsource: LogsourceConfig,
}

/// The configuration for logsource extraction. It feeds the conflict-based
/// pruning.
///
/// The logsource comes from each event's `metadata["logsource"]` (StrIEM's
/// convention). It does not come from the log body. See [`crate::event`]. A
/// [`LogsourceEvent`](crate::event::LogsourceEvent) carries it beside the body,
/// and the [`VectorLogSourceExtractor`] reads it from there.
#[derive(Debug, Clone)]
pub struct LogsourceConfig {
    /// When this is `false`, the service installs no extractor. The
    /// engine then evaluates every rule against every event (no pruning).
    pub enabled: bool,
    /// The override for the sub-key of `metadata["logsource"]` that each
    /// dimension reads. It is a
    /// `product=<key>,service=<key>,category=<key>,custom.<dim>=<key>` string.
    /// An absent dimension uses `product`, `service`, or `category`. Most
    /// deployments do not set this.
    pub field_map: Option<String>,
    /// The fixed logsource. The service uses it when the event's metadata has no
    /// dimension. It is a `product=...,service=...,category=...` string.
    pub event_logsource: Option<String>,
}

impl Default for LogsourceConfig {
    fn default() -> Self {
        LogsourceConfig {
            enabled: true,
            field_map: None,
            event_logsource: None,
        }
    }
}

impl Config {
    /// Makes the [`VectorLogSourceExtractor`] for the conflict-pruning path.
    /// Gives `Ok(None)` when pruning is off.
    pub fn build_logsource_extractor(&self) -> Result<Option<VectorLogSourceExtractor>, String> {
        if !self.logsource.enabled {
            return Ok(None);
        }

        // The extractor reads the logsource that `LogsourceEvent` carries from
        // the event metadata. `field_map` only sets which metadata sub-key feeds
        // each dimension. The standard keys are the defaults.
        let (product_key, service_key, category_key, custom) = match &self.logsource.field_map {
            Some(map) => {
                let parsed = parse_logsource_kv(map)
                    .map_err(|e| format!("invalid logsource field-map: {e}"))?;
                (
                    parsed.product.unwrap_or_else(|| "product".to_string()),
                    parsed.service.unwrap_or_else(|| "service".to_string()),
                    parsed.category.unwrap_or_else(|| "category".to_string()),
                    parsed.custom,
                )
            }
            None => (
                "product".to_string(),
                "service".to_string(),
                "category".to_string(),
                Vec::new(),
            ),
        };

        let mut extractor =
            VectorLogSourceExtractor::new().with_keys(product_key, service_key, category_key, custom);

        if let Some(static_ls) = &self.logsource.event_logsource {
            let parsed = parse_logsource_kv(static_ls)
                .map_err(|e| format!("invalid event-logsource: {e}"))?;
            let mut defaults = LogSource {
                product: parsed.product,
                service: parsed.service,
                category: parsed.category,
                ..Default::default()
            };
            for (dim, value) in parsed.custom {
                defaults.custom.insert(dim, value);
            }
            if defaults.product.is_some()
                || defaults.service.is_some()
                || defaults.category.is_some()
                || !defaults.custom.is_empty()
            {
                extractor = extractor.with_defaults(defaults);
            }
        }

        Ok(Some(extractor))
    }
}

/// A parsed `product=..,service=..,category=..,custom.<name>=..` option.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ParsedLogsource {
    pub product: Option<String>,
    pub service: Option<String>,
    pub category: Option<String>,
    pub custom: Vec<(String, String)>,
}

/// Parses a logsource key/value option. A bare key that is not one of the three
/// standard dimensions is an error. A custom dimension uses the `custom.`
/// prefix. This function comes from the rsigma CLI's `parse_logsource_kv`.
pub(crate) fn parse_logsource_kv(input: &str) -> Result<ParsedLogsource, String> {
    let mut out = ParsedLogsource::default();
    for pair in input.split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("expected key=value, got '{pair}'"))?;
        let (key, value) = (key.trim(), value.trim());
        if value.is_empty() {
            continue;
        }
        match key {
            "product" => out.product = Some(value.to_string()),
            "service" => out.service = Some(value.to_string()),
            "category" => out.category = Some(value.to_string()),
            other => {
                if let Some(dimension) = other.strip_prefix("custom.") {
                    if dimension.is_empty() {
                        return Err(format!("empty custom dimension name in '{pair}'"));
                    }
                    out.custom.push((dimension.to_string(), value.to_string()));
                } else {
                    return Err(format!(
                        "unknown logsource key '{other}' (expected product, service, category, or custom.<name>)"
                    ));
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_standard_dimensions() {
        let parsed = parse_logsource_kv("product=windows,service=sysmon").unwrap();
        assert_eq!(parsed.product.as_deref(), Some("windows"));
        assert_eq!(parsed.service.as_deref(), Some("sysmon"));
        assert_eq!(parsed.category, None);
    }

    #[test]
    fn rejects_unknown_bare_keys() {
        let err = parse_logsource_kv("product=windows,os=linux").unwrap_err();
        assert!(err.contains("unknown logsource key 'os'"), "got: {err}");
    }

    #[test]
    fn disabled_yields_no_extractor() {
        let cfg = Config {
            input: "0.0.0.0:6000".parse().unwrap(),
            output: None,
            rules: PathBuf::from("rules"),
            batch_size: 64,
            logsource: LogsourceConfig {
                enabled: false,
                ..Default::default()
            },
        };
        assert!(cfg.build_logsource_extractor().unwrap().is_none());
    }
}
