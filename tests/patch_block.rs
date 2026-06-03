use gp2uv::core::{dat::Geometry, model, unwrap, uvtable};

#[test]
fn patch_block_uv_inplace_preserves_everything_else() {
    let orig_block = std::fs::read("tests/fixtures/svga_block.dec.bin").unwrap();
    assert_eq!(orig_block.len(), 11476);

    let orig = uvtable::decode(&orig_block).unwrap();
    let geom =
        Geometry::parse(&std::fs::read("tests/fixtures/original.dat").unwrap(), 106).unwrap();
    let models = model::build_face_models(&orig, &geom).unwrap();
    let uw = unwrap::unwrap(&models, &geom, 25.0, unwrap::PackOpts::default());
    let patched = uvtable::patched_table(&orig, &models, &uw);

    let mut copy = orig_block.clone();
    uvtable::patch_block_uv(&mut copy, &patched, &geom.body_face_indices()).unwrap();

    // size never changes
    assert_eq!(copy.len(), 11476);

    // decode the patched block: every real face's verts (u,v AND vert_ref) match
    let back = uvtable::decode(&copy).unwrap();
    for idx in patched.real_face_indices() {
        assert_eq!(
            back.face(idx).unwrap().verts,
            patched.face(idx).unwrap().verts,
            "face {idx}"
        );
    }

    // pre-base region is byte-identical (proves structured data preserved)
    assert_eq!(
        &copy[4..patched.base],
        &orig_block[4..orig.base],
        "pre-base region must be preserved byte-identical"
    );
}
