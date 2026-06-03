//! GUI-free application core for the GP2 Car UV Mapper.
//!
//! All real logic lives here so it can be exercised by a headless integration
//! test (no display required). The egui `App` in the binary is a thin shell
//! that drives this struct.
//!
//! No clock/timestamp is generated here — the binary supplies the timestamp to
//! [`AppCore::patch`] so the library stays deterministic.

use std::path::{Path, PathBuf};

use crate::core::bmp::{self, Opts};
use crate::core::dat::Geometry;
use crate::core::model::{build_face_models_opts, ModelOpts};
use crate::core::palette::PALETTE;
use crate::core::patch::{self, PatchReport};
use crate::core::symmetric::unwrap_symmetric;
use crate::core::unwrap::{unwrap, LayoutMode, PackOpts, PackStrategy, Unwrap};
use crate::core::uvtable::{self, UvTable};

/// Atlas / preview dimensions (match the BMP writer).
const PREVIEW_W: usize = 256;
const PREVIEW_H: usize = 164;

/// Candidate `.dat` scale-section start offsets, tried in order.
const DAT_STARTS: [i32; 5] = [106, 78, 54, 82, 110];

/// Wireframe palette index (a dark colour, visible on the green background).
const WIRE_IDX: u8 = 1;
/// Label palette index (contrasting).
const LABEL_IDX: u8 = 255;

pub struct AppCore {
    pub exe_path: Option<PathBuf>,
    pub exe_bytes: Option<Vec<u8>>,
    pub orig_table: Option<UvTable>,
    pub geom: Option<Geometry>,
    pub dat_path: Option<PathBuf>,
    /// Raw bytes of the loaded `.dat` (for optionally installing geometry).
    pub dat_bytes: Option<Vec<u8>>,
    pub eps_deg: f32,
    pub labels: bool,
    /// Also write the loaded `.dat`'s 3D block into the exe when patching, so
    /// the rendered geometry and our UVs are guaranteed to match.
    pub install_geometry: bool,
    /// Layout: GP2-style symmetric (default) or dense MaxRects.
    pub layout_mode: LayoutMode,
    /// Rectangle-packing strategy used by [`unwrap`] (dense mode only).
    pub pack_strategy: PackStrategy,
    /// Rotate islands to their min-area rectangle before packing.
    pub orient: bool,
    /// Recover collapsed-texture faces from `.dat` edge-walk geometry.
    pub recover_collapsed: bool,
    pub unwrap: Option<Unwrap>,
    pub status: Vec<String>,
    pub n_islands: usize,
    /// Faces recovered from edge-walk geometry in the last recompute.
    pub n_recovered: usize,
    pub max_stretch_pct: f64,
    /// Ink-fill of the last unwrap: sum of face polygon areas / atlas area.
    pub ink_fill: f64,
}

impl Default for AppCore {
    fn default() -> Self {
        Self::new()
    }
}

impl AppCore {
    pub fn new() -> Self {
        Self {
            exe_path: None,
            exe_bytes: None,
            orig_table: None,
            geom: None,
            dat_path: None,
            dat_bytes: None,
            eps_deg: 23.0,
            labels: true,
            install_geometry: true,
            layout_mode: LayoutMode::Gp2Symmetric,
            pack_strategy: PackStrategy::MaxRects,
            orient: true,
            recover_collapsed: true,
            unwrap: None,
            status: Vec::new(),
            n_islands: 0,
            n_recovered: 0,
            max_stretch_pct: 0.0,
            ink_fill: 0.0,
        }
    }

    fn log(&mut self, msg: impl Into<String>) {
        self.status.push(msg.into());
    }

