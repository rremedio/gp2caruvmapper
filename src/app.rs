//! egui desktop shell for the GP2 Car UV Mapper.
//!
//! This module is intentionally thin: every piece of real logic lives in
//! [`gp2uv::app_core::AppCore`], which is GUI-free and headless-testable. The
//! `App::update` body only handles widgets, file dialogs, the texture cache and
//! the (binary-only) wall-clock timestamp.

use std::time::{SystemTime, UNIX_EPOCH};

use eframe::egui;
use gp2uv::app_core::AppCore;

/// Tint-group colours for the labelled preview. Index 0/1/2 are the symmetric
/// layout's slices (top/centre, left, right); the rest cycle so dense-mode
/// islands stay distinguishable.
const TINT_PALETTE: [egui::Color32; 10] = [
    egui::Color32::from_rgb(40, 150, 40),  // green   – top / centre
    egui::Color32::from_rgb(50, 95, 220),  // blue    – left
    egui::Color32::from_rgb(215, 55, 50),  // red     – right
    egui::Color32::from_rgb(225, 140, 0),  // orange
    egui::Color32::from_rgb(160, 70, 200), // purple
    egui::Color32::from_rgb(0, 150, 150),  // teal
    egui::Color32::from_rgb(205, 0, 120),  // magenta
    egui::Color32::from_rgb(120, 120, 0),  // olive
    egui::Color32::from_rgb(0, 110, 180),  // steel
    egui::Color32::from_rgb(150, 80, 40),  // brown
];

/// Draw the unwrap as a crisp vector view: each face filled by its tint group,
/// outlined, with its index centred in a readable font. `zoom` is px per atlas
/// unit (the atlas is 256x164).
fn paint_labelled(
    ui: &mut egui::Ui,
    uw: &gp2uv::core::unwrap::Unwrap,
    zoom: f32,
    recovered: &[usize],
) {
    let canvas = egui::vec2(256.0 * zoom, 164.0 * zoom);
    let (resp, painter) = ui.allocate_painter(canvas, egui::Sense::hover());
    let origin = resp.rect.min;
    painter.rect_filled(resp.rect, 0.0, egui::Color32::from_gray(238));
    let font = egui::FontId::proportional((zoom * 2.6).clamp(8.0, 30.0));
    for (idx, poly) in uw.iter_faces() {
        if poly.len() < 2 {
            continue;
        }
        let pts: Vec<egui::Pos2> = poly
            .iter()
            .map(|&[x, y]| origin + egui::vec2(x as f32 * zoom, y as f32 * zoom))
            .collect();
        let base = TINT_PALETTE[uw.tint(idx).unwrap_or(0) as usize % TINT_PALETTE.len()];
        let fill = egui::Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 70);
        painter.add(egui::Shape::convex_polygon(
            pts.clone(),
            fill,
            egui::Stroke::new(1.0, base),
        ));
        // Recovered (collapsed-vertRef -> edge-walk) faces get a bold gold outline.
        if recovered.contains(&idx) {
            painter.add(egui::Shape::closed_line(
                pts.clone(),
                egui::Stroke::new(2.5, egui::Color32::from_rgb(255, 190, 0)),
            ));
        }
        let cx = pts.iter().map(|p| p.x).sum::<f32>() / pts.len() as f32;
        let cy = pts.iter().map(|p| p.y).sum::<f32>() / pts.len() as f32;
        painter.text(
            egui::pos2(cx, cy),
            egui::Align2::CENTER_CENTER,
            idx.to_string(),
            font.clone(),
            egui::Color32::BLACK,
        );
    }
}

pub struct UiApp {
    core: AppCore,
    /// Cached preview texture; rebuilt whenever the unwrap/labels change.
    texture: Option<egui::TextureHandle>,
    /// eps value the cached texture was rendered at (detect slider changes).
    last_eps: f32,
    /// Two-click guard for the destructive patch action.
    patch_armed: bool,
    /// Zoom factor for the labelled vector preview (px per atlas unit).
    label_zoom: f32,
    /// Folder to open the .dat file dialog in first (last session's .dat dir).
    dat_start_dir: Option<std::path::PathBuf>,
}

