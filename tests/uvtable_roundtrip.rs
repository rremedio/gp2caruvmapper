use gp2uv::core::{dat::Geometry, model, unwrap, uvtable};

#[test]
fn encode_decode_roundtrip_and_size() {
    let orig =
        uvtable::decode(&std::fs::read("tests/fixtures/svga_block.dec.bin").unwrap()).unwrap();
    let geom =
        Geometry::parse(&std::fs::read("tests/fixtures/original.dat").unwrap(), 106).unwrap();
    let models = model::build_face_models(&orig, &geom).unwrap();
    let uw = unwrap::unwrap(&models, &geom, 25.0, unwrap::PackOpts::default());
    let patched = uvtable::patched_table(&orig, &models, &uw);
    let block = uvtable::encode(&patched);
    let back = uvtable::decode(&block).unwrap();
    // real faces round-trip exactly (uv + vert_ref)
    for idx in patched.real_face_indices() {
        assert_eq!(
            back.face(idx).unwrap().verts,
            patched.face(idx).unwrap().verts,
            "face {idx}"
        );
    }
    // default faces still present and shared
    assert_eq!(back.faces_present(), patched.faces_present());

    // encode(orig) round-trips for real faces too.
    let orig_block = uvtable::encode(&orig);
    let orig_back = uvtable::decode(&orig_block).unwrap();
    for idx in orig.real_face_indices() {
        assert_eq!(
            orig_back.face(idx).unwrap().verts,
            orig.face(idx).unwrap().verts,
            "orig face {idx}"
        );
    }
    assert_eq!(orig_back.faces_present(), orig.faces_present());

    // fits the block
    assert!(orig_block.len() <= 11476, "orig encodes within block");
    assert!(block.len() <= 11476, "patched encodes within block");
}
