//! `.dat` face (texture-command) parsing + edge-walk.
use gp2uv::core::dat::Geometry;

#[test]
fn parses_texture_commands_and_edge_walk_on_stock() {
    let bytes = std::fs::read("tests/fixtures/original.dat").unwrap();
    // Stock parses cleanly at start=106 (verified against the Python reference).
    let geom = Geometry::parse(&bytes, 106).unwrap();

    // Faces (texture commands) are parsed alongside the geometry.
    let faces = geom.faces();
    // The Python reference reports 187 texture commands for original.dat.
    assert_eq!(faces.len(), 187, "expected 187 texture commands");

    // Face at command-position 2 is a 4-point polygon. Its edge-walk must
    // resolve to 4 point indices, all in range. Reference: [0, 14, 11, 11].
    let ew = geom.edge_walk(2).expect("edge_walk(2)");
    assert_eq!(ew.len(), 4, "edge_walk(2) should yield 4 points");
    for &p in &ew {
        assert!(p < geom.points.len(), "point {p} out of range");
    }
    assert_eq!(ew, vec![0, 14, 11, 11], "edge_walk(2) reference mismatch");
}
