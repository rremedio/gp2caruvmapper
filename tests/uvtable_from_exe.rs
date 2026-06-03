use gp2uv::core::uvtable;
#[test]
fn read_from_exe_matches_block_fixture() {
    let Ok(path) = std::env::var("GP2_EXE") else { eprintln!("GP2_EXE not set; skip"); return; };
    let exe = std::fs::read(path).unwrap();
    let from_exe = uvtable::read_svga_from_exe(&exe).unwrap();
    let from_dec = uvtable::decode(&std::fs::read("tests/fixtures/svga_block.dec.bin").unwrap()).unwrap();
    assert_eq!(from_exe.faces_present(), 179);
    for idx in from_dec.real_face_indices() {
        assert_eq!(from_exe.face(idx).unwrap().verts, from_dec.face(idx).unwrap().verts);
    }
}
