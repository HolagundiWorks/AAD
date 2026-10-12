//! `project.plan` and `company.validate` through the engine: the JSON form the wizard review step,
//! scripts and the assistant use. They write nothing and never panic on hostile input.

use cadcraft_engine::Session;
use serde_json::json;

fn config() -> serde_json::Value {
    json!({
        "schema_version": 1,
        "project": {"id": "prj-1", "code": "PRJ-001", "name": "Residential Building"},
        "floors": [
            {"id": "f0", "name": "Ground", "code": "GROUND", "sequence": 0, "elevation": 0.0, "floor_to_floor": 3300.0},
            {"id": "f1", "name": "First", "code": "FIRST", "sequence": 1, "floor_to_floor": 3000.0}
        ],
        "drawing_sets": [{"id": "s-ar", "code": "AR", "name": "Architectural", "enabled": true}]
    })
}

#[test]
fn plan_reports_the_drawings_for_a_valid_configuration() {
    let mut s = Session::default();
    let r = s.execute("project.plan", &json!({"config": config()})).unwrap();
    assert_eq!(r["valid"], true);
    assert_eq!(r["plan"]["drawings"].as_array().unwrap().len(), 3, "master and two floor drawings");
    assert_eq!(r["plan"]["elevations"][1][1], 3300.0);
    assert!(r["preflight"].is_null());
}

#[test]
fn plan_reports_issues_instead_of_failing_and_changes_nothing() {
    let mut s = Session::default();
    let mut c = config();
    c["project"]["code"] = json!("bad code!");
    let r = s.execute("project.plan", &json!({"config": c})).unwrap();
    assert_eq!(r["valid"], false);
    assert_eq!(r["issues"][0]["path"], "project.code");
    assert!(s.execute("project.plan", &json!({})).is_err(), "config is required");
    assert!(s.execute("project.plan", &json!({"config": "text"})).is_err());
    assert!(s.execute("project.plan", &json!({"config": {"schema_version": 9}})).is_err());
}

#[test]
fn company_validate_lists_issues_and_the_nameplate_lines() {
    let mut s = Session::default();
    let ok = s.execute("company.validate", &json!({"profile": {"id": "c-1", "name": "Studio", "contact": {"email": "a@b.co"}}})).unwrap();
    assert_eq!(ok["valid"], true);
    assert_eq!(ok["nameplate"], json!(["Studio", "a@b.co"]));
    let bad = s.execute("company.validate", &json!({"profile": {"id": "c 1", "name": " "}})).unwrap();
    assert_eq!(bad["valid"], false);
    assert!(s.execute("company.validate", &json!({"profile": 5})).is_err());
}
