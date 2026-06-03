//! Invariants for the GP2-style symmetric layout, across stock + extreme shapes.
//!
//! The original `.dat` plus three deliberately extreme bodywork shapes
//! (60s / 70s / SWC) must each: place EVERY body face (no silent drops), keep
//! every coord in-canvas, preserve per-face vertex counts, and pack without
//! island-bbox overlap. The UV table (face structure + original uv) is shared;
//! only the geometry differs per shape.

use gp2uv::core::{dat::Geometry, model, symmetric, uvtable};

fn check(dat: &str) {
    let table =
        uvtable::decode(&std::fs::read("tests/fixtures/svga_block.dec.bin").unwrap()).unwrap();
    let geom = Geometry::parse(&std::fs::read(dat).unwrap(), 106).unwrap();
    let models = model::build_face_models(&table, &geom).unwrap();
    let uw = symmetric::unwrap_symmetric(&models, &geom);

    // 1) coverage: every body face is in the wireframe.
    for m in &models {
        let c = uw
            .coords(m.face_idx)
            .unwrap_or_else(|| panic!("{dat}: face {} missing from wireframe", m.face_idx));
        // 2) per-face vertex count preserved (pairs with vert_refs for patching).
        assert_eq!(
            c.len(),
            m.point_indices.len(),
            "{dat}: face {} vertex count changed",
            m.face_idx
        );
        // 3) every coord inside the atlas.
        for &[u, v] in c {
            assert!(
                (0..256).contains(&u) && (0..164).contains(&v),
                "{dat}: coord {u},{v} out of canvas"
            );
        }
    }

    // 4) islands don't overlap.
    assert_eq!(uw.max_overlap(), 0, "{dat}: island bboxes overlap");
}

#[test]
fn symmetric_stock() {
    check("tests/fixtures/original.dat");
}

#[test]
fn symmetric_extreme_60s() {
    check("tests/fixtures/60s.dat");
}

#[test]
fn symmetric_extreme_70s() {
    check("tests/fixtures/70s.dat");
}

#[test]
fn symmetric_extreme_swc() {
    check("tests/fixtures/SWC.dat");
}
