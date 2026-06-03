use gp2uv::core::{dat::Geometry, model, patch, unwrap, uvtable};

#[test]
fn patch_roundtrips_on_temp_copy() {
    let Ok(src) = std::env::var("GP2_EXE") else {
        eprintln!("GP2_EXE not set; skip");
        return;
    };
    let dir = std::env::temp_dir();
    let tmp = dir.join("GP2_uvmapper_test_copy.exe");
    std::fs::copy(&src, &tmp).unwrap();
    let orig = uvtable::read_svga_from_exe(&std::fs::read(&tmp).unwrap()).unwrap();
    let geom =
        Geometry::parse(&std::fs::read("tests/fixtures/original.dat").unwrap(), 106).unwrap();
    let models = model::build_face_models(&orig, &geom).unwrap();
    let uw = unwrap::unwrap(&models, &geom, 25.0, unwrap::PackOpts::default());
    let patched = uvtable::patched_table(&orig, &models, &uw);
    let body = geom.body_face_indices();
    let report = patch::patch_svga(&tmp, &patched, &body, "20260602-000000").unwrap();
    assert!(report.backup_path.exists());
    assert_eq!(report.block_len, 11476);
    // re-read patched copy: table reproduces what we wrote
    let back = uvtable::read_svga_from_exe(&std::fs::read(&tmp).unwrap()).unwrap();
    for idx in body.iter().copied() {
        assert_eq!(
            back.face(idx).unwrap().verts,
            patched.face(idx).unwrap().verts,
            "face {idx}"
        );
    }
    // verify_exe still passes on the patched copy (size + magic preserved)
    patch::verify_exe(&std::fs::read(&tmp).unwrap()).unwrap();
    // everything OUTSIDE the block is unchanged vs the source
    let a = std::fs::read(&src).unwrap();
    let b = std::fs::read(&tmp).unwrap();
    assert_eq!(a.len(), b.len());
    assert_eq!(a[..uvtable::SVGA_FILE_OFF], b[..uvtable::SVGA_FILE_OFF]);
    assert_eq!(
        a[uvtable::SVGA_FILE_OFF + uvtable::SVGA_LEN..],
        b[uvtable::SVGA_FILE_OFF + uvtable::SVGA_LEN..]
    );
    std::fs::remove_file(&tmp).ok();
    let _ = std::fs::remove_file(report.backup_path);
}

#[test]
fn verify_dat_checks_length_and_magic() {
    // wrong length
    assert!(patch::verify_dat(&[0u8; 100]).is_err());
    // right length, wrong magic
    let mut bad = vec![0u8; patch::CAR_GEOM_LEN];
    bad[0] = 0xFF;
    bad[1] = 0xFF;
    assert!(patch::verify_dat(&bad).is_err());
    // right length + magic
    let mut ok = vec![0u8; patch::CAR_GEOM_LEN];
    ok[0] = 0x02;
    ok[1] = 0x80; // 0x8002 LE
    assert!(patch::verify_dat(&ok).is_ok());
}

#[test]
fn patch_with_geometry_installs_dat_block() {
    let Ok(src) = std::env::var("GP2_EXE") else {
        eprintln!("GP2_EXE not set; skip");
        return;
    };
    let tmp = std::env::temp_dir().join("GP2_uvmapper_geom_copy.exe");
    std::fs::copy(&src, &tmp).unwrap();
    let dat = std::fs::read("tests/fixtures/original.dat").unwrap();
    let orig = uvtable::read_svga_from_exe(&std::fs::read(&tmp).unwrap()).unwrap();
    let geom = Geometry::parse(&dat, 106).unwrap();
    let models = model::build_face_models(&orig, &geom).unwrap();
    let uw = unwrap::unwrap(&models, &geom, 23.0, unwrap::PackOpts::default());
    let patched = uvtable::patched_table(&orig, &models, &uw);
    let body = geom.body_face_indices();

    // Install a MODIFIED .dat (distinct from what the stock exe already holds),
    // so the test proves the install actually wrote the new bytes. Keep it valid
    // (length + magic), tweak some bytes well past the header.
    let mut dat_mod = dat.clone();
    for b in &mut dat_mod[1000..1064] {
        *b ^= 0x5A;
    }
    assert_ne!(dat_mod, dat, "modified dat must differ");

    let report = patch::patch_exe(&tmp, &patched, Some(&dat_mod), &body, "20260602-000000").unwrap();
    assert!(report.geometry_installed);

    let b = std::fs::read(&tmp).unwrap();
    // 1. the geometry block now equals the MODIFIED .dat byte-for-byte,
    //    and actually differs from the source exe's original block.
    assert_eq!(&b[patch::CAR_GEOM_OFF..patch::CAR_GEOM_OFF + patch::CAR_GEOM_LEN], &dat_mod[..]);
    let a_pre = std::fs::read(&src).unwrap();
    assert_ne!(
        a_pre[patch::CAR_GEOM_OFF..patch::CAR_GEOM_OFF + patch::CAR_GEOM_LEN],
        b[patch::CAR_GEOM_OFF..patch::CAR_GEOM_OFF + patch::CAR_GEOM_LEN],
        "geometry block must have changed"
    );
    // 2. UV table reproduces what we wrote
    let back = uvtable::read_svga_from_exe(&b).unwrap();
    for idx in body.iter().copied() {
        assert_eq!(back.face(idx).unwrap().verts, patched.face(idx).unwrap().verts);
    }
    // 3. size preserved + still a valid exe
    let a = std::fs::read(&src).unwrap();
    assert_eq!(a.len(), b.len());
    patch::verify_exe(&b).unwrap();
    // 4. bytes BEFORE the geometry block and BETWEEN the two regions are untouched
    assert_eq!(a[..patch::CAR_GEOM_OFF], b[..patch::CAR_GEOM_OFF]);
    assert_eq!(
        a[patch::CAR_GEOM_OFF + patch::CAR_GEOM_LEN..uvtable::SVGA_FILE_OFF],
        b[patch::CAR_GEOM_OFF + patch::CAR_GEOM_LEN..uvtable::SVGA_FILE_OFF]
    );
    // 5. bytes AFTER the UV block are untouched
    assert_eq!(
        a[uvtable::SVGA_FILE_OFF + uvtable::SVGA_LEN..],
        b[uvtable::SVGA_FILE_OFF + uvtable::SVGA_LEN..]
    );
    std::fs::remove_file(&tmp).ok();
    let _ = std::fs::remove_file(report.backup_path);
}
