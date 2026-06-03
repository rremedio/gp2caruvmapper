use gp2uv::app_core::AppCore;

#[test]
fn new_has_defaults() {
    let c = AppCore::new();
    assert_eq!(c.eps_deg, 23.0);
    assert!(c.labels);
    assert!(!c.ready());
    assert_eq!(c.n_islands, 0);
    assert!(c.recover_collapsed, "recovery defaults on");
    assert_eq!(c.n_recovered, 0);
}

#[test]
fn stock_recompute_recovers_nothing() {
    let Ok(exe) = std::env::var("GP2_EXE") else {
        eprintln!("skip stock_recompute_recovers_nothing: GP2_EXE unset");
        return;
    };
    let mut c = AppCore::new();
    c.load_exe(exe.into()).unwrap();
    c.load_dat("tests/fixtures/original.dat".into()).unwrap();
    c.recompute().unwrap();
    // Stock geometry has no collapsed faces, so recovery is a no-op.
    assert_eq!(c.n_recovered, 0);
}

#[test]
fn core_pipeline_headless() {
    let Ok(exe) = std::env::var("GP2_EXE") else {
        eprintln!("skip core_pipeline_headless: GP2_EXE unset");
        return;
    };
    let mut c = AppCore::new();
    c.load_exe(exe.clone().into()).unwrap();
    c.load_dat("tests/fixtures/original.dat".into()).unwrap();
    c.recompute().unwrap();
    assert!(c.ready());
    assert!(c.n_islands > 0);
    let bmp = c.bmp_bytes().unwrap();
    assert_eq!(&bmp[0..2], b"BM");

    // Preview should be 256x164 RGBA.
    let (w, h, rgba) = c.preview_rgba().unwrap();
    assert_eq!((w, h), (256, 164));
    assert_eq!(rgba.len(), 256 * 164 * 4);

    // patch a temp copy
    let tmp = std::env::temp_dir().join("gp2_appcore_test.exe");
    std::fs::copy(&exe, &tmp).unwrap();
    let mut c2 = AppCore::new();
    c2.load_exe(tmp.clone()).unwrap();
    c2.load_dat("tests/fixtures/original.dat".into()).unwrap();
    c2.recompute().unwrap();
    let rep = c2.patch("20260602-000000").unwrap();
    assert_eq!(rep.block_len, 11476);
    std::fs::remove_file(&tmp).ok();
    std::fs::remove_file(rep.backup_path).ok();
}
