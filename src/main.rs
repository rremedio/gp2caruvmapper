//! GP2 Car UV Mapper — desktop entry point.
//!
//! The window shell lives in [`app`]; all real logic is in the GUI-free
//! `gp2uv::app_core::AppCore`.

mod app;

use app::UiApp;

fn main() -> eframe::Result<()> {
    // The 256x164 preview is drawn at 3x (768x492); the minimum inner size below
    // reserves room for it plus the side controls, top bar and status strip so the
    // image area is never cropped by the window. Initial size is a bit larger.
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 860.0])
            .with_min_inner_size([1120.0, 780.0]),
        ..Default::default()
    };
    eframe::run_native(
        "GP2 Car UV Mapper",
        native_options,
        Box::new(|_cc| Ok(Box::new(UiApp::new()))),
    )
}
