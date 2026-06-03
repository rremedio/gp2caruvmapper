//! Recover collapsed-texture faces from `.dat` edge-walk geometry.
use gp2uv::core::{
    dat::{Geometry, Point3D},
    model::{self, ModelOpts},
    uvtable,
};

/// Newell 3D polygon area (same convention as model::area3d).
fn area3d(p: &[Point3D]) -> f64 {
    let n = p.len();
    if n < 3 {
        return 0.0;
    }
    let (mut nx, mut ny, mut nz) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let a = p[i];
        let b = p[(i + 1) % n];
        nx += (a.y as f64 - b.y as f64) * (a.z as f64 + b.z as f64);
        ny += (a.z as f64 - b.z as f64) * (a.x as f64 + b.x as f64);
        nz += (a.x as f64 - b.x as f64) * (a.y as f64 + b.y as f64);
    }
    (nx * nx + ny * ny + nz * nz).sqrt() / 2.0
}

const SWC: &str = "/media/rremedio/Roberto/st-disk/I/GP2/GP2edit/swc/SWC-Carshape (set1).dat";

/// On stock `original.dat` there are no collapsed faces, so recovery must be a
/// no-op: build_face_models_opts(recover=true) equals build_face_models().
#[test]
fn recovery_is_noop_on_stock() {
    let dec = std::fs::read("tests/fixtures/svga_block.dec.bin").unwrap();
    let table = uvtable::decode(&dec).unwrap();
    let geom = Geometry::parse(&std::fs::read("tests/fixtures/original.dat").unwrap(), 106).unwrap();

    let base = model::build_face_models(&table, &geom).unwrap();
    let (rec, n_recovered) =
        model::build_face_models_opts(&table, &geom, ModelOpts { recover_collapsed: true })
            .unwrap();

    assert_eq!(n_recovered, 0, "stock has no collapsed faces to recover");
    assert_eq!(base.len(), rec.len());
    for (a, b) in base.iter().zip(rec.iter()) {
        assert_eq!(a.face_idx, b.face_idx);
        assert_eq!(a.point_indices, b.point_indices, "face {}", a.face_idx);
        assert_eq!(a.vert_refs, b.vert_refs, "face {}", a.face_idx);
    }
}

/// On the SWC reshaped car, several faces have collapsed `vertRef` polygons but
/// valid edge-walk polygons of the same vertex count. With recovery ON those
/// faces get non-degenerate point sets (different from vertRef/24); OFF, none.
#[test]
fn recovers_collapsed_faces_on_swc() {
    if !std::path::Path::new(SWC).exists() {
        eprintln!("SWC .dat not present; skipping");
        return;
    }
    let dec = std::fs::read("tests/fixtures/svga_block.dec.bin").unwrap();
    let table = uvtable::decode(&dec).unwrap();
    let swc = std::fs::read(SWC).unwrap();
    let geom = [106, 78, 54, 82, 110]
        .iter()
        .find_map(|&s| Geometry::parse(&swc, s))
        .expect("parse SWC geometry");

    // recover OFF: no faces recovered.
    let (off, n_off) =
        model::build_face_models_opts(&table, &geom, ModelOpts { recover_collapsed: false })
            .unwrap();
    assert_eq!(n_off, 0, "recover OFF must not recover any face");

    // recover ON: at least a few faces recovered, all non-degenerate now.
    let (on, n_on) =
        model::build_face_models_opts(&table, &geom, ModelOpts { recover_collapsed: true })
            .unwrap();
    assert!(n_on >= 1, "expected at least one recovered face on SWC, got {n_on}");

    // For every face that changed vs the OFF build, its new polygon must be
    // non-degenerate, and its vert_refs must equal point_indices*24.
    let mut changed = 0;
    for (a, b) in off.iter().zip(on.iter()) {
        assert_eq!(a.face_idx, b.face_idx);
        if a.point_indices != b.point_indices {
            changed += 1;
            let pts: Vec<Point3D> = b.point_indices.iter().map(|&p| geom.points[p]).collect();
            assert!(
                area3d(&pts) >= 1.0,
                "recovered face {} still degenerate (area {})",
                b.face_idx,
                area3d(&pts)
            );
            for (k, &pi) in b.point_indices.iter().enumerate() {
                assert_eq!(b.vert_refs[k], (pi * 24) as u16, "face {}", b.face_idx);
            }
        }
    }
    assert_eq!(changed, n_on, "recovered count must equal changed faces");
}
