//! Runtime configuration for the detection service.
//!
//! The service is deliberately small: it needs to know where to listen for
//! Vector ingest, where (optionally) to forward findings, which rules to load,
//! and how to derive each event's logsource for the conflict-pruning path.

use std::net::SocketAddr;
use std::path::PathBuf;

use rsigma_eval::LogSourceExtractor;
use rsigma_parser::LogSource;

use crate::logsource_event::LS_PREFIX;

/// Fully-resolved service configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Address the Vector ingest gRPC server binds to.
    pub input: SocketAddr,
    /// Downstream Vector endpoint findings are forwarded to (e.g.
    /// `http://vector:6000`). `None` disables egress (findings are logged).
    pub output: Option<String>,
    /// Directory or file of Sigma rules to load.
    pub rules: PathBuf,
    /// How many events to gather before a single engine evaluation call.
    pub batch_size: usize,
    /// Logsource-conflict pruning configuration (the `logsource_compatible`
    /// path). Enabled by default — it is the reason this service exists.
    pub logsource: LogsourceConfig,
}

/// Configuration for logsource extraction feeding the conflict-based pruning.
///
/// The logsource is sourced from each event's `metadata["logsource"]` (StrIEM's
/// convention), not from the log body — see [`crate::logsource_event`]. The
/// extractor reads it back through the reserved [`LS_PREFIX`] namespace that a
/// [`LogsourceEvent`](crate::logsource_event::LogsourceEvent) exposes.
#[derive(Debug, Clone)]
pub struct LogsourceConfig {
    /// When `false`, no [`LogSourceExtractor`] is installed and the engine
    /// evaluates every rule against every event (no pruning).
    pub enabled: bool,
    /// Overrides which sub-key of `metadata["logsource"]` each dimension reads,
    /// as a `product=<key>,service=<key>,category=<key>,custom.<dim>=<key>`
    /// string. Absent dimensions default to `product`/`service`/`category`;
    /// most deployments leave this unset.
    pub field_map: Option<String>,
    /// Static logsource applied when the event's metadata does not carry a
    /// dimension, as a `product=...,service=...,category=...` string.
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
    /// Build the [`LogSourceExtractor`] for the conflict-pruning path, or
    /// `Ok(None)` when pruning is disabled.
    pub fn build_logsource_extractor(&self) -> Result<Option<LogSourceExtractor>, String> {
        if !self.logsource.enabled {
            return Ok(None);
        }

        // The extractor reads the logsource through the reserved LS_PREFIX
        // namespace exposed by `LogsourceEvent`, which maps it back from the
        // event metadata. `field_map` only customizes which metadata sub-key
        // feeds each dimension; the standard keys are the defaults.
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

        let mut extractor = LogSourceExtractor::new().with_field_names(
            format!("{LS_PREFIX}{product_key}"),
            format!("{LS_PREFIX}{service_key}"),
            format!("{LS_PREFIX}{category_key}"),
        );
        if !custom.is_empty() {
            extractor = extractor.with_custom_fields(
                custom
                    .into_iter()
                    .map(|(dim, key)| (dim, format!("{LS_PREFIX}{key}")))
                    .collect(),
            );
        }

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

/// Parse a logsource key/value option. Bare keys other than the three standard
/// dimensions are an error; custom dimensions use an explicit `custom.` prefix.
/// Ported from the rsigma CLI's `parse_logsource_kv`.
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
