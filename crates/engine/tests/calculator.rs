//! The `cal` command: the architect calculator behind the command line, scripts and the agent.

use aad_engine::Session;
use serde_json::json;

#[test]
fn feet_and_inches_become_an_area_in_both_systems() {
    let mut s = Session::new();
    let r = s.execute("cal", &json!({"expr": "10' 6\" x 20'"})).unwrap();
    assert_eq!(r["kind"], "area");
    assert_eq!(r["imperial"], true);
    assert_eq!(r["results"]["m2"], "19.51");
    assert_eq!(r["results"]["ft2"], "210.00");
    assert!(r["message"].as_str().unwrap().ends_with("210.00 ft²"), "{r}");

    let r = s.execute("cal", &json!({"expr": "3 x 4 x 5"})).unwrap();
    assert_eq!((r["kind"].as_str(), r["results"]["m3"].as_str(), r["results"]["ft3"].as_str()), (Some("volume"), Some("60.00"), Some("2,118.88")));
    assert_eq!(s.execute("cal", &json!({"expr": "(2+3)x4"})).unwrap()["squareMetres"], 20.0);
}

#[test]
fn bad_expressions_are_errors_and_calculating_does_not_touch_the_drawing() {
    let mut s = Session::new();
    for bad in ["", "1/0", "((1", "abc", "1..2 x 3"] {
        let r = s.execute("cal", &json!({"expr": bad}));
        // An empty expression is zero; the others are refused.
        assert_eq!(r.is_ok(), bad.is_empty(), "{bad:?}");
    }
    assert!(s.execute("cal", &json!({})).is_err());
    assert!(s.execute("cal", &json!({"expr": "9".repeat(10_000)})).is_err());
    assert!(s.doc().unwrap().model.iter().next().is_none());
}
