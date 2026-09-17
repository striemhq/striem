//! OCSF Detection Finding construction.
//!
//! This module changes an rsigma [`EvaluationResult`] (detection or correlation)
//! into an OCSF `Detection Finding` event (class_uid 2004). The shape is the
//! same as the one that StrIEM's `detection` service makes. The input is not the
//! same as StrIEM's. rsigma carries the matched-rule metadata on the flat
//! [`RuleHeader`] (`rule_title`, `rule_id`, `level`, `tags`) and the match body.
//! It does not carry a full `SigmaRule`. Thus this module reads from the result
//! header.

use rsigma_eval::{EvaluationResult, ResultBody, RuleHeader};
use rsigma_parser::Level;
use serde_json::{Value, json};

/// Makes the OCSF Detection Finding object for one evaluation result.
///
/// `finding_uid` is the finding's own `metadata.uid`. `correlation_uid` links
/// the finding to the event that caused it. `time_millis` is the event time in
/// epoch milliseconds.
pub fn result_to_ocsf(
    result: &EvaluationResult,
    finding_uid: &str,
    correlation_uid: &str,
    time_millis: i64,
) -> Value {
    let mut ocsf = base_finding(&result.header);

    ocsf["time"] = json!(time_millis);
    ocsf["metadata"]["uid"] = json!(finding_uid);
    ocsf["metadata"]["correlation_uid"] = json!(correlation_uid);

    // Add the match evidence. Thus a downstream consumer can see why the rule
    // fired, and does not run the rule again.
    match &result.body {
        ResultBody::Detection(d) => {
            ocsf["finding_info"]["kind"] = json!("detection");
            if !d.matched_selections.is_empty() {
                ocsf["finding_info"]["data"]["matched_selections"] = json!(d.matched_selections);
            }
            if !d.matched_fields.is_empty()
                && let Ok(fields) = serde_json::to_value(&d.matched_fields)
            {
                ocsf["finding_info"]["data"]["matched_fields"] = fields;
            }
            if let Some(event) = &d.event {
                ocsf["finding_info"]["data"]["event"] = event.clone();
            }
        }
        ResultBody::Correlation(c) => {
            ocsf["finding_info"]["kind"] = json!("correlation");
            ocsf["finding_info"]["data"]["correlation_type"] = json!(c.correlation_type.as_str());
            ocsf["finding_info"]["data"]["aggregated_value"] = json!(c.aggregated_value);
            ocsf["finding_info"]["data"]["timespan_secs"] = json!(c.timespan_secs);
            if !c.group_key.is_empty() {
                ocsf["finding_info"]["data"]["group_key"] = json!(
                    c.group_key
                        .iter()
                        .map(|(k, v)| json!({ "name": k, "value": v }))
                        .collect::<Vec<_>>()
                );
            }
        }
    }

    ocsf
}

/// The shared OCSF Detection Finding base. The function fills it from the rule
/// metadata.
fn base_finding(header: &RuleHeader) -> Value {
    let mut ocsf = json!({
        "category_uid": 2,
        "category_name": "Findings",
        "class_uid": 2004,
        "class_name": "Detection Finding",
        "activity_id": 1,
        "activity_name": "Create",
        "type_uid": 200401,
        "type_name": "Detection Finding: Create",
        "status_id": 1,
        "status": "New",
        "metadata": {
            "version": "1.8.0",
            "product": {
                "vendor_name": "StrIEM",
                "name": "rsigma-detection"
            }
        },
        "finding_info": {
            "title": header.rule_title,
            "uid": header.rule_id,
            "analytic": {
                "type_id": 1,
                "type": "Rule",
                "name": header.rule_title,
                "uid": header.rule_id
            }
        }
    });

    if let Some(attacks) = attacks_from_tags(&header.tags) {
        ocsf["finding_info"]["attacks"] = json!(attacks);
    }

    if let Some(level) = header.level {
        let (severity, severity_id) = severity_for(level);
        ocsf["severity"] = json!(severity);
        ocsf["severity_id"] = json!(severity_id);
    }

    ocsf
}

/// Changes an rsigma [`Level`] to the OCSF `severity` and `severity_id` pair.
fn severity_for(level: Level) -> (&'static str, u8) {
    match level {
        Level::Informational => ("Informational", 1),
        Level::Low => ("Low", 2),
        Level::Medium => ("Medium", 3),
        Level::High => ("High", 4),
        Level::Critical => ("Critical", 5),
    }
}

