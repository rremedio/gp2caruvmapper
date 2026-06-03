//! CRITICAL acceptance gate: our per-face flatten reproduces GP2's unwrap shape.
//!
//! For each real face we flatten its `.dat` points and Procrustes-fit the
//! original `(u, v)` pairs onto the flattened points (best similarity incl.
//! reflection). The normalized RMS residual measures shape congruence.

use gp2uv::core::{dat::Geometry, model, uvtable, unwrap::flatten_face};

/// 2x2 SVD via the symmetric eigendecomposition of MᵀM and M·Mᵀ.
/// Returns (U, s0, s1, V) such that M = U * diag(s0, s1) * Vᵀ,
/// with U, V orthogonal (each a 2x2 rotation/reflection) and s0,s1 >= 0.
fn svd2x2(m: [[f64; 2]; 2]) -> ([[f64; 2]; 2], f64, f64, [[f64; 2]; 2]) {
    let a = m[0][0];
    let b = m[0][1];
    let c = m[1][0];
    let d = m[1][1];

    // Symmetric S = Mᵀ M = [[e, f],[f, g]].
    let e = a * a + c * c;
    let f = a * b + c * d;
    let g = b * b + d * d;

    // Eigen-decomposition of [[e,f],[f,g]] -> angle of V.
    let theta = 0.5 * (2.0 * f).atan2(e - g);
    let cv = theta.cos();
    let sv = theta.sin();
    let vmat = [[cv, -sv], [sv, cv]]; // columns are eigenvectors of S

    // Singular values from M*V columns.
    // First column of M*V:
    let mv00 = a * cv + b * sv;
    let mv10 = c * cv + d * sv;
    // Second column of M*V:
    let mv01 = a * (-sv) + b * cv;
    let mv11 = c * (-sv) + d * cv;

    let s0 = (mv00 * mv00 + mv10 * mv10).sqrt();
    let s1 = (mv01 * mv01 + mv11 * mv11).sqrt();

    // U columns = normalized M*V columns (handle zero singular values).
    let mut u00 = if s0 > 1e-15 { mv00 / s0 } else { 1.0 };
    let mut u10 = if s0 > 1e-15 { mv10 / s0 } else { 0.0 };
    let mut u01 = if s1 > 1e-15 { mv01 / s1 } else { -u10 };
    let mut u11 = if s1 > 1e-15 { mv11 / s1 } else { u00 };

    // Ensure U's second column is orthonormal even if s1 ~ 0.
    if s1 <= 1e-15 {
        u01 = -u10;
        u11 = u00;
    }
    let _ = (&mut u00, &mut u10, &mut u01, &mut u11);

    let umat = [[u00, u01], [u10, u11]];
    (umat, s0, s1, vmat)
}

fn matmul(a: [[f64; 2]; 2], b: [[f64; 2]; 2]) -> [[f64; 2]; 2] {
    [
        [
            a[0][0] * b[0][0] + a[0][1] * b[1][0],
            a[0][0] * b[0][1] + a[0][1] * b[1][1],
        ],
        [
            a[1][0] * b[0][0] + a[1][1] * b[1][0],
            a[1][0] * b[0][1] + a[1][1] * b[1][1],
        ],
    ]
}

fn transpose(a: [[f64; 2]; 2]) -> [[f64; 2]; 2] {
    [[a[0][0], a[1][0]], [a[0][1], a[1][1]]]
}