impl UiApp {
    pub fn new() -> Self {
        let mut core = AppCore::new();
        // Remember the last GP2.EXE: auto-load it so the app comes up ready.
        let (recent_exe, recent_dat) = AppCore::recent_paths();
        if let Some(exe) = recent_exe {
            if exe.exists() {
                let _ = core.load_exe(exe);
            }
        }
        let dat_start_dir = recent_dat.and_then(|d| d.parent().map(|p| p.to_path_buf()));
        Self {
            core,
            texture: None,
            last_eps: f32::NAN,
            patch_armed: false,
            label_zoom: 3.0,
            dat_start_dir,
        }
    }

    /// Force the preview texture to be rebuilt on the next frame.
    fn invalidate_texture(&mut self) {
        self.texture = None;
    }

    /// (Re)build the preview texture if needed and return a handle clone.
    fn ensure_texture(&mut self, ctx: &egui::Context) -> Option<egui::TextureHandle> {
        let stale = self.texture.is_none() || self.last_eps != self.core.eps_deg;
        if stale {
            if let Some((w, h, rgba)) = self.core.preview_rgba() {
                let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
                let tex = ctx.load_texture("uv_preview", img, egui::TextureOptions::NEAREST);
                self.last_eps = self.core.eps_deg;
                self.texture = Some(tex);
            } else {
                self.texture = None;
            }
        }
        self.texture.clone()
    }
}

impl Default for UiApp {
    fn default() -> Self {
        Self::new()
    }
}

/// `YYYYMMDD-HHMMSS`-ish timestamp. Falls back to an epoch-seconds string if the
/// clock is unavailable. The clock is used ONLY here, in the binary.
fn timestamp() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => {
            let secs = d.as_secs();
            // Civil-from-days (Howard Hinnant's algorithm), UTC.
            let days = (secs / 86_400) as i64;
            let sod = secs % 86_400;
            let (hh, mm, ss) = (sod / 3600, (sod % 3600) / 60, sod % 60);
            let z = days + 719_468;
            let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
            let doe = z - era * 146_097;
            let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
            let y = yoe + era * 400;
            let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
            let mp = (5 * doy + 2) / 153;
            let d_ = doy - (153 * mp + 2) / 5 + 1;
            let m = if mp < 10 { mp + 3 } else { mp - 9 };
            let y = if m <= 2 { y + 1 } else { y };
            format!("{y:04}{m:02}{d_:02}-{hh:02}{mm:02}{ss:02}")
        }
        Err(_) => "00000000-000000".to_string(),
    }
}

