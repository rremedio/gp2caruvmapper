//! Recovered faces get their NEW vert_ref written into the patched table and
//! into the decrypted block (round-trips through decode).
use gp2uv::core::{dat::Geometry, model::{self, ModelOpts}, unwrap, uvtable};

const SWC: &str = "/media/rremedio/Roberto/st-disk/I/GP2/GP2edit/swc/SWC-Carshape (set1).dat";

#[test]
fn recovered_vert_refs_are_written_through_patch() {
    if !std::path::Path::new(SWC).exists() {
        eprintln!("SWC .dat not present; skipping");
        return;
    }
    let block = std::fs::read("tests/fixtures/svga_block.dec.bin").unwrap();
    let orig = uvtable::decode(&block).unwrap();
    let swc = std::fs::read(SWC).unwrap();
    let geom = [106, 78, 54, 82, 110]
        .iter()
        .find_map(|&s| Geometry::parse(&swc, s))
        .expect("parse SWC");

    let (models, n_recovered) =
        model::build_face_models_opts(&orig, &geom, ModelOpts { recover_collapsed: true }).unwrap();
    assert!(n_recovered >= 1, "expected recovered faces on SWC");

    let uw = unwrap::unwrap(&models, &geom, 23.0, unwrap::PackOpts::default());
    let patched = uvtable::patched_table(&orig, &models, &uw);

    // Every recovered model's vert_refs must appear in the patched table, and
    // differ from the original table's vert_refs for that face.
    let mut checked = 0;
    for m in models.iter().filter(|m| m.recovered) {
        let pf = patched.face(m.face_idx).unwrap();
        let of = orig.face(m.face_idx).unwrap();
        assert_eq!(pf.verts.len(), m.vert_refs.len());
        let mut differs = false;
        for (k, v) in pf.verts.iter().enumerate() {
            assert_eq!(v.vert_ref, m.vert_refs[k], "face {} vert {k}", m.face_idx);
            if v.vert_ref != of.verts[k].vert_ref {
                differs = true;
            }
        }
        assert!(differs, "recovered face {} vert_ref unchanged", m.face_idx);
        checked += 1;
    }
    assert_eq!(checked, n_recovered);

    // Now write the patched table into a decrypted block in place and confirm
    // decode reads back the recovered vert_refs.
    let mut copy = block.clone();
    uvtable::patch_block_uv(&mut copy, &patched, &geom.body_face_indices()).unwrap();
    let back = uvtable::decode(&copy).unwrap();
    for m in models.iter().filter(|m| m.recovered) {
        let bf = back.face(m.face_idx).unwrap();
        for (k, v) in bf.verts.iter().enumerate() {
            assert_eq!(v.vert_ref, m.vert_refs[k], "block face {} vert {k}", m.face_idx);
        }
    }
}
