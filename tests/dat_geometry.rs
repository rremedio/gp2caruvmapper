use gp2uv::core::dat::Geometry;
#[test]
fn resolves_original_dat_counts() {
    let bytes = std::fs::read("tests/fixtures/original.dat").unwrap();
    let g = Geometry::parse(&bytes, 106).unwrap();
    assert_eq!(g.scales.len(), 62);
    assert_eq!(g.points.len(), 388);
    assert_eq!(g.edges.len(), 301);
}
