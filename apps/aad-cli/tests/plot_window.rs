//! Plotting a window of model space instead of the whole drawing, with the real PDF writer.

use std::sync::Once;

use aad_engine::Session;
use serde_json::json;

fn install_io() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        aad_engine::cmd::file::set_io(aad_engine::cmd::file::IoHooks {
            read: |b, name| aad_io::read(b, name).map_err(|e| e.to_string()),
            write: |d, name| aad_io::write(d, name).map_err(|e| e.to_string()),
            plot: Some(|d, space, opts| aad_io::plot(d, space, opts).map_err(|e| e.to_string())),
        });
    });
}

fn pdf(s: &mut Session, params: serde_json::Value) -> Vec<u8> {
    let r = s.execute("plot", &params).unwrap();
    let data = r["data"].as_str().unwrap();
    // base64 of a PDF always starts with "JVBER" (%PDF)
    assert!(data.starts_with("JVBER"), "a PDF");
    data.as_bytes().to_vec()
}

#[test]
fn a_window_plots_part_of_the_drawing_and_a_bad_one_is_refused() {
    install_io();
    let mut s = Session::new();
    s.open_drawing(aad_engine::sample::default_sample(), "Bracket", None);
    let whole = pdf(&mut s, json!({}));
    let ext = s.doc().unwrap().extents(&aad_engine::doc::Space::Model);
    let (c, w, h) = (ext.center(), ext.width(), ext.height());
    // The left half of the drawing.
    let half = pdf(&mut s, json!({"window": [ext.min.x, ext.min.y, c.x, ext.max.y]}));
    assert_ne!(whole, half, "the window changes what is on the page");
    // The same window given with the corners the other way round is the same page.
    let flipped = pdf(&mut s, json!({"window": [c.x, ext.max.y, ext.min.x, ext.min.y]}));
    assert_eq!(half, flipped);
    // A window that holds nothing still gives a valid page; a malformed one is an error.
    pdf(&mut s, json!({"window": [ext.max.x + w, ext.max.y + h, ext.max.x + 2.0 * w, ext.max.y + 2.0 * h]}));
    assert!(s.execute("plot", &json!({"window": [1, 1, 1, 9]})).is_err());
    assert!(s.execute("plot", &json!({"window": [1, 2, 3]})).is_err());
    assert!(s.execute("exportpdf", &json!({"window": [f64::NAN, 0.0, 1.0, 1.0]})).is_err());
}
