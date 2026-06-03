use gp2uv::core::uvtable;
use std::collections::BTreeMap;

#[derive(serde::Deserialize)]
struct Golden { nfaces: usize, faces: BTreeMap<String, FaceGold> }
#[derive(serde::Deserialize)]
struct FaceGold { uv: Vec<[u16; 2]>, pts: Vec<u32> }

#[test]
fn decodes_block_matching_golden() {
    let dec = std::fs::read("tests/fixtures/svga_block.dec.bin").unwrap();
    let golden: Golden = serde_json::from_slice(
        &std::fs::read("tests/fixtures/svga_uv_golden.json").unwrap()).unwrap();
    let t = uvtable::decode(&dec).unwrap();
    assert_eq!(t.faces_present(), golden.nfaces); // 179
    assert_eq!(t.real_face_indices().len(), 121); // 58 share the default entry
    for (idx_str, fg) in &golden.faces {
        let idx: usize = idx_str.parse().unwrap();
        let f = t.face(idx).unwrap();
        assert_eq!(f.verts.len(), fg.uv.len(), "face {idx} vcount");
        for (k, vert) in f.verts.iter().enumerate() {
            assert_eq!([vert.u, vert.v], fg.uv[k], "face {idx} v{k} uv");
            assert_eq!(vert.vert_ref as u32, fg.pts[k] * 24, "face {idx} v{k} vertRef");
        }
    }
}
