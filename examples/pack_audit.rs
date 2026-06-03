//! Audit the current packer across many .dat files.
//! Usage: GP2_EXE=/path/GP2.EXE cargo run --release --example pack_audit -- <shapes_dir> [eps] [strategy] [orient]
//!   strategy: shelf | skyline | maxrects   (default maxrects)
//!   orient:   0 | 1                          (default 1)
use gp2uv::core::unwrap::{PackOpts, PackStrategy};
use gp2uv::core::{dat::Geometry, model, unwrap, uvtable};

fn poly_area(p: &[[i32; 2]]) -> f64 {
    let n = p.len();
    if n < 3 { return 0.0; }
    let mut a = 0i64;
    for i in 0..n {
        let j = (i + 1) % n;
        a += (p[i][0] as i64) * (p[j][1] as i64) - (p[j][0] as i64) * (p[i][1] as i64);
    }
    (a as f64).abs() / 2.0
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().expect("need shapes dir");
    let eps: f64 = args.next().map(|s| s.parse().unwrap()).unwrap_or(25.0);
    let strategy = match args.next().as_deref() {
        Some("shelf") => PackStrategy::Shelf,
        Some("skyline") => PackStrategy::Skyline,
        Some("maxrects") | None => PackStrategy::MaxRects,
        Some(other) => panic!("unknown strategy {other:?} (shelf|skyline|maxrects)"),
    };
    let orient = match args.next().as_deref() {
        Some("0") => false,
        Some("1") | None => true,
        Some(other) => panic!("unknown orient {other:?} (0|1)"),
    };
    let opts = PackOpts { strategy, orient };
    let exe = std::fs::read(std::env::var("GP2_EXE").expect("GP2_EXE")).unwrap();
    let table = uvtable::read_svga_from_exe(&exe).expect("read table");

    let mut entries: Vec<_> = std::fs::read_dir(&dir).unwrap()
        .filter_map(|e| e.ok()).map(|e| e.path())
        .filter(|p| p.extension().map(|x| x.eq_ignore_ascii_case("dat")).unwrap_or(false))
        .collect();
    entries.sort();

    let canvas = (256.0_f64) * (164.0);
    let (mut ok, mut parse_fail, mut model_fail, mut oob_files, mut unfit_files) = (0, 0, 0, 0, 0);
    let mut util_sum = 0.0; let mut ink_sum = 0.0; let mut worst = Vec::new();

    for path in &entries {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let bytes = match std::fs::read(path) { Ok(b) => b, Err(_) => { continue; } };
        let geom = [106, 78, 54, 82, 110].iter().find_map(|&s| Geometry::parse(&bytes, s));
        let Some(geom) = geom else { parse_fail += 1; println!("{name:<34} PARSE_FAIL"); continue; };
        let Some(models) = model::build_face_models(&table, &geom) else {
            model_fail += 1; println!("{name:<34} MODEL_FAIL (point idx out of range; npoints={})", geom.points.len()); continue; };
        let uw = unwrap::unwrap(&models, &geom, eps, opts);
        // metrics
        let (mut minu, mut minv, mut maxu, mut maxv) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        let mut oob = 0; let mut ink = 0.0;
        for (_idx, poly) in uw.iter_faces() {
            for &[u, v] in poly {
                if !(0..256).contains(&u) || !(0..164).contains(&v) { oob += 1; }
                minu = minu.min(u); maxu = maxu.max(u); minv = minv.min(v); maxv = maxv.max(v);
            }
            ink += poly_area(poly);
        }
        let bbox = ((maxu - minu).max(0) as f64) * ((maxv - minv).max(0) as f64) / canvas;
        let inkfrac = ink / canvas;
        util_sum += bbox; ink_sum += inkfrac;
        if oob > 0 { oob_files += 1; }
        if !uw.fits { unfit_files += 1; }
        ok += 1;
        worst.push((inkfrac, name.clone(), uw.fits, oob, bbox));
        let flag = if oob > 0 { " *OOB*" } else { "" };
        let fit = if uw.fits { "fit " } else { "UNFIT" };
        println!("{name:<34} {fit} bbox-util={:>5.1}% ink={:>5.1}% oob={oob}{flag}",
                 bbox * 100.0, inkfrac * 100.0);
    }

    worst.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    println!("\n==== SUMMARY (eps={eps} strategy={strategy:?} orient={orient}) ====");
    println!("files: {}  ok={ok} parse_fail={parse_fail} model_fail={model_fail}", entries.len());
    println!("out-of-bounds files: {oob_files}   unfit (clamped) files: {unfit_files}");
    if ok > 0 {
        println!("avg bbox-utilization: {:.1}%   avg ink-fill: {:.1}%",
                 util_sum / ok as f64 * 100.0, ink_sum / ok as f64 * 100.0);
        println!("lowest ink-fill 5: {:?}", worst.iter().take(5).map(|w| (w.1.as_str(), (w.0 * 100.0) as i32)).collect::<Vec<_>>());
    }
}
