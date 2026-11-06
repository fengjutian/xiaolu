//! Fixed desensitised diagnostic fixtures for AI evaluation (test only).
//!
//! These samples cover the acceptance cases from the AI requirements: normal,
//! overheating, disk health, memory pressure, driver anomaly, network failure,
//! missing data and conflicting signals. They contain no host name, user name,
//! serial number, MAC/IP, command line or local path.

use super::answer::{enforce_local_priority, parse_answer, validate};
use super::model::{DiagnosticFact, DiagnosticSnapshot, AI_SCHEMA_VERSION};
use super::prompt::build_payload;
use super::rules;
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    name: String,
    #[allow(dead_code)]
    description: String,
    facts: Vec<DiagnosticFact>,
    limitations: Vec<String>,
}

fn load_all() -> Vec<Fixture> {
    [
        include_str!("fixtures/normal.json"),
        include_str!("fixtures/overheated.json"),
        include_str!("fixtures/disk_health.json"),
        include_str!("fixtures/memory_pressure.json"),
        include_str!("fixtures/driver_anomaly.json"),
        include_str!("fixtures/network_failure.json"),
        include_str!("fixtures/missing_data.json"),
        include_str!("fixtures/conflicting_signals.json"),
    ]
    .iter()
    .map(|raw| serde_json::from_str::<Fixture>(raw).expect("fixture must be valid JSON"))
    .collect()
}

fn snapshot(fixture: &Fixture) -> DiagnosticSnapshot {
    let snapshot_id = format!("snap-fixture-{}", fixture.name);
    let mut facts = fixture.facts.clone();
    for fact in facts.iter_mut() {
        fact.snapshot_id = snapshot_id.clone();
    }
    let local_findings = rules::evaluate(&facts, &fixture.limitations);
    DiagnosticSnapshot {
        schema_version: AI_SCHEMA_VERSION,
        snapshot_id,
        captured_at_unix_ms: 1_762_300_000_000,
        facts,
        local_findings,
        limitations: fixture.limitations.clone(),
        sources: Vec::new(),
        excluded_fact_count: 0,
    }
}

fn expected_rules(name: &str) -> Vec<&'static str> {
    match name {
        "normal" | "missing_data" => vec![],
        "overheated" => vec!["rule.sensor.alert.cpu-high"],
        "disk_health" => vec!["rule.storage.health.0", "rule.storage.smart.0"],
        "memory_pressure" => vec!["rule.memory.usage", "rule.trend.high"],
        "driver_anomaly" => vec!["rule.drivers.problems"],
        "network_failure" => vec!["rule.network.interfaces"],
        "conflicting_signals" => vec!["rule.memory.usage"],
        other => panic!("unexpected fixture: {other}"),
    }
}

#[test]
fn fixtures_cover_every_required_scenario() {
    let names: Vec<String> = load_all().into_iter().map(|fixture| fixture.name).collect();
    for expected in [
        "normal",
        "overheated",
        "disk_health",
        "memory_pressure",
        "driver_anomaly",
        "network_failure",
        "missing_data",
        "conflicting_signals",
    ] {
        assert!(names.iter().any(|name| name == expected), "missing {expected}");
    }
}

#[test]
fn every_local_finding_references_an_existing_available_fact() {
    for fixture in load_all() {
        let snapshot = snapshot(&fixture);
        assert!(!snapshot.facts.is_empty(), "{} has no facts", fixture.name);
        let ids: HashSet<&str> = snapshot
            .facts
            .iter()
            .filter(|fact| fact.is_available())
            .map(|fact| fact.id.as_str())
            .collect();
        for fact in &snapshot.facts {
            assert_eq!(fact.snapshot_id, snapshot.snapshot_id);
            assert!(!fact.sensitive, "{} contains sensitive fact", fixture.name);
        }
        for finding in &snapshot.local_findings {
            assert!(
                !finding.evidence_refs.is_empty(),
                "{} finding {} has no evidence",
                fixture.name,
                finding.id
            );
            for reference in &finding.evidence_refs {
                assert!(
                    ids.contains(reference.as_str()),
                    "{} references unknown/unavailable fact {}",
                    fixture.name,
                    reference
                );
            }
            assert!(
                matches!(finding.severity.as_str(), "info" | "warning" | "critical"),
                "{} invalid severity",
                fixture.name
            );
        }
    }
}

#[test]
fn local_rules_match_fixture_expectations() {
    for fixture in load_all() {
        let snapshot = snapshot(&fixture);
        let mut actual: Vec<String> = snapshot
            .local_findings
            .iter()
            .map(|finding| finding.id.clone())
            .collect();
        actual.sort();
        let mut expected: Vec<String> = expected_rules(&fixture.name)
            .into_iter()
            .map(str::to_string)
            .collect();
        expected.sort();
        assert_eq!(actual, expected, "fixture {}", fixture.name);
    }
}

#[test]
fn payloads_contain_no_sensitive_fields() {
    let forbidden = [
        "hostName",
        "userName",
        "serialNumber",
        "macAddress",
        "commandLine",
        "C:\\\\Users",
        "apiKey",
    ];
    for fixture in load_all() {
        let snapshot = snapshot(&fixture);
        let (payload, _) = build_payload(&snapshot, None, &[]);
        let encoded = serde_json::to_string(&payload).expect("payload serialises");
        for needle in forbidden {
            assert!(
                !encoded.contains(needle),
                "fixture {} leaked {}",
                fixture.name,
                needle
            );
        }
    }
}

#[test]
fn missing_data_cannot_be_used_as_evidence() {
    let fixture = load_all()
        .into_iter()
        .find(|fixture| fixture.name == "missing_data")
        .expect("missing_data fixture");
    let snapshot = snapshot(&fixture);
    assert!(snapshot.local_findings.is_empty());
    assert!(!snapshot.limitations.is_empty());

    let raw = parse_answer(
        r#"{"summary":"温度为 45°C，一切正常","findings":[
            {"id":"temp","title":"温度正常","severity":"info","summary":"45°C","confidence":"high","evidenceRefs":["sensor.temperature"]}
        ]}"#,
    )
    .expect("raw answer");
    let validated = validate(raw, &snapshot.facts).expect("summary still allowed");
    assert!(validated.findings.is_empty());
    assert!(validated
        .limitations
        .iter()
        .any(|item| item.contains("缺少有效证据")));
}

#[test]
fn conflicting_model_answer_gets_local_rule_warning() {
    let fixture = load_all()
        .into_iter()
        .find(|fixture| fixture.name == "conflicting_signals")
        .expect("conflicting fixture");
    let snapshot = snapshot(&fixture);
    assert!(snapshot
        .local_findings
        .iter()
        .any(|finding| finding.id == "rule.memory.usage" && finding.severity == "critical"));

    let raw = parse_answer(
        r#"{"summary":"整体正常","findings":[
            {"id":"disk","title":"磁盘空间充足","severity":"info","summary":"空间充足","confidence":"high","evidenceRefs":["disk.c.space"]}
        ]}"#,
    )
    .expect("raw answer");
    let mut validated = validate(raw, &snapshot.facts).expect("valid evidence");
    enforce_local_priority(&snapshot.local_findings, &mut validated);
    assert!(validated
        .limitations
        .iter()
        .any(|item| item.contains("本地规则已将")));
}
