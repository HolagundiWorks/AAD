//! Delta between two takeoff tables (revision / publish compare).

use crate::TakeoffTable;

/// Human-readable summary of quantity changes between two runs.
pub fn takeoff_delta_summary(from: &TakeoffTable, to: &TakeoffTable) -> String {
    let dl = to.total_wall_length() - from.total_wall_length();
    let da = to.total_wall_net_area() - from.total_wall_net_area();
    let dr = to.total_room_area() - from.total_room_area();
    let dw = to.walls.len() as i64 - from.walls.len() as i64;
    let droom = to.rooms.len() as i64 - from.rooms.len() as i64;
    let dop = to.openings.len() as i64 - from.openings.len() as i64;
    format!(
        "walls {dw:+} (len {dl:+.3}, net area {da:+.3}); rooms {droom:+} (area {dr:+.3}); openings {dop:+}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WallTakeoff;

    #[test]
    fn delta_reports_length_change() {
        let mut a = TakeoffTable::default();
        a.walls.push(WallTakeoff {
            id: 0,
            layer: "A-WALL".into(),
            length: 10.0,
            thickness: 0.2,
            height: 3.0,
            gross_area: 30.0,
            volume: 6.0,
            cost_code: String::new(),
            wall_type: String::new(),
            class_status: String::new(),
        });
        let mut b = a.clone();
        b.walls[0].length = 12.0;
        b.walls[0].gross_area = 36.0;
        b.walls[0].volume = 7.2;
        let s = takeoff_delta_summary(&a, &b);
        assert!(s.contains("+2.000") || s.contains("2.000"), "{s}");
    }
}
