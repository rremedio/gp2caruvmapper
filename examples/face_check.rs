//! Per-face diagnostic: find faces that unwrap to (near-)degenerate UVs.
//! Usage: GP2_EXE=/path/GP2.EXE cargo run --release --example face_check -- <car.dat> [eps]
use gp2uv::core::{dat::Geometry, model, unwrap, uvtable};

fn area(p: &[[i32; 2]]) -> f64 {
    let n = p.len();
    if n < 3 { return 0.0; }
    let mut a = 0i64;
    for i in 0..n {
        let j = (i + 1) % n;
        a += (p[i][0] as i64) * (p[j][1] as i64) - (p[j][0] as i64) * (p[i][1] as i64);
    }
    (a as f64).abs() / 2.0
}
fn area3d(p: &[gp2uv::core::dat::Point3D]) -> f64 {
    // Newell area
    let n = p.len();
    if n < 3 { return 0.0; }
    let (mut nx, mut ny, mut nz) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let a = p[i]; let b = p[(i + 1) % n];
        nx += (a.y as f64 - b.y as f64) * (a.z as f64 + b.z as f64);
        ny += (a.z as f64 - b.z as f64) * (a.x as f64 + b.x as f64);
        nz += (a.x as f64 - b.x as f64) * (a.y as f64 + b.y as f64);
    }
    (nx * nx + ny * ny + nz * nz).sqrt() / 2.0
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("need .dat path");
    let eps: f64 = args.next().map(|s| s.parse().unwrap()).unwrap_or(23.0);
    let exe = std::fs::read(std::env::var("GP2_EXE").expect("GP2_EXE")).unwrap();
    let table = uvtable::read_svga_from_exe(&exe).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let geom = [106, 78, 54, 82, 110].iter().find_map(|&s| Geometry::parse(&bytes, s)).expect("parse");
    let models = model::build_face_models(&table, &geom).expect("models");
    let uw = unwrap::unwrap(&models, &geom, eps, unwrap::PackOpts::default());

    let mut degen_uv = 0; let mut degen_3d = 0; let mut tiny = 0;
    for m in &models {
        let p3 = m.points3d(&geom);
        let a3 = area3d(&p3);
        let coords = uw.coords(m.face_idx).cloned().unwrap_or_default();
        let auv = area(&coords);
        if a3 < 1.0 { degen_3d += 1; }
        if auv < 0.5 { degen_uv += 1; }
        else if auv < 4.0 { tiny += 1; }
        if auv < 4.0 || a3 < 1.0 {
            println!("face {:>3}: nverts={} 3D-area={:>10.1} uv-area={:>6.1}  pts={:?}",
                m.face_idx, m.point_indices.len(), a3, auv, m.point_indices);
        }
    }
    println!("\n{}: {} real faces  | degenerate-UV(<0.5)={} tiny-UV(<4)={} degenerate-3D(<1)={}",
        path, models.len(), degen_uv, tiny, degen_3d);
    println!("unwrap fits={} ", uw.fits);
}
