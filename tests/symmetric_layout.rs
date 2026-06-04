//! Invariants for the GP2-style symmetric layout, across stock + extreme shapes.
//!
//! The original `.dat` plus three deliberately extreme bodywork shapes
//! (60s / 70s / SWC) must each: place EVERY body face (no silent drops), keep
//! every coord in-canvas, preserve per-face vertex counts, and pack without
//! island-bbox overlap. The UV table (face structure + original uv) is shared;
//! only the geometry differs per shape.

use gp2uv::core::{dat::Geometry, model, symmetric, unwrap, uvtable};

fn stock() -> (Vec<model::FaceModel>, Geometry) {
    let table =
        uvtable::decode(&std::fs::read("tests/fixtures/svga_block.dec.bin").unwrap()).unwrap();
    let geom =
        Geometry::parse(&std::fs::read("tests/fixtures/original.dat").unwrap(), 106).unwrap();
    let models = model::build_face_models(&table, &geom).unwrap();
    (models, geom)
}

/// The symmetric layout tags every face with a colour-group: 0 = top/centre,
/// 1 = left, 2 = right. A left face and its right mirror land in 1 and 2.
#[test]
fn symmetric_tints_faces_by_slice() {
    let (models, geom) = stock();
    let uw = symmetric::unwrap_symmetric(&models, &geom);
    for m in &models {
        assert!(uw.tint(m.face_idx).is_some(), "face {} has no tint", m.face_idx);
    }
    assert_eq!(uw.tint(16), Some(0), "front-fan face -> top/centre");
    assert_eq!(uw.tint(26), Some(1), "left sidepod -> left");
    assert_eq!(uw.tint(25), Some(2), "right sidepod (mirror of 26) -> right");
}

/// The dense layout tags faces by island index, so islands are distinguishable.
#[test]
fn dense_tints_faces_by_island() {
    let (models, geom) = stock();
    let uw = unwrap::unwrap(&models, &geom, 23.0, unwrap::PackOpts::default());
    for m in &models {
        assert!(uw.tint(m.face_idx).is_some(), "face {} has no tint", m.face_idx);
    }
    let groups: std::collections::HashSet<u8> =
        models.iter().filter_map(|m| uw.tint(m.face_idx)).collect();
    assert!(groups.len() > 1, "dense should tint islands distinctly");
}

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
