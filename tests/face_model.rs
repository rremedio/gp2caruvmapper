use gp2uv::core::{uvtable, dat::Geometry, model};
#[test]
fn builds_face_models_for_stock() {
    let dec = std::fs::read("tests/fixtures/svga_block.dec.bin").unwrap();
    let table = uvtable::decode(&dec).unwrap();
    let geom = Geometry::parse(&std::fs::read("tests/fixtures/original.dat").unwrap(), 106).unwrap();
    let models = model::build_face_models(&table, &geom).unwrap();
    assert_eq!(models.len(), 122);
    assert!(models.iter().any(|m| m.face_idx == 3), "face 3 is a body face");
    for m in &models {
        assert_eq!(m.point_indices.len(), m.orig_uv.len());
        assert_eq!(m.point_indices.len(), m.vert_refs.len());
        for &pi in &m.point_indices { assert!(pi < geom.points.len(), "point {pi} out of range"); }
    }
}