impl eframe::App for UiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // --- Top: file bar -------------------------------------------------
        egui::TopBottomPanel::top("files").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Open GP2.EXE…").clicked() {
                    let start = self
                        .core
                        .exe_path
                        .as_ref()
                        .and_then(|p| p.parent().map(|p| p.to_path_buf()));
                    let mut dlg = rfd::FileDialog::new().add_filter("GP2 executable", &["exe", "EXE"]);
                    if let Some(dir) = start {
                        dlg = dlg.set_directory(dir);
                    }
                    if let Some(path) = dlg.pick_file() {
                        let _ = self.core.load_exe(path);
                        self.patch_armed = false;
                        self.invalidate_texture();
                    }
                }
                if ui.button("Open .dat…").clicked() {
                    let start = self
                        .core
                        .dat_path
                        .as_ref()
                        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                        .or_else(|| self.dat_start_dir.clone());
                    let mut dlg = rfd::FileDialog::new().add_filter("Car geometry", &["dat", "DAT"]);
                    if let Some(dir) = start {
                        dlg = dlg.set_directory(dir);
                    }
                    if let Some(path) = dlg.pick_file() {
                        let _ = self.core.load_dat(path);
                        self.invalidate_texture();
                    }
                }
            });

            ui.horizontal(|ui| {
                match (&self.core.exe_path, &self.core.orig_table) {
                    (Some(p), Some(t)) => ui.label(format!(
                        "EXE: {} — {} faces present, {} real",
                        p.display(),
                        t.faces_present(),
                        t.real_face_indices().len()
                    )),
                    _ => ui.label("EXE: (none loaded)"),
                };
            });
            ui.horizontal(|ui| {
                match (&self.core.dat_path, &self.core.geom) {
                    (Some(p), Some(g)) => {
                        ui.label(format!("DAT: {} — {} points", p.display(), g.points.len()))
                    }
                    _ => ui.label("DAT: (none loaded)"),
                };
            });
        });

        // --- Bottom: status log -------------------------------------------
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.label("Status:");
            egui::ScrollArea::vertical()
                .max_height(140.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    let n = self.core.status.len();
                    let start = n.saturating_sub(12);
                    for line in &self.core.status[start..] {
                        ui.monospace(line);
                    }
                });
        });

        // --- Left: controls + actions -------------------------------------
        egui::SidePanel::left("controls")
            .resizable(false)
            .default_width(240.0)
            .show(ctx, |ui| {
                ui.heading("Controls");

                // Layout selector (applies to both modes).
                let mut layout_changed = false;
                egui::ComboBox::from_label("Layout")
                    .selected_text(self.core.layout_mode.label())
                    .show_ui(ui, |ui| {
                        for m in gp2uv::core::unwrap::LayoutMode::ALL {
                            if ui
                                .selectable_value(&mut self.core.layout_mode, m, m.label())
                                .clicked()
                            {
                                layout_changed = true;
                            }
                        }
                    });
                let dense = self.core.layout_mode == gp2uv::core::unwrap::LayoutMode::Dense;

                // Weld angle + packing only apply to the dense layout.
                if dense {
                    let eps_resp = ui.add(
                        egui::Slider::new(&mut self.core.eps_deg, 0.0..=60.0).text("weld angle"),
                    );
                    if eps_resp.changed() {
                        layout_changed = true;
                    }
                }

                ui.separator();

                if dense {
                    egui::ComboBox::from_label("Packing")
                        .selected_text(self.core.pack_strategy.label())
                        .show_ui(ui, |ui| {
                            for s in gp2uv::core::unwrap::PackStrategy::ALL {
                                if ui
                                    .selectable_value(&mut self.core.pack_strategy, s, s.label())
                                    .clicked()
                                {
                                    layout_changed = true;
                                }
                            }
                        });
                    if ui
                        .checkbox(&mut self.core.orient, "Rotate islands to minimize space")
                        .changed()
                    {
                        layout_changed = true;
                    }
                }
                if layout_changed {
                    let _ = self.core.recompute_if_ready();
                    self.invalidate_texture();
                }

                if ui
                    .checkbox(
                        &mut self.core.recover_collapsed,
                        "Recover collapsed faces from geometry (experimental)",
                    )
                    .on_hover_text(
                        "When a face's stored UV vertices collapse to a degenerate \
                         polygon, rebuild it from the .dat edge-walk geometry (same \
                         vertex count only). Fixes faces that render untextured.",
                    )
                    .changed()
                {
                    let _ = self.core.recompute_if_ready();
                    self.invalidate_texture();
                }

                ui.separator();
                ui.label(format!("islands: {}", self.core.n_islands));
                if !self.core.recovered_faces.is_empty() {
                    let list = self
                        .core
                        .recovered_faces
                        .iter()
                        .map(|f| f.to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    ui.label(format!(
                        "recovered {} faces (gold outline):",
                        self.core.n_recovered
                    ));
                    ui.label(&list);
                }
                ui.label(format!("max stretch: {:.1}%", self.core.max_stretch_pct));
                ui.label(format!("ink-fill: {:.1}%", self.core.ink_fill * 100.0));
                ui.label(format!(
                    "fits: {}",
                    self.core
                        .unwrap
                        .as_ref()
                        .map(|u| u.fits)
                        .unwrap_or(false)
                ));

                ui.separator();
                ui.heading("Actions");

                ui.add_enabled_ui(self.core.ready(), |ui| {
                    if ui.button("Save BMP…").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Bitmap", &["bmp"])
                            .set_file_name("template.bmp")
                            .save_file()
                        {
                            let _ = self.core.save_bmp(&path);
                        }
                    }
                });

                ui.separator();
                ui.add_enabled_ui(self.core.ready(), |ui| {
                    ui.checkbox(
                        &mut self.core.install_geometry,
                        "Also install 3D geometry from this .dat",
                    )
                    .on_hover_text(
                        "Writes the loaded .dat's car shape into the exe too, so the \
                         rendered geometry matches these UVs. Leave on unless the same \
                         geometry is already installed.",
                    );
                    if !self.patch_armed {
                        if ui.button("Patch GP2.EXE…").clicked() {
                            self.patch_armed = true;
                        }
                    } else {
                        let faces = self
                            .core
                            .orig_table
                            .as_ref()
                            .map(|t| t.real_face_indices().len())
                            .unwrap_or(0);
                        let exe = self
                            .core
                            .exe_path
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default();
                        let geom_line = if self.core.install_geometry {
                            "\nAlso installs the .dat's 3D geometry."
                        } else {
                            ""
                        };
                        ui.colored_label(
                            egui::Color32::YELLOW,
                            format!(
                                "Patch {exe}?\nWrites {faces} faces in place.{geom_line}\nCreates a .bak backup."
                            ),
                        );
                        ui.horizontal(|ui| {
                            if ui.button("Confirm patch").clicked() {
                                let ts = timestamp();
                                match self.core.patch(&ts) {
                                    Ok(rep) => self.core.status.push(format!(
                                        "Patched {} faces{}; backup {}",
                                        rep.faces,
                                        if rep.geometry_installed {
                                            " + 3D geometry"
                                        } else {
                                            ""
                                        },
                                        rep.backup_path.display()
                                    )),
                                    Err(e) => self.core.status.push(e),
                                }
                                self.patch_armed = false;
                            }
                            if ui.button("Cancel").clicked() {
                                self.patch_armed = false;
                            }
                        });
                    }
                });

                ui.separator();
                if ui.button("Restore backup…").clicked() {
                    if let (Some(target), Some(backup)) = (
                        self.core.exe_path.clone(),
                        rfd::FileDialog::new()
                            .add_filter("Backup", &["bak", "*"])
                            .pick_file(),
                    ) {
                        match gp2uv::core::patch::restore(&backup, &target) {
                            Ok(()) => self.core.status.push(format!(
                                "Restored {} from {}",
                                target.display(),
                                backup.display()
                            )),
                            Err(e) => self.core.status.push(format!("Restore failed: {e:?}")),
                        }
                    } else {
                        self.core
                            .status
                            .push("Restore needs an EXE loaded and a .bak selected".into());
                    }
                }
            });

        // --- Centre: two previews side by side ----------------------------
        // Left: the clean template (exactly what gets saved / painted on).
        // Right: a high-res vector view, colour-tinted by slice/island with
        // always-on readable face numbers.
        let tex = self.ensure_texture(ctx);
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.columns(2, |cols| {
                cols[0].heading("Template (clean)");
                match &tex {
                    Some(tex) => {
                        let size = tex.size_vec2() * 3.0;
                        egui::ScrollArea::both().id_salt("clean_preview").show(
                            &mut cols[0],
                            |ui| {
                                ui.image((tex.id(), size));
                            },
                        );
                    }
                    None => {
                        cols[0].label("Load a GP2.EXE and a car .dat to see the unwrap.");
                    }
                }

                cols[1].heading("Labels");
                cols[1].add(
                    egui::Slider::new(&mut self.label_zoom, 1.5..=10.0).text("zoom"),
                );
                let zoom = self.label_zoom;
                let recovered = self.core.recovered_faces.as_slice();
                match self.core.unwrap.as_ref() {
                    Some(uw) => {
                        egui::ScrollArea::both().id_salt("labelled_preview").show(
                            &mut cols[1],
                            |ui| paint_labelled(ui, uw, zoom, recovered),
                        );
                    }
                    None => {
                        cols[1].label("Load a GP2.EXE and a car .dat to see the unwrap.");
                    }
                }
            });
        });
    }
}