/// Normalized RMS residual of the best SIMILARITY (rotation+reflection+scale,
/// no shear) mapping `a` onto `b`. Both are Nx2. `None` if degenerate.
fn procrustes_residual(a: &[[f64; 2]], b: &[[f64; 2]]) -> Option<f64> {
    let n = a.len();
    if n < 3 || b.len() != n {
        return None;
    }
    // Centroids.
    let mut ca = [0.0, 0.0];
    let mut cb = [0.0, 0.0];
    for i in 0..n {
        ca[0] += a[i][0];
        ca[1] += a[i][1];
        cb[0] += b[i][0];
        cb[1] += b[i][1];
    }
    ca[0] /= n as f64;
    ca[1] /= n as f64;
    cb[0] /= n as f64;
    cb[1] /= n as f64;

    let ac: Vec<[f64; 2]> = a.iter().map(|p| [p[0] - ca[0], p[1] - ca[1]]).collect();
    let bc: Vec<[f64; 2]> = b.iter().map(|p| [p[0] - cb[0], p[1] - cb[1]]).collect();

    // Cross-covariance M = sum_i Ac_i^T * Bc_i  (2x2).
    let mut m = [[0.0; 2]; 2];
    let mut sum_a2 = 0.0;
    let mut sum_b2 = 0.0;
    for i in 0..n {
        m[0][0] += ac[i][0] * bc[i][0];
        m[0][1] += ac[i][0] * bc[i][1];
        m[1][0] += ac[i][1] * bc[i][0];
        m[1][1] += ac[i][1] * bc[i][1];
        sum_a2 += ac[i][0] * ac[i][0] + ac[i][1] * ac[i][1];
        sum_b2 += bc[i][0] * bc[i][0] + bc[i][1] * bc[i][1];
    }
    if sum_a2 < 1e-12 || sum_b2 < 1e-12 {
        return None;
    }

    // SVD M = U S Vᵀ.  Optimal rotation R = V Uᵀ (orthogonal, det +/-1),
    // maximizing trace(Rᵀ M).  This allows reflection.
    let (umat, s0, s1, vmat) = svd2x2(m);
    let r = matmul(vmat, transpose(umat)); // R = V Uᵀ
    let scale = (s0 + s1) / sum_a2;

    // residual_rms = sqrt(mean_i | s * R * Ac_i - Bc_i |² ) / sqrt(mean |Bc_i|²)
    let mut num = 0.0;
    for i in 0..n {
        // s * R * Ac_i
        let rx = scale * (r[0][0] * ac[i][0] + r[0][1] * ac[i][1]);
        let ry = scale * (r[1][0] * ac[i][0] + r[1][1] * ac[i][1]);
        let dx = rx - bc[i][0];
        let dy = ry - bc[i][1];
        num += dx * dx + dy * dy;
    }
    let mean_num = num / n as f64;
    let mean_b2 = sum_b2 / n as f64;
    Some((mean_num / mean_b2).sqrt())
}

#[test]
fn procrustes_helper_sanity() {
    // B = 2 * rotate(A) by 30deg => residual ~ 0.
    let a = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let th: f64 = std::f64::consts::FRAC_PI_6; // 30 deg
    let (c, s) = (th.cos(), th.sin());
    let b: Vec<[f64; 2]> = a
        .iter()
        .map(|p| [2.0 * (c * p[0] - s * p[1]), 2.0 * (s * p[0] + c * p[1])])
        .collect();
    let r = procrustes_residual(&a, &b).unwrap();
    assert!(r < 1e-9, "expected ~0 residual, got {r}");

    // Reflection should also fit (residual ~ 0).
    let bref: Vec<[f64; 2]> = a.iter().map(|p| [p[0], -p[1]]).collect();
    let rr = procrustes_residual(&a, &bref).unwrap();
    assert!(rr < 1e-9, "expected ~0 residual under reflection, got {rr}");
}

#[test]
fn congruence_gate() {
    let dec = std::fs::read("tests/fixtures/svga_block.dec.bin").unwrap();
    let table = uvtable::decode(&dec).unwrap();
    let geom =
        Geometry::parse(&std::fs::read("tests/fixtures/original.dat").unwrap(), 106).unwrap();
    let models = model::build_face_models(&table, &geom).unwrap();

    let mut residuals = Vec::new();
    for m in &models {
        let pts = m.points3d(&geom);
        if pts.len() < 3 {
            continue;
        }
        let flat = flatten_face(&pts);
        let orig: Vec<[f64; 2]> = m
            .orig_uv
            .iter()
            .map(|uv| [uv[0] as f64, uv[1] as f64])
            .collect();
        if let Some(r) = procrustes_residual(&orig, &flat) {
            residuals.push(r);
        }
    }

    let lt10 = residuals.iter().filter(|&&r| r < 0.10).count();
    let lt20 = residuals.iter().filter(|&&r| r < 0.20).count();
    println!("congruence: {} faces; <10%={} <20%={}", residuals.len(), lt10, lt20);
    assert!(lt10 >= 90, "expected >=90 faces <10% residual, got {lt10}");
    assert!(lt20 >= 109, "expected >=109 faces <20% residual, got {lt20}");
}
