//! Audit collapsed-face recovery for a car `.dat`.
//!
//! Usage: GP2_EXE=/path/GP2.EXE cargo run --release --example recover_audit -- <car.dat>
//!
//! Reports, for the given car: how many real faces have a degenerate vertRef
//! polygon, how many of those are recovered from the edge-walk geometry, how
//! many are skipped because the edge-walk vertex count differs, and confirms
//! every recovered face is non-degenerate afterwards.
use gp2uv::core::{
    dat::{Geometry, Point3D},
    model::{self, area3d, ModelOpts},
    uvtable,
};

fn main() {
    let path = std::env::args().nth(1).expect("need .dat path");
    let exe = std::fs::read(std::env::var("GP2_EXE").expect("GP2_EXE")).unwrap();
    let table = uvtable::read_svga_from_exe(&exe).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let geom = [106, 78, 54, 82, 110]
        .iter()
        .find_map(|&s| Geometry::parse(&bytes, s))
        .expect("parse geometry");

    let (off, _) =
        model::build_face_models_opts(&table, &geom, ModelOpts { recover_collapsed: false })
            .unwrap();
    let (on, n_recovered) =
        model::build_face_models_opts(&table, &geom, ModelOpts { recover_collapsed: true }).unwrap();

    let mut degenerate = 0usize; // real faces with a collapsed vertRef polygon
    let mut skipped_count = 0usize; // degenerate but edge-walk count mismatched/invalid
    for m in &off {
        let pts: Vec<Point3D> = m.point_indices.iter().map(|&p| geom.points[p]).collect();
        if area3d(&pts) < 1.0 {
            degenerate += 1;
            match geom.edge_walk(m.face_idx) {
                Some(ew) if ew.len() == m.point_indices.len() => {}
                _ => skipped_count += 1,
            }
        }
    }

    println!("{path}");
    println!("  real faces           : {}", off.len());
    println!("  degenerate vertRef   : {degenerate}");
    println!("  recovered            : {n_recovered}");
    println!("  skipped (count != n) : {skipped_count}");

    // Confirm every recovered face is non-degenerate after recovery.
    let mut bad = 0;
    for m in on.iter().filter(|m| m.recovered) {
        let pts: Vec<Point3D> = m.point_indices.iter().map(|&p| geom.points[p]).collect();
        let a = area3d(&pts);
        if a < 1.0 {
            bad += 1;
            println!("  !! recovered face {} still degenerate (area {a})", m.face_idx);
        }
    }
    println!(
        "  all recovered faces non-degenerate: {}",
        if bad == 0 { "yes" } else { "NO" }
    );
}