    /// Read + verify a GP2.EXE and pull its SVGA UV table.
    pub fn load_exe(&mut self, path: PathBuf) -> Result<(), String> {
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                let m = format!("Failed to read EXE {}: {e}", path.display());
                self.log(&m);
                return Err(m);
            }
        };
        if let Err(e) = patch::verify_exe(&bytes) {
            let m = format!("Not a patchable GP2.EXE: {e:?}");
            self.log(&m);
            return Err(m);
        }
        let table = match uvtable::read_svga_from_exe(&bytes) {
            Some(t) => t,
            None => {
                let m = "Could not decode SVGA UV table from EXE".to_string();
                self.log(&m);
                return Err(m);
            }
        };
        let real = table.real_face_indices().len();
        let present = table.faces_present();
        self.exe_path = Some(path.clone());
        self.exe_bytes = Some(bytes);
        self.orig_table = Some(table);
        self.log(format!(
            "Loaded EXE {} ({present} faces present, {real} real)",
            path.display()
        ));
        if uvtable::looks_patched(self.orig_table.as_ref().unwrap()) {
            self.log(
                "Note: this EXE looks already patched — layout uses the embedded \
                 factory table, so results are unaffected (idempotent).",
            );
        }
        let _ = self.recompute_if_ready();
        Ok(())
    }

    /// Read a car `.dat` and resolve geometry, trying known start offsets.
    pub fn load_dat(&mut self, path: PathBuf) -> Result<(), String> {
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                let m = format!("Failed to read .dat {}: {e}", path.display());
                self.log(&m);
                return Err(m);
            }
        };
        let mut geom = None;
        let mut used = 0;
        for &start in &DAT_STARTS {
            if let Some(g) = Geometry::parse(&bytes, start) {
                used = start;
                geom = Some(g);
                break;
            }
        }
        let geom = match geom {
            Some(g) => g,
            None => {
                let m = format!("Could not parse geometry from {}", path.display());
                self.log(&m);
                return Err(m);
            }
        };
        let npoints = geom.points.len();
        self.dat_path = Some(path.clone());
        self.dat_bytes = Some(bytes);
        self.geom = Some(geom);
        self.log(format!(
            "Loaded .dat {} ({npoints} points, start {used})",
            path.display()
        ));
        let _ = self.recompute_if_ready();
        Ok(())
    }

    /// Recompute the unwrap from the loaded table + geometry.
    pub fn recompute(&mut self) -> Result<(), String> {
        if self.orig_table.is_none() {
            let m = "Cannot recompute: no EXE loaded".to_string();
            self.log(&m);
            return Err(m);
        }
        let geom = match self.geom.as_ref() {
            Some(g) => g,
            None => {
                let m = "Cannot recompute: no .dat loaded".to_string();
                self.log(&m);
                return Err(m);
            }
        };
        // Anchor layout to the embedded FACTORY table, not the loaded EXE's table
        // (which may already be patched by us) — keeps recompute idempotent.
        let factory = uvtable::factory_table();
        let (models, n_recovered) = match build_face_models_opts(
            &factory,
            geom,
            ModelOpts {
                recover_collapsed: self.recover_collapsed,
            },
        ) {
            Some(m) => m,
            None => {
                let m = "Failed to build face models".to_string();
                self.log(&m);
                return Err(m);
            }
        };
        let pack = PackOpts {
            strategy: self.pack_strategy,
            orient: self.orient,
        };
        let uw = match self.layout_mode {
            LayoutMode::Dense => unwrap(&models, geom, self.eps_deg as f64, pack),
            LayoutMode::Gp2Symmetric => unwrap_symmetric(&models, geom),
        };
        // `geom`/`table` borrows end above; now safe to mutate self.
        self.n_recovered = n_recovered;
        if n_recovered > 0 {
            self.log(format!(
                "Recovered {n_recovered} collapsed faces from geometry"
            ));
        }
        let islands = uw.islands();
        self.n_islands = islands.len();
        self.max_stretch_pct = islands
            .iter()
            .map(|i| i.max_stretch_pct)
            .fold(0.0_f64, f64::max);
        // Ink-fill: sum of face polygon areas / atlas area (256*164).
        let mut ink = 0.0;
        for (_idx, poly) in uw.iter_faces() {
            let n = poly.len();
            if n < 3 {
                continue;
            }
            let mut a = 0i64;
            for i in 0..n {
                let j = (i + 1) % n;
                a += (poly[i][0] as i64) * (poly[j][1] as i64)
                    - (poly[j][0] as i64) * (poly[i][1] as i64);
            }
            ink += (a as f64).abs() / 2.0;
        }
        self.ink_fill = ink / (PREVIEW_W as f64 * PREVIEW_H as f64);
        match self.layout_mode {
            LayoutMode::Gp2Symmetric => self.log(format!(
                "Recomputed: {} GP2 clusters, max foreshorten {:.1}%, ink {:.1}% ({})",
                self.n_islands,
                self.max_stretch_pct,
                self.ink_fill * 100.0,
                self.layout_mode.label(),
            )),
            LayoutMode::Dense => self.log(format!(
                "Recomputed: {} islands, max stretch {:.1}%, weld {:.1} deg, {} {}, ink {:.1}%",
                self.n_islands,
                self.max_stretch_pct,
                self.eps_deg,
                self.pack_strategy.label(),
                if self.orient {
                    "oriented"
                } else {
                    "axis-aligned"
                },
                self.ink_fill * 100.0,
            )),
        }
        if !uw.fits {
            self.log(
                "⚠ unwrap did not fit the 256×164 atlas at this weld angle — \
                 lower the angle or simplify the mesh; preview is clamped.",
            );
        }
        self.unwrap = Some(uw);
        Ok(())
    }

    /// Recompute only if both inputs are present (used after each load / eps
    /// change). Silently no-ops when not ready.
    pub fn recompute_if_ready(&mut self) -> Result<(), String> {
        if self.orig_table.is_some() && self.geom.is_some() {
            self.recompute()
        } else {
            Ok(())
        }
    }

    fn opts(&self) -> Opts {
        Opts {
            labels: self.labels,
            wire_idx: WIRE_IDX,
            label_idx: LABEL_IDX,
        }
    }

    /// Render the current unwrap to an 8bpp BMP byte buffer.
    pub fn bmp_bytes(&self) -> Result<Vec<u8>, String> {
        let uw = self
            .unwrap
            .as_ref()
            .ok_or_else(|| "No unwrap to render".to_string())?;
        Ok(bmp::write_template(uw, &self.opts()))
    }

    /// Write the current unwrap as a BMP file.
    pub fn save_bmp(&self, path: &Path) -> Result<(), String> {
        let bytes = self.bmp_bytes()?;
        std::fs::write(path, &bytes)
            .map_err(|e| format!("Failed to write BMP {}: {e}", path.display()))
    }

    /// Render the current unwrap to a 256x164 RGBA buffer for the GUI preview.
    ///
    /// Decodes the BMP we just produced (single source of truth) into top-origin
    /// RGBA via [`PALETTE`].
    pub fn preview_rgba(&self) -> Option<(usize, usize, Vec<u8>)> {
        let bmp = self.bmp_bytes().ok()?;
        // BITMAPFILEHEADER(14) + BITMAPINFOHEADER(40) + palette(1024) = 54+1024.
        let pixel_off = 14 + 40 + 1024;
        let row_stride = (PREVIEW_W + 3) & !3;
        if bmp.len() < pixel_off + row_stride * PREVIEW_H {
            return None;
        }
        let mut rgba = vec![0u8; PREVIEW_W * PREVIEW_H * 4];
        // BMP rows are bottom-up; flip back to top-origin.
        for y in 0..PREVIEW_H {
            let src_row = PREVIEW_H - 1 - y;
            let src = pixel_off + src_row * row_stride;
            for x in 0..PREVIEW_W {
                let idx = bmp[src + x] as usize;
                let [r, g, b] = PALETTE[idx];
                let d = (y * PREVIEW_W + x) * 4;
                rgba[d] = r;
                rgba[d + 1] = g;
                rgba[d + 2] = b;
                rgba[d + 3] = 255;
            }
        }
        Some((PREVIEW_W, PREVIEW_H, rgba))
    }

    /// Patch the loaded GP2.EXE in place with the current unwrap.
    pub fn patch(&self, timestamp: &str) -> Result<PatchReport, String> {
        // Presence guard only; the table we patch FROM is the embedded factory
        // table, so patching is idempotent regardless of the EXE's current state.
        let _ = self
            .orig_table
            .as_ref()
            .ok_or_else(|| "No EXE loaded to patch".to_string())?;
        let uw = self
            .unwrap
            .as_ref()
            .ok_or_else(|| "No unwrap computed; load a .dat first".to_string())?;
        let path = self
            .exe_path
            .as_ref()
            .ok_or_else(|| "No EXE path".to_string())?;
        let geom = self
            .geom
            .as_ref()
            .ok_or_else(|| "No .dat loaded".to_string())?;
        let factory = uvtable::factory_table();
        let (models, _) = build_face_models_opts(
            &factory,
            geom,
            ModelOpts {
                recover_collapsed: self.recover_collapsed,
            },
        )
        .ok_or_else(|| "Failed to build face models".to_string())?;
        let patched = uvtable::patched_table(&factory, &models, uw);
        // The body faces (jam_id 530) are exactly the faces we unwrap + patch.
        // Patch only the ones that actually carry a UV slot (== the models),
        // so the in-place patch never references a missing entry.
        let body: Vec<usize> = models.iter().map(|m| m.face_idx).collect();
        let geom_dat = if self.install_geometry {
            if self.dat_bytes.is_none() {
                return Err("Install-geometry is on but no .dat is loaded".to_string());
            }
            self.dat_bytes.as_deref()
        } else {
            None
        };
        patch::patch_exe(path, &patched, geom_dat, &body, timestamp)
            .map_err(|e| format!("Patch failed: {e:?}"))
    }

    pub fn ready(&self) -> bool {
        self.orig_table.is_some() && self.geom.is_some() && self.unwrap.is_some()
    }
}
