//! Render the symmetric layout to a BMP for visual inspection.
use gp2uv::core::{bmp, dat::Geometry, model, symmetric, uvtable};
fn main() {
    let table =
        uvtable::decode(&std::fs::read("tests/fixtures/svga_block.dec.bin").unwrap()).unwrap();
    for tag in ["original", "60s", "70s", "SWC"] {
        let geom = Geometry::parse(
            &std::fs::read(format!("tests/fixtures/{tag}.dat")).unwrap(),
            106,
        )
        .unwrap();
        let models = model::build_face_models(&table, &geom).unwrap();
        let uw = symmetric::unwrap_symmetric(&models, &geom);
        let bmp = bmp::write_template(&uw, &bmp::Opts::default());
        let out = format!("/tmp/sym_{tag}.bmp");
        std::fs::write(&out, bmp).unwrap();
        println!("wrote {out}");
    }
}
