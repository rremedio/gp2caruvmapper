use gp2uv::core::dat::Header;
#[test]
fn parses_original_dat_header() {
    let bytes = std::fs::read("tests/fixtures/original.dat").unwrap();
    let h = Header::parse(&bytes).unwrap();
    assert_eq!(h.magic as u16, 0x8002);
}
