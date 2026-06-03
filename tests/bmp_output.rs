use gp2uv::core::{bmp, dat::Geometry, model, unwrap, uvtable};

#[test]
fn bmp_header_palette_size_valid() {
    let table =
        uvtable::decode(&std::fs::read("tests/fixtures/svga_block.dec.bin").unwrap()).unwrap();
    let geom =
        Geometry::parse(&std::fs::read("tests/fixtures/original.dat").unwrap(), 106).unwrap();
    let models = model::build_face_models(&table, &geom).unwrap();
    let uw = unwrap::unwrap(&models, &geom, 25.0, unwrap::PackOpts::default());
    let png = bmp::write_template(&uw, &bmp::Opts::default());
    assert_eq!(&png[0..2], b"BM");
    let off = u32::from_le_bytes(png[10..14].try_into().unwrap());
    assert_eq!(off, 14 + 40 + 1024);
    let w = i32::from_le_bytes(png[18..22].try_into().unwrap());
    let h = i32::from_le_bytes(png[22..26].try_into().unwrap());
    assert_eq!((w, h), (256, 164));
    let bpp = u16::from_le_bytes(png[28..30].try_into().unwrap());
    assert_eq!(bpp, 8);
    assert_eq!(png.len(), (14 + 40 + 1024 + 256 * 164) as usize);
    // background present and some wireframe pixels set
    let pix = &png[(off as usize)..];
    assert!(pix.contains(&0), "has green bg");
    assert!(pix.contains(&1), "has wireframe");
    // Write out for eyeballing (optional, helpful).
    let _ = std::fs::write("/tmp/gp2_template_preview.bmp", &png);
}
