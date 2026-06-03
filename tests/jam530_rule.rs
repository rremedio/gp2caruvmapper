use gp2uv::core::{dat::Geometry, model, unwrap, uvtable};

#[test]
fn jam530_adds_face3_and_unwraps_it() {
    let table =
        uvtable::decode(&std::fs::read("tests/fixtures/svga_block.dec.bin").unwrap()).unwrap();
    let geom =
        Geometry::parse(&std::fs::read("tests/fixtures/original.dat").unwrap(), 106).unwrap();
    // body set is exactly the 121 covered + face 3
    let body = geom.body_face_indices();
    assert_eq!(body.len(), 122, "jam_id 530 body faces");
    assert!(body.contains(&3), "face 3 is a body face");

    let models = model::build_face_models(&table, &geom).unwrap();
    assert_eq!(models.len(), 122);
    let m3 = models.iter().find(|m| m.face_idx == 3).expect("face 3 model");
    // face 3's points come from the default entry's vertRef {30,31,32,33}, non-degenerate
    let p3 = m3.points3d(&geom);
    assert!(model::area3d(&p3) > 1.0, "face 3 geometry non-degenerate");

    // and it gets a real (non-degenerate) unwrap region
    let uw = unwrap::unwrap(&models, &geom, 23.0, unwrap::PackOpts::default());
    let c = uw.coords(3).expect("face 3 unwrap coords");
    // shoelace area of the unwrapped polygon
    let n = c.len();
    let mut a = 0i64;
    for i in 0..n {
        let j = (i + 1) % n;
        a += (c[i][0] as i64) * (c[j][1] as i64) - (c[j][0] as i64) * (c[i][1] as i64);
    }
    assert!((a.abs() / 2) > 0, "face 3 unwrapped to a non-degenerate polygon");
}