/// Makes the OCSF `attacks` array from the Sigma `attack.*` tags.
///
/// An `attack.t<id>` tag becomes a technique or sub-technique ref. A known
/// `attack.<tactic>` tag becomes a tactic ref. This function gives `None` when
/// there are no ATT&CK tags. It comes from StrIEM's `get_ocsf_attacks`.
fn attacks_from_tags(tags: &[String]) -> Option<Vec<Value>> {
    let (techniques, other): (Vec<&String>, Vec<&String>) =
        tags.iter().partition(|tag| tag.starts_with("attack.t"));

    let attacks = techniques
        .iter()
        .map(|tag| {
            let technique = format!("T{}", tag.trim_start_matches("attack.t"));
            if technique.contains('.') {
                json!({ "sub_technique": { "uid": technique } })
            } else {
                json!({ "technique": { "uid": technique } })
            }
        })
        .chain(
            other
                .iter()
                .filter(|tag| tag.starts_with("attack."))
                .filter_map(|tag| tactic_for(tag.trim_start_matches("attack."))),
        )
        .collect::<Vec<Value>>();

    if attacks.is_empty() {
        None
    } else {
        Some(attacks)
    }
}

/// Changes a Sigma ATT&CK tactic slug to its OCSF tactic ref.
fn tactic_for(slug: &str) -> Option<Value> {
    let (uid, name) = match slug {
        "initial-access" => ("TA0001", "Initial Access"),
        "execution" => ("TA0002", "Execution"),
        "persistence" => ("TA0003", "Persistence"),
        "privilege-escalation" => ("TA0004", "Privilege Escalation"),
        "defense-evasion" => ("TA0005", "Stealth"),
        "credential-access" => ("TA0006", "Credential Access"),
        "discovery" => ("TA0007", "Discovery"),
        "lateral-movement" => ("TA0008", "Lateral Movement"),
        "collection" => ("TA0009", "Collection"),
        "exfiltration" => ("TA0010", "Exfiltration"),
        "command-and-control" => ("TA0011", "Command and Control"),
        "impact" => ("TA0040", "Impact"),
        _ => return None,
    };
    Some(json!({ "tactic": { "uid": uid, "name": name } }))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use rsigma_eval::{DetectionBody, EvaluationResult, FieldMatch, ResultBody, RuleHeader};
    use rsigma_parser::Level;

    use super::*;

    fn detection_result() -> EvaluationResult {
        EvaluationResult {
            header: RuleHeader {
                rule_title: "Suspicious PowerShell".to_string(),
                rule_id: Some("11111111-1111-1111-1111-111111111111".to_string()),
                level: Some(Level::High),
                tags: vec![
                    "attack.t1059.001".to_string(),
                    "attack.execution".to_string(),
                ],
                custom_attributes: Arc::new(HashMap::new()),
                enrichments: None,
            },
            body: ResultBody::Detection(DetectionBody {
                matched_selections: vec!["selection".to_string()],
                matched_fields: vec![FieldMatch::new("CommandLine", json!("powershell -enc ..."))],
                event: None,
            }),
        }
    }

    #[test]
    fn builds_detection_finding_2004() {
        let ocsf =
            result_to_ocsf(&detection_result(), "finding-uid", "corr-uid", 1_700_000_000_000);

        assert_eq!(ocsf["class_uid"], 2004);
        assert_eq!(ocsf["type_uid"], 200401);
        assert_eq!(ocsf["time"], 1_700_000_000_000i64);
        assert_eq!(ocsf["metadata"]["uid"], "finding-uid");
        assert_eq!(ocsf["metadata"]["correlation_uid"], "corr-uid");
        assert_eq!(ocsf["metadata"]["product"]["vendor_name"], "StrIEM");
        assert_eq!(ocsf["finding_info"]["title"], "Suspicious PowerShell");
        assert_eq!(
            ocsf["finding_info"]["uid"],
            "11111111-1111-1111-1111-111111111111"
        );
        assert_eq!(ocsf["finding_info"]["kind"], "detection");
        assert_eq!(
            ocsf["finding_info"]["data"]["matched_selections"][0],
            "selection"
        );
    }

    #[test]
    fn maps_level_to_severity() {
        let ocsf = result_to_ocsf(&detection_result(), "f", "c", 0);
        assert_eq!(ocsf["severity"], "High");
        assert_eq!(ocsf["severity_id"], 4);
    }

    #[test]
    fn maps_attack_tags_to_ocsf_attacks() {
        let ocsf = result_to_ocsf(&detection_result(), "f", "c", 0);
        let attacks = ocsf["finding_info"]["attacks"]
            .as_array()
            .expect("attacks array");
        // One sub-technique (T1059.001) and one tactic (Execution, TA0002).
        assert_eq!(attacks.len(), 2);
        assert_eq!(attacks[0]["sub_technique"]["uid"], "T1059.001");
        assert_eq!(attacks[1]["tactic"]["uid"], "TA0002");
    }
}
