//! Face model: links each real UV face to its `.dat` 3D points.
//!
//! Each UV vertex carries a `vert_ref`; the global `.dat` point index is
//! `vert_ref / 24`. A [`FaceModel`] bundles, per real face, the resolved
//! point indices, the original `(u, v)` pairs, and the raw vert refs.

use crate::core::{
    dat::{Geometry, Point3D},
    uvtable::UvTable,
};

/// One real UV face linked to its `.dat` points.
pub struct FaceModel {
    pub face_idx: usize,
    pub point_indices: Vec<usize>, // vert_ref/24 (or edge-walk when recovered)
    pub orig_uv: Vec<[u16; 2]>,
    pub vert_refs: Vec<u16>,
    /// True when this face's collapsed `vertRef` polygon was replaced with its
    /// `.dat` edge-walk polygon (both point_indices and vert_refs).
    pub recovered: bool,
}

/// Options controlling face-model construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelOpts {
    /// Replace a real face's degenerate `vertRef` polygon with its `.dat`
    /// edge-walk polygon when the edge-walk is valid and has the SAME vertex
    /// count (an in-place patch constraint).
    pub recover_collapsed: bool,
}

impl Default for ModelOpts {
    fn default() -> Self {
        Self {
            recover_collapsed: true,
        }
    }
}

/// Newell 3D polygon area. Returns 0.0 for fewer than 3 points.
pub fn area3d(points: &[Point3D]) -> f64 {
    let n = points.len();
    if n < 3 {
        return 0.0;
    }
    let (mut nx, mut ny, mut nz) = (0.0f64, 0.0, 0.0);
    for i in 0..n {
        let a = points[i];
        let b = points[(i + 1) % n];
        nx += (a.y as f64 - b.y as f64) * (a.z as f64 + b.z as f64);
        ny += (a.z as f64 - b.z as f64) * (a.x as f64 + b.x as f64);
        nz += (a.x as f64 - b.x as f64) * (a.y as f64 + b.y as f64);
    }
    (nx * nx + ny * ny + nz * nz).sqrt() / 2.0
}

/// Minimum 3D area below which a face is treated as collapsed/degenerate.
const DEGENERATE_AREA: f64 = 1.0;

/// Build a [`FaceModel`] for every real body face (`.dat` jam_id == 530), with
/// recovery on. Body faces with no SVGA UV slot are skipped.
///
/// Returns `None` if any `vert_ref/24` is out of range for `geom.points`.
pub fn build_face_models(table: &UvTable, geom: &Geometry) -> Option<Vec<FaceModel>> {
    build_face_models_opts(table, geom, ModelOpts::default()).map(|(models, _)| models)
}

/// Build face models with explicit [`ModelOpts`]. Returns the models plus the
/// number of faces recovered from edge-walk geometry.
///
/// Recovery rule for each real face (when `opts.recover_collapsed`):
/// 1. Resolve `point_indices` from `vert_ref/24` and compute its 3D area.
/// 2. If that area is ~0 (collapsed), try the `.dat` edge-walk for the face.
///    Only override when the edge-walk is `Some`, has EXACTLY the original
///    vertex count (an in-place patch constraint), all indices are in range,
///    and its polygon is non-degenerate. Then use the edge-walk indices and set
///    `vert_refs[k] = (ew[k] * 24)`. Otherwise leave the face as-is.
pub fn build_face_models_opts(
    table: &UvTable,
    geom: &Geometry,
    opts: ModelOpts,
) -> Option<(Vec<FaceModel>, usize)> {
    let mut out = Vec::new();
    let mut recovered_count = 0usize;
    for idx in geom.body_face_indices() {
        // A body face with no SVGA UV slot has nothing to unwrap/patch; skip it.
        let f = match table.face(idx) {
            Some(f) => f,
            None => continue,
        };
        let mut pi = Vec::new();
        let mut uv = Vec::new();
        let mut vr = Vec::new();
        for v in &f.verts {
            let p = (v.vert_ref / 24) as usize;
            if p >= geom.points.len() {
                return None;
            }
            pi.push(p);
            uv.push([v.u, v.v]);
            vr.push(v.vert_ref);
        }

        let mut recovered = false;
        if opts.recover_collapsed {
            let area_vr: f64 = {
                let pts: Vec<Point3D> = pi.iter().map(|&p| geom.points[p]).collect();
                area3d(&pts)
            };
            if area_vr < DEGENERATE_AREA {
                if let Some(ew) = geom.edge_walk(idx) {
                    let same_count = ew.len() == pi.len();
                    let in_range = ew.iter().all(|&p| p < geom.points.len());
                    if same_count && in_range {
                        let ew_pts: Vec<Point3D> = ew.iter().map(|&p| geom.points[p]).collect();
                        if area3d(&ew_pts) >= DEGENERATE_AREA {
                            for (k, &p) in ew.iter().enumerate() {
                                pi[k] = p;
                                vr[k] = (p * 24) as u16;
                            }
                            recovered = true;
                            recovered_count += 1;
                        }
                    }
                }
            }
        }

        out.push(FaceModel {
            face_idx: idx,
            point_indices: pi,
            orig_uv: uv,
            vert_refs: vr,
            recovered,
        });
    }
    Some((out, recovered_count))
}

impl FaceModel {
    /// The resolved 3D points for this face, in vertex order.
    pub fn points3d(&self, geom: &Geometry) -> Vec<Point3D> {
        self.point_indices.iter().map(|&p| geom.points[p]).collect()
    }
}
