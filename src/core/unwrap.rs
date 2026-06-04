//! Per-face flatten: project a 3D polygon onto its own plane (2D).
//!
//! This reproduces the SHAPE GP2 uses for its per-face UV unwrap: a
//! uniform-scale rigid flatten of the face's `.dat` points. We build an
//! in-plane basis from Newell's normal and the first edge, then project.

use crate::core::dat::{Geometry, Point3D};
use crate::core::model::FaceModel;

/// Flatten a 3D polygon onto its supporting plane.
///
/// Basis: Newell's normal `N`; `U = normalize((P1-P0) - (N-component))`;
/// `V = N x U`. Each point maps to `[(P-P0)·U, (P-P0)·V]`. `P0` maps to the
/// origin and `P1` lands on the +U axis.
pub fn flatten_face(points: &[Point3D]) -> Vec<[f64; 2]> {
    let p: Vec<[f64; 3]> = points
        .iter()
        .map(|q| [q.x as f64, q.y as f64, q.z as f64])
        .collect();
    let n = p.len();
    if n == 0 {
        return Vec::new();
    }

    // Newell's normal.
    let mut nx = 0.0;
    let mut ny = 0.0;
    let mut nz = 0.0;
    for i in 0..n {
        let a = p[i];
        let b = p[(i + 1) % n];
        nx += (a[1] - b[1]) * (a[2] + b[2]);
        ny += (a[2] - b[2]) * (a[0] + b[0]);
        nz += (a[0] - b[0]) * (a[1] + b[1]);
    }
    let normalize = |v: [f64; 3]| -> [f64; 3] {
        let m = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if m == 0.0 {
            [0.0, 0.0, 0.0]
        } else {
            [v[0] / m, v[1] / m, v[2] / m]
        }
    };
    let nrm = normalize([nx, ny, nz]);

    let p0 = p[0];
    // U = first edge with N-component removed, normalized.
    let mut e = if n > 1 {
        [p[1][0] - p0[0], p[1][1] - p0[1], p[1][2] - p0[2]]
    } else {
        [1.0, 0.0, 0.0]
    };
    let dot_en = e[0] * nrm[0] + e[1] * nrm[1] + e[2] * nrm[2];
    e = [
        e[0] - dot_en * nrm[0],
        e[1] - dot_en * nrm[1],
        e[2] - dot_en * nrm[2],
    ];
    let u = normalize(e);
    // V = N x U.
    let v = [
        nrm[1] * u[2] - nrm[2] * u[1],
        nrm[2] * u[0] - nrm[0] * u[2],
        nrm[0] * u[1] - nrm[1] * u[0],
    ];

    p.iter()
        .map(|q| {
            let d = [q[0] - p0[0], q[1] - p0[1], q[2] - p0[2]];
            [
                d[0] * u[0] + d[1] * u[1] + d[2] * u[2],
                d[0] * v[0] + d[1] * v[1] + d[2] * v[2],
            ]
        })
        .collect()
}

/// Newell's unit normal for a 3D polygon. Returns `[0,0,0]` if degenerate.
fn face_normal(points: &[Point3D]) -> [f64; 3] {
    let p: Vec<[f64; 3]> = points
        .iter()
        .map(|q| [q.x as f64, q.y as f64, q.z as f64])
        .collect();
    let n = p.len();
    let mut nx = 0.0;
    let mut ny = 0.0;
    let mut nz = 0.0;
    for i in 0..n {
        let a = p[i];
        let b = p[(i + 1) % n];
        nx += (a[1] - b[1]) * (a[2] + b[2]);
        ny += (a[2] - b[2]) * (a[0] + b[0]);
        nz += (a[0] - b[0]) * (a[1] + b[1]);
    }
    let m = (nx * nx + ny * ny + nz * nz).sqrt();
    if m == 0.0 {
        [0.0, 0.0, 0.0]
    } else {
        [nx / m, ny / m, nz / m]
    }
}

/// Number of shared point indices between two faces (uses set semantics).
fn shared_count(a: &FaceModel, b: &FaceModel) -> usize {
    let mut count = 0;
    for &ia in &a.point_indices {
        if b.point_indices.contains(&ia) {
            count += 1;
        }
    }
    count
}

/// Angle in degrees between two unit normals, using the absolute dot product
/// so opposite-wound (anti-parallel) normals still count as coplanar (0deg).
fn normal_angle_deg(n1: [f64; 3], n2: [f64; 3]) -> f64 {
    let dot = (n1[0] * n2[0] + n1[1] * n2[1] + n1[2] * n2[2]).abs();
    dot.clamp(0.0, 1.0).acos().to_degrees()
}

/// A group of connected, near-coplanar faces (positions into the models slice).
pub struct Island {
    pub face_pos: Vec<usize>,
}

/// Simple union-find over `0..n`.
struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        UnionFind {
            parent: (0..n).collect(),
        }
    }
    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }
    fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra != rb {
            self.parent[ra] = rb;
        }
    }
}

/// Group faces into islands of connected, near-coplanar faces.
///
/// Two faces are adjacent iff they share >= 2 point indices (a shared edge).
/// Adjacent faces merge into one island when the angle between their Newell
/// normals is <= `eps_deg`.
pub fn build_islands(models: &[FaceModel], geom: &Geometry, eps_deg: f64) -> Vec<Island> {
    let n = models.len();
    let normals: Vec<[f64; 3]> = models
        .iter()
        .map(|m| face_normal(&m.points3d(geom)))
        .collect();

    let mut uf = UnionFind::new(n);
    for i in 0..n {
        for j in (i + 1)..n {
            if shared_count(&models[i], &models[j]) >= 2
                && normal_angle_deg(normals[i], normals[j]) <= eps_deg
            {
                uf.union(i, j);
            }
        }
    }

    // Collect members per root, preserving first-seen order of roots.
    let mut order: Vec<usize> = Vec::new();
    let mut groups: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
    for i in 0..n {
        let r = uf.find(i);
        groups.entry(r).or_insert_with(|| {
            order.push(r);
            Vec::new()
        });
        groups.get_mut(&r).unwrap().push(i);
    }

    order
        .into_iter()
        .map(|r| Island {
            face_pos: groups.remove(&r).unwrap(),
        })
        .collect()
}

// ===================== Task 10: unfold + pack =====================

/// Atlas canvas dimensions.
const ATLAS_W: f64 = 256.0;
const ATLAS_H: f64 = 164.0;

/// Which rectangle-packing strategy to use when laying islands into the atlas.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PackStrategy {
    Shelf,
    Skyline,
    MaxRects,
}

impl PackStrategy {
    pub fn label(self) -> &'static str {
        match self {
            Self::Shelf => "Shelf (fast)",
            Self::Skyline => "Skyline",
            Self::MaxRects => "MaxRects (densest)",
        }
    }
    pub const ALL: [PackStrategy; 3] = [Self::Shelf, Self::Skyline, Self::MaxRects];
}

/// Which layout the unwrap uses.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LayoutMode {
    /// Angle-welded islands packed densely (weld-angle + pack-strategy apply).
    Dense,
    /// GP2-style: canonical islands, planar projection, 3 mirror slices.
    Gp2Symmetric,
}

impl LayoutMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Dense => "Dense (MaxRects)",
            Self::Gp2Symmetric => "GP2 / Symmetric",
        }
    }
    pub const ALL: [LayoutMode; 2] = [Self::Gp2Symmetric, Self::Dense];
}

/// Packing options threaded through [`unwrap`].
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PackOpts {
    pub strategy: PackStrategy,
    /// Rotate each island to its minimum-area oriented rectangle before packing.
    pub orient: bool,
}

impl Default for PackOpts {
    fn default() -> Self {
        Self {
            strategy: PackStrategy::MaxRects,
            orient: true,
        }
    }
}

/// Diagnostic per-island stretch report.
pub struct IslandStretch {
    pub face_count: usize,
    pub max_stretch_pct: f64,
}

/// Final unwrap result: per-face atlas coords (in model vertex order) plus
/// per-island diagnostics.
pub struct Unwrap {
    /// face_idx -> atlas coords (model vertex order).
    faces: std::collections::HashMap<usize, Vec<[i32; 2]>>,
    /// Insertion order of face_idx for stable iteration.
    order: Vec<usize>,
    /// Packed integer bbox per island: [x0, y0, x1, y1] (x1/y1 exclusive-ish).
    island_boxes: Vec<[i32; 4]>,
    stretches: Vec<IslandStretch>,
    /// face_idx -> colour-group index for the labelled preview. The symmetric
    /// layout uses the slice (0 = top/centre, 1 = left, 2 = right); the dense
    /// layout uses the island index. Empty entries default to group 0.
    tint: std::collections::HashMap<usize, u8>,
    /// True when the final packing fit the atlas without clamping/overflow.
    /// When false, coords were still produced but clamped into the canvas.
    pub fits: bool,
}

impl Unwrap {
    pub fn iter_faces(&self) -> impl Iterator<Item = (usize, &Vec<[i32; 2]>)> {
        self.order.iter().map(move |&idx| (idx, &self.faces[&idx]))
    }

    pub fn coords(&self, face_idx: usize) -> Option<&Vec<[i32; 2]>> {
        self.faces.get(&face_idx)
    }

    /// Colour-group index for a face (see [`Unwrap::tint`] field).
    pub fn tint(&self, face_idx: usize) -> Option<u8> {
        self.tint.get(&face_idx).copied()
    }

    /// Count of overlapping island-bbox pixels (0 = none). Uses a coverage
    /// grid over the atlas; any pixel covered by >=2 island boxes counts once.
    pub fn max_overlap(&self) -> u64 {
        let w = ATLAS_W as usize;
        let h = ATLAS_H as usize;
        let mut cover = vec![0u16; w * h];
        for b in &self.island_boxes {
            let x0 = b[0].max(0) as usize;
            let y0 = b[1].max(0) as usize;
            let x1 = (b[2].max(0) as usize).min(w);
            let y1 = (b[3].max(0) as usize).min(h);
            for y in y0..y1 {
                for x in x0..x1 {
                    cover[y * w + x] += 1;
                }
            }
        }
        cover.iter().filter(|&&c| c >= 2).count() as u64
    }

    pub fn islands(&self) -> &[IslandStretch] {
        &self.stretches
    }

    /// Build an [`Unwrap`] from already-computed parts (used by the symmetric
    /// layout, which assembles its own per-face atlas coords).
    pub(crate) fn from_parts(
        faces: std::collections::HashMap<usize, Vec<[i32; 2]>>,
        order: Vec<usize>,
        island_boxes: Vec<[i32; 4]>,
        stretches: Vec<IslandStretch>,
        tint: std::collections::HashMap<usize, u8>,
        fits: bool,
    ) -> Self {
        Self {
            faces,
            order,
            island_boxes,
            stretches,
            tint,
            fits,
        }
    }
}

/// 2D edge length.
fn len2(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// 3D edge length.
fn len3(a: Point3D, b: Point3D) -> f64 {
    (((a.x - b.x) as f64).powi(2) + ((a.y - b.y) as f64).powi(2) + ((a.z - b.z) as f64).powi(2))
        .sqrt()
}

/// An island unfolded into a single 2D plane (placed coords per face, in model
/// vertex order), plus its local bbox.
struct PlacedIsland {
    /// face position (into models slice) -> placed 2D coords (model vertex order).
    faces: Vec<(usize, Vec<[f64; 2]>)>,
    bbox: [f64; 4], // x0,y0,x1,y1
    stretch: IslandStretch,
}

/// Unfold one island via edge-matching, producing placed 2D coords per face.
fn unfold_island(island: &Island, models: &[FaceModel], geom: &Geometry) -> PlacedIsland {
    let fp = &island.face_pos;
    let m = fp.len();

    // 3D points + local flatten per face (cached).
    let pts3d: Vec<Vec<Point3D>> = fp.iter().map(|&p| models[p].points3d(geom)).collect();
    let local: Vec<Vec<[f64; 2]>> = pts3d.iter().map(|p| flatten_face(p)).collect();

    // placed[k] = Some(coords) once face fp[k] is placed.
    let mut placed: Vec<Option<Vec<[f64; 2]>>> = vec![None; m];
    // Map global point_index -> placed 2D position (the unfold frame).
    let mut pos: std::collections::HashMap<usize, [f64; 2]> = std::collections::HashMap::new();

    // Seed: face 0 placed as its own flatten.
    placed[0] = Some(local[0].clone());
    for (k, &pi) in models[fp[0]].point_indices.iter().enumerate() {
        pos.entry(pi).or_insert(local[0][k]);
    }

    // BFS over remaining faces.
    let mut remaining: Vec<usize> = (1..m).collect();
    let mut progressed = true;
    while progressed && !remaining.is_empty() {
        progressed = false;
        let mut still: Vec<usize> = Vec::new();
        for &k in &remaining {
            let face = &models[fp[k]];
            // Find >=2 of this face's point indices that are already placed.
            let mut anchors: Vec<(usize, [f64; 2], [f64; 2])> = Vec::new();
            // (local_vertex_index, local_2d, target_2d)
            for (vi, &pi) in face.point_indices.iter().enumerate() {
                if let Some(&tgt) = pos.get(&pi) {
                    anchors.push((vi, local[k][vi], tgt));
                }
            }
            if anchors.len() < 2 {
                still.push(k);
                continue;
            }

            // Use two anchors A,B to derive the rigid similarity.
            let (ai, la, ta) = anchors[0];
            let (_bi, lb, tb) = anchors[1];

            let placed_local = place_face(&local[k], la, lb, ta, tb, &anchors, ai);

            // Record placed coords; populate pos for any newly placed indices.
            for (vi, &pi) in face.point_indices.iter().enumerate() {
                pos.entry(pi).or_insert(placed_local[vi]);
            }
            placed[k] = Some(placed_local);
            progressed = true;
        }
        remaining = still;
    }

    // Any face not reachable via shared edges (shouldn't happen within a true
    // island, but guard): place it at its own flatten as a fallback.
    for k in 0..m {
        if placed[k].is_none() {
            placed[k] = Some(local[k].clone());
        }
    }

    // Compute island bbox over all placed coords.
    let mut x0 = f64::INFINITY;
    let mut y0 = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut y1 = f64::NEG_INFINITY;
    for c in placed.iter().flatten() {
        for &[x, y] in c {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }

    // Stretch diagnostic: per-edge len2d/len3d ratio across the whole island.
    let mut ratios: Vec<f64> = Vec::new();
    for (k, coords) in placed.iter().enumerate() {
        let coords = coords.as_ref().unwrap();
        let p3 = &pts3d[k];
        let n = coords.len();
        for e in 0..n {
            let a = e;
            let b = (e + 1) % n;
            let d3 = len3(p3[a], p3[b]);
            let d2 = len2(coords[a], coords[b]);
            if d3 > 1e-9 {
                ratios.push(d2 / d3);
            }
        }
    }
    let max_stretch_pct = if ratios.len() >= 2 {
        let mut sorted = ratios.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = sorted[sorted.len() / 2];
        ratios
            .iter()
            .map(|r| ((r / median) - 1.0).abs() * 100.0)
            .fold(0.0, f64::max)
    } else {
        0.0
    };

    let faces: Vec<(usize, Vec<[f64; 2]>)> = fp
        .iter()
        .zip(placed)
        .map(|(&p, c)| (p, c.unwrap()))
        .collect();

    PlacedIsland {
        faces,
        bbox: [x0, y0, x1, y1],
        stretch: IslandStretch {
            face_count: m,
            max_stretch_pct,
        },
    }
}

/// Compute the rigid 2D similarity (rotation + translation, plus reflection if
/// needed) mapping `local` so that local A->target A and local B->target B
/// exactly, then apply it to all of `local`. Reflection is chosen so a third
/// anchor (if available) lands on the correct side.
fn place_face(
    local: &[[f64; 2]],
    la: [f64; 2],
    lb: [f64; 2],
    ta: [f64; 2],
    tb: [f64; 2],
    anchors: &[(usize, [f64; 2], [f64; 2])],
    _ai: usize,
) -> Vec<[f64; 2]> {
    // Vectors A->B in both frames.
    let dl = [lb[0] - la[0], lb[1] - la[1]];
    let dt = [tb[0] - ta[0], tb[1] - ta[1]];
    let ll = (dl[0] * dl[0] + dl[1] * dl[1]).sqrt();
    let lt = (dt[0] * dt[0] + dt[1] * dt[1]).sqrt();

    // Degenerate guard.
    if ll < 1e-12 || lt < 1e-12 {
        return local.to_vec();
    }

    // Both frames are isometric copies of the same 3D face, so |A->B| matches.
    // Rotation angle from local edge to target edge.
    let al = dl[1].atan2(dl[0]);
    let at = dt[1].atan2(dt[0]);

    // Try both orientations (no reflection / reflection across the A->B line)
    // and pick the one whose third anchor best matches its known target.
    let apply = |reflect: bool| -> Vec<[f64; 2]> {
        // First, optionally reflect local across the line through A along dir dl
        // (reflection about the A->B axis flips the perpendicular component).
        let theta = at - al;
        let (c, s) = (theta.cos(), theta.sin());
        local
            .iter()
            .map(|&p| {
                // translate so A at origin (local frame)
                let mut vx = p[0] - la[0];
                let mut vy = p[1] - la[1];
                if reflect {
                    // reflect about the local A->B axis: component along dl kept,
                    // perpendicular negated.
                    let ux = dl[0] / ll;
                    let uy = dl[1] / ll;
                    let along = vx * ux + vy * uy;
                    let perp = -vx * uy + vy * ux; // signed perpendicular
                    let perp = -perp;
                    vx = along * ux - perp * uy;
                    vy = along * uy + perp * ux;
                }
                // rotate by theta, then translate to target A.
                let rx = vx * c - vy * s;
                let ry = vx * s + vy * c;
                [rx + ta[0], ry + ta[1]]
            })
            .collect()
    };

    let cand_no = apply(false);
    let cand_yes = apply(true);

    // Score by third anchor mismatch (if any anchor beyond the first two has a
    // known target).
    let score = |cand: &[[f64; 2]]| -> f64 {
        let mut err = 0.0;
        let mut found = false;
        for &(vi, _l, t) in anchors.iter().skip(2) {
            let d = len2(cand[vi], t);
            err += d;
            found = true;
        }
        if !found {
            // No third anchor: prefer the non-reflected candidate by returning
            // 0 for both (handled by caller default).
            0.0
        } else {
            err
        }
    };

    // If there's a 3rd+ anchor, choose lower error; else default to no-reflect.
    let has_third = anchors.len() >= 3;
    if has_third {
        if score(&cand_no) <= score(&cand_yes) {
            cand_no
        } else {
            cand_yes
        }
    } else {
        cand_no
    }
}

/// Andrew's monotone-chain convex hull of a set of 2D points. Returns hull
/// vertices in counter-clockwise order (no repeated last point). Degenerate
/// inputs (<3 unique points) return the deduplicated points as-is.
fn convex_hull(pts: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut p: Vec<[f64; 2]> = pts.to_vec();
    p.sort_by(|a, b| {
        a[0].partial_cmp(&b[0])
            .unwrap()
            .then(a[1].partial_cmp(&b[1]).unwrap())
    });
    p.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-12 && (a[1] - b[1]).abs() < 1e-12);
    let n = p.len();
    if n < 3 {
        return p;
    }
    // Cross product of OA x OB (z component).
    let cross = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| -> f64 {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut hull: Vec<[f64; 2]> = Vec::with_capacity(2 * n);
    // Lower hull.
    for &pt in &p {
        while hull.len() >= 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], pt) <= 0.0 {
            hull.pop();
        }
        hull.push(pt);
    }
    // Upper hull.
    let lower_len = hull.len() + 1;
    for &pt in p.iter().rev() {
        while hull.len() >= lower_len
            && cross(hull[hull.len() - 2], hull[hull.len() - 1], pt) <= 0.0
        {
            hull.pop();
        }
        hull.push(pt);
    }
    hull.pop();
    hull
}

/// Compute the rotation (radians) that, applied to all island vertices, makes
/// the island's minimum-area oriented bounding rectangle axis-aligned. Uses the
/// rotating-calipers theorem: the min-area rectangle shares an edge with the
/// convex hull. Returns 0.0 when orientation can't help (degenerate hull).
pub(crate) fn min_area_rect_angle(pts: &[[f64; 2]]) -> f64 {
    let hull = convex_hull(pts);
    let h = hull.len();
    if h < 3 {
        return 0.0;
    }
    let mut best_area = f64::INFINITY;
    let mut best_angle = 0.0;
    for i in 0..h {
        let a = hull[i];
        let b = hull[(i + 1) % h];
        let ex = b[0] - a[0];
        let ey = b[1] - a[1];
        let len = (ex * ex + ey * ey).sqrt();
        if len < 1e-12 {
            continue;
        }
        // Angle that rotates this edge onto the horizontal axis.
        let theta = -ey.atan2(ex);
        let (c, s) = (theta.cos(), theta.sin());
        let mut x0 = f64::INFINITY;
        let mut y0 = f64::INFINITY;
        let mut x1 = f64::NEG_INFINITY;
        let mut y1 = f64::NEG_INFINITY;
        for &p in &hull {
            let rx = p[0] * c - p[1] * s;
            let ry = p[0] * s + p[1] * c;
            x0 = x0.min(rx);
            y0 = y0.min(ry);
            x1 = x1.max(rx);
            y1 = y1.max(ry);
        }
        let area = (x1 - x0) * (y1 - y0);
        if area < best_area {
            best_area = area;
            best_angle = theta;
        }
    }
    best_angle
}

/// Rotate a placed island in-place so its min-area oriented rectangle is
/// axis-aligned, and recompute its bbox. No-op if the rotation is ~0.
fn orient_island(island: &mut PlacedIsland) {
    let all: Vec<[f64; 2]> = island
        .faces
        .iter()
        .flat_map(|(_, c)| c.iter().copied())
        .collect();
    if all.len() < 3 {
        return;
    }
    let theta = min_area_rect_angle(&all);
    if theta.abs() < 1e-9 {
        return;
    }
    let (c, s) = (theta.cos(), theta.sin());
    let mut x0 = f64::INFINITY;
    let mut y0 = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut y1 = f64::NEG_INFINITY;
    for (_, coords) in island.faces.iter_mut() {
        for p in coords.iter_mut() {
            let rx = p[0] * c - p[1] * s;
            let ry = p[0] * s + p[1] * c;
            *p = [rx, ry];
            x0 = x0.min(rx);
            y0 = y0.min(ry);
            x1 = x1.max(rx);
            y1 = y1.max(ry);
        }
    }
    island.bbox = [x0, y0, x1, y1];
}

/// A scaled island ready for packing.
struct Packable {
    idx: usize, // index into placed islands
    h: f64,     // height used for shelf ordering
}

/// Shelf-pack scaled islands into ATLAS_W x ATLAS_H. Returns per-island
/// placement offsets (origin of each island's normalized bbox) and whether all
/// fit. Allows 90 deg rotation of an island if it helps fit a shelf.
///
/// `dims` are the (w,h) of each island AFTER scaling by `s`. Returns
/// `(offsets, rotated, fits)` where offset[i] = [x,y] placement and
/// rotated[i] indicates the island was turned 90 deg.
fn shelf_pack(dims: &[[f64; 2]], gutter: f64) -> (Vec<[f64; 2]>, Vec<bool>, bool) {
    let n = dims.len();
    // Order islands by (rotated-to-tallest) height descending.
    let mut order: Vec<Packable> = (0..n)
        .map(|i| Packable {
            idx: i,
            h: dims[i][1],
        })
        .collect();
    order.sort_by(|a, b| b.h.partial_cmp(&a.h).unwrap());

    let mut offsets = vec![[0.0f64; 2]; n];
    let mut rotated = vec![false; n];

    let mut cursor_x = gutter;
    let mut cursor_y = gutter;
    let mut shelf_h = 0.0f64;

    for pk in &order {
        let i = pk.idx;
        let (mut w, mut h) = (dims[i][0], dims[i][1]);
        let mut rot = false;

        // If too wide for the canvas, try rotating.
        if w + 2.0 * gutter > ATLAS_W && h + 2.0 * gutter <= ATLAS_W {
            std::mem::swap(&mut w, &mut h);
            rot = true;
        }

        // New shelf if it doesn't fit horizontally on the current shelf.
        if cursor_x + w + gutter > ATLAS_W {
            // start a new shelf
            cursor_y += shelf_h + gutter;
            cursor_x = gutter;
            shelf_h = 0.0;
        }

        // Optional rotation to better fit the current shelf height (only if a
        // shelf is already established and rotation reduces height usage).
        if shelf_h > 0.0 && !rot {
            // If rotating makes the island fit under the existing shelf height
            // while it currently exceeds it, rotate.
            if h > shelf_h && w <= shelf_h && (cursor_x + h + gutter <= ATLAS_W) {
                std::mem::swap(&mut w, &mut h);
                rot = true;
            }
        }

        // Re-check new shelf after possible rotation.
        if cursor_x + w + gutter > ATLAS_W {
            cursor_y += shelf_h + gutter;
            cursor_x = gutter;
            shelf_h = 0.0;
        }

        offsets[i] = [cursor_x, cursor_y];
        rotated[i] = rot;
        cursor_x += w + gutter;
        shelf_h = shelf_h.max(h);

        if w + 2.0 * gutter > ATLAS_W {
            return (offsets, rotated, false);
        }
    }

    let total_h = cursor_y + shelf_h + gutter;
    let fits = total_h <= ATLAS_H;
    (offsets, rotated, fits)
}

/// Bottom-left skyline packer. Maintains a skyline as a list of horizontal
/// segments `(x, width, y)`; each rect is placed at the lowest skyline position
/// where it fits, left-most, allowing 90deg rotation. Rects packed tallest-first.
///
/// Each placed rect reserves `w+gutter` x `h+gutter` so neighbours keep a 1px
/// gutter. Returns `(offsets, rotated, fits)`.
fn skyline_pack(dims: &[[f64; 2]], gutter: f64) -> (Vec<[f64; 2]>, Vec<bool>, bool) {
    let n = dims.len();
    let mut offsets = vec![[0.0f64; 2]; n];
    let mut rotated = vec![false; n];

    // Order by max side desc (height-ish) for a denser skyline.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        dims[b][0]
            .max(dims[b][1])
            .partial_cmp(&dims[a][0].max(dims[a][1]))
            .unwrap()
    });

    // Skyline segments: (x, width, y). Start with one ground segment.
    let mut sky: Vec<(f64, f64, f64)> = vec![(0.0, ATLAS_W, 0.0)];

    // Find the lowest y at which a [w x h] rect can be placed starting at some
    // segment, returning (x, y) or None.
    let place = |sky: &[(f64, f64, f64)], w: f64, h: f64| -> Option<(f64, f64)> {
        let mut best: Option<(f64, f64)> = None; // (y, x)
        for i in 0..sky.len() {
            let x = sky[i].0;
            if x + w > ATLAS_W + 1e-9 {
                continue;
            }
            // Max y across all segments spanned by [x, x+w).
            let mut y = 0.0f64;
            let mut covered = 0.0f64;
            let mut j = i;
            while j < sky.len() && covered + 1e-9 < w {
                y = y.max(sky[j].2);
                covered += sky[j].1;
                j += 1;
            }
            if covered + 1e-9 < w {
                continue; // ran off the end
            }
            if y + h > ATLAS_H + 1e-9 {
                continue;
            }
            let better = match best {
                None => true,
                Some((by, bx)) => y < by - 1e-9 || (((y - by).abs() <= 1e-9) && x < bx),
            };
            if better {
                best = Some((y, x));
            }
        }
        best.map(|(y, x)| (x, y))
    };

    // Insert a rect of [w x h] at (x, y): raise the skyline over [x, x+w) to y+h.
    let insert = |sky: &mut Vec<(f64, f64, f64)>, x: f64, y: f64, w: f64, h: f64| {
        let top = y + h;
        let x_end = x + w;
        let mut out: Vec<(f64, f64, f64)> = Vec::with_capacity(sky.len() + 2);
        for &(sx, sw, sy) in sky.iter() {
            let sx_end = sx + sw;
            // Non-overlapping part: keep as-is.
            if sx_end <= x + 1e-9 || sx >= x_end - 1e-9 {
                out.push((sx, sw, sy));
                continue;
            }
            // Left remainder.
            if sx < x - 1e-9 {
                out.push((sx, x - sx, sy));
            }
            // Right remainder.
            if sx_end > x_end + 1e-9 {
                out.push((x_end, sx_end - x_end, sy));
            }
        }
        out.push((x, w, top));
        out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        // Merge adjacent equal-height segments.
        let mut merged: Vec<(f64, f64, f64)> = Vec::with_capacity(out.len());
        for seg in out {
            if let Some(last) = merged.last_mut() {
                if (last.2 - seg.2).abs() < 1e-9 && (last.0 + last.1 - seg.0).abs() < 1e-9 {
                    last.1 += seg.1;
                    continue;
                }
            }
            merged.push(seg);
        }
        *sky = merged;
    };

    for &i in &order {
        let w0 = dims[i][0] + gutter;
        let h0 = dims[i][1] + gutter;
        // Try both orientations; pick the lower placement.
        let a = place(&sky, w0, h0).map(|(x, y)| (x, y, w0, h0, false));
        let b = place(&sky, h0, w0).map(|(x, y)| (x, y, h0, w0, true));
        let chosen = match (a, b) {
            (Some(pa), Some(pb)) => {
                if pa.1 <= pb.1 {
                    Some(pa)
                } else {
                    Some(pb)
                }
            }
            (Some(pa), None) => Some(pa),
            (None, Some(pb)) => Some(pb),
            (None, None) => None,
        };
        match chosen {
            Some((x, y, w, h, rot)) => {
                offsets[i] = [x + gutter, y + gutter];
                rotated[i] = rot;
                insert(&mut sky, x, y, w, h);
            }
            None => return (offsets, rotated, false),
        }
    }
    (offsets, rotated, true)
}

/// MaxRects packer with Best-Short-Side-Fit (BSSF). Maintains a list of free
/// rectangles; each island is placed into the free rect minimizing the shorter
/// leftover side (trying both orientations). Free rects are split
/// guillotine-free and contained free rects are pruned. Islands inserted by
/// max(w,h) desc. Returns `(offsets, rotated, fits)`.
fn maxrects_pack(dims: &[[f64; 2]], gutter: f64) -> (Vec<[f64; 2]>, Vec<bool>, bool) {
    let n = dims.len();
    let mut offsets = vec![[0.0f64; 2]; n];
    let mut rotated = vec![false; n];

    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        dims[b][0]
            .max(dims[b][1])
            .partial_cmp(&dims[a][0].max(dims[a][1]))
            .unwrap()
    });

    // Free rectangles: [x, y, w, h].
    let mut free: Vec<[f64; 4]> = vec![[0.0, 0.0, ATLAS_W, ATLAS_H]];

    let fits_in = |fr: &[f64; 4], w: f64, h: f64| w <= fr[2] + 1e-9 && h <= fr[3] + 1e-9;

    for &i in &order {
        let w0 = dims[i][0] + gutter;
        let h0 = dims[i][1] + gutter;

        // BSSF: find free rect + orientation minimizing the shorter leftover.
        let mut best_score = (f64::INFINITY, f64::INFINITY); // (short, long)
        let mut best: Option<([f64; 2], f64, f64, bool)> = None; // (pos, w, h, rot)
        for fr in &free {
            for (w, h, rot) in [(w0, h0, false), (h0, w0, true)] {
                if !fits_in(fr, w, h) {
                    continue;
                }
                let leftover_h = (fr[2] - w).abs();
                let leftover_v = (fr[3] - h).abs();
                let short = leftover_h.min(leftover_v);
                let long = leftover_h.max(leftover_v);
                if short < best_score.0 - 1e-9
                    || ((short - best_score.0).abs() <= 1e-9 && long < best_score.1 - 1e-9)
                {
                    best_score = (short, long);
                    best = Some(([fr[0], fr[1]], w, h, rot));
                }
            }
        }

        let (pos, w, h, rot) = match best {
            Some(b) => b,
            None => return (offsets, rotated, false),
        };

        offsets[i] = [pos[0] + gutter, pos[1] + gutter];
        rotated[i] = rot;

        // Placed rect occupies [pos.x, pos.x+w) x [pos.y, pos.y+h).
        let px0 = pos[0];
        let py0 = pos[1];
        let px1 = pos[0] + w;
        let py1 = pos[1] + h;

        // Split every free rect that overlaps the placed rect (guillotine-free).
        let mut next_free: Vec<[f64; 4]> = Vec::with_capacity(free.len() + 4);
        for fr in &free {
            let fx0 = fr[0];
            let fy0 = fr[1];
            let fx1 = fr[0] + fr[2];
            let fy1 = fr[1] + fr[3];
            // No overlap -> keep.
            if px0 >= fx1 - 1e-9 || px1 <= fx0 + 1e-9 || py0 >= fy1 - 1e-9 || py1 <= fy0 + 1e-9 {
                next_free.push(*fr);
                continue;
            }
            // Left slab.
            if px0 > fx0 + 1e-9 {
                next_free.push([fx0, fy0, px0 - fx0, fr[3]]);
            }
            // Right slab.
            if px1 < fx1 - 1e-9 {
                next_free.push([px1, fy0, fx1 - px1, fr[3]]);
            }
            // Bottom slab.
            if py0 > fy0 + 1e-9 {
                next_free.push([fx0, fy0, fr[2], py0 - fy0]);
            }
            // Top slab.
            if py1 < fy1 - 1e-9 {
                next_free.push([fx0, py1, fr[2], fy1 - py1]);
            }
        }

        // Prune free rects fully contained in another.
        let contained = |a: &[f64; 4], b: &[f64; 4]| -> bool {
            a[0] >= b[0] - 1e-9
                && a[1] >= b[1] - 1e-9
                && a[0] + a[2] <= b[0] + b[2] + 1e-9
                && a[1] + a[3] <= b[1] + b[3] + 1e-9
        };
        let mut pruned: Vec<[f64; 4]> = Vec::with_capacity(next_free.len());
        for (a_i, a) in next_free.iter().enumerate() {
            if a[2] <= 1e-9 || a[3] <= 1e-9 {
                continue;
            }
            let mut keep = true;
            for (b_i, b) in next_free.iter().enumerate() {
                if a_i == b_i {
                    continue;
                }
                if contained(a, b) {
                    // If identical, keep only the lower index to avoid dropping both.
                    let identical = (a[0] - b[0]).abs() < 1e-9
                        && (a[1] - b[1]).abs() < 1e-9
                        && (a[2] - b[2]).abs() < 1e-9
                        && (a[3] - b[3]).abs() < 1e-9;
                    if !identical || b_i < a_i {
                        keep = false;
                        break;
                    }
                }
            }
            if keep {
                pruned.push(*a);
            }
        }
        free = pruned;
    }
    (offsets, rotated, true)
}

/// Dispatch to the chosen packer. All packers share the
/// `(offsets, rotated, fits)` contract over island `dims` (w,h after scale).
fn pack_with(
    strategy: PackStrategy,
    dims: &[[f64; 2]],
    gutter: f64,
) -> (Vec<[f64; 2]>, Vec<bool>, bool) {
    match strategy {
        PackStrategy::Shelf => shelf_pack(dims, gutter),
        PackStrategy::Skyline => skyline_pack(dims, gutter),
        PackStrategy::MaxRects => maxrects_pack(dims, gutter),
    }
}

/// Full unwrap: weld into islands, unfold each, uniform-scale shelf-pack into
/// 256x164, and quantize to integer atlas coords (model vertex order per face).
pub fn unwrap(models: &[FaceModel], geom: &Geometry, eps_deg: f64, pack: PackOpts) -> Unwrap {
    let islands = build_islands(models, geom, eps_deg);
    let mut placed: Vec<PlacedIsland> = islands
        .iter()
        .map(|isl| unfold_island(isl, models, geom))
        .collect();

    // Optionally rotate each island to its minimum-area oriented rectangle so
    // its axis-aligned bbox is as tight as possible before packing.
    if pack.orient {
        for p in placed.iter_mut() {
            orient_island(p);
        }
    }

    // Normalize each island to its own origin; record raw (unscaled) sizes.
    let raw_dims: Vec<[f64; 2]> = placed
        .iter()
        .map(|p| [p.bbox[2] - p.bbox[0], p.bbox[3] - p.bbox[1]])
        .collect();

    let gutter = 1.0;

    // Binary-search largest uniform scale s that fits.
    let fits_at = |s: f64| -> (Vec<[f64; 2]>, Vec<bool>, bool) {
        let dims: Vec<[f64; 2]> = raw_dims.iter().map(|d| [d[0] * s, d[1] * s]).collect();
        pack_with(pack.strategy, &dims, gutter)
    };

    let mut lo = 1e-3;
    let mut hi = 100.0;
    // Ensure hi doesn't fit and lo does (best-effort); just run binary search.
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        let (_o, _r, fits) = fits_at(mid);
        if fits {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let s = lo;
    let (offsets, rotated, fits) = fits_at(s);

    // Apply scale + offsets to every face vertex.
    let mut faces: std::collections::HashMap<usize, Vec<[i32; 2]>> =
        std::collections::HashMap::new();
    let mut order: Vec<usize> = Vec::new();
    let mut island_boxes: Vec<[i32; 4]> = Vec::new();
    let mut stretches: Vec<IslandStretch> = Vec::new();
    let mut tint: std::collections::HashMap<usize, u8> = std::collections::HashMap::new();

    for (ii, pi) in placed.iter().enumerate() {
        let off = offsets[ii];
        let rot = rotated[ii];
        let bx0 = pi.bbox[0];
        let by0 = pi.bbox[1];
        let dims = raw_dims[ii];

        let mut ibx0 = i32::MAX;
        let mut iby0 = i32::MAX;
        let mut ibx1 = i32::MIN;
        let mut iby1 = i32::MIN;

        for (face_pos, coords) in &pi.faces {
            let face_idx = models[*face_pos].face_idx;
            let mut out: Vec<[i32; 2]> = Vec::with_capacity(coords.len());
            for &[x, y] in coords {
                // normalize to island origin
                let mut nx = (x - bx0) * s;
                let mut ny = (y - by0) * s;
                if rot {
                    // 90 deg rotation: (nx,ny) -> (ny, scaledW - nx)
                    let w = dims[0] * s;
                    let new_x = ny;
                    let new_y = w - nx;
                    nx = new_x;
                    ny = new_y;
                }
                let ax = (off[0] + nx).round();
                let ay = (off[1] + ny).round();
                let u = (ax as i32).clamp(0, 255);
                let v = (ay as i32).clamp(0, 163);
                out.push([u, v]);
                ibx0 = ibx0.min(u);
                iby0 = iby0.min(v);
                ibx1 = ibx1.max(u + 1);
                iby1 = iby1.max(v + 1);
            }
            faces.insert(face_idx, out);
            order.push(face_idx);
            tint.insert(face_idx, ii as u8); // colour by island index
        }

        island_boxes.push([ibx0, iby0, ibx1, iby1]);
        stretches.push(IslandStretch {
            face_count: pi.stretch.face_count,
            max_stretch_pct: pi.stretch.max_stretch_pct,
        });
    }

    Unwrap {
        faces,
        order,
        island_boxes,
        stretches,
        tint,
        fits,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
    }

    #[test]
    fn tilted_square_preserves_edges_and_anchors() {
        // A unit square in the plane z = x (tilted 45deg about the y axis).
        // Corners (in order): (0,0,0),(1,0,1),(1,1,1),(0,1,0).
        // Edge lengths: P0->P1 = sqrt(2), P1->P2 = 1, P2->P3 = sqrt(2), P3->P0 = 1.
        let pts = [
            Point3D { x: 0, y: 0, z: 0 },
            Point3D { x: 1, y: 0, z: 1 },
            Point3D { x: 1, y: 1, z: 1 },
            Point3D { x: 0, y: 1, z: 0 },
        ];
        let f = flatten_face(&pts);
        assert_eq!(f.len(), 4);

        // P0 maps near origin.
        assert!(
            dist(f[0], [0.0, 0.0]) < 1e-6,
            "P0 not at origin: {:?}",
            f[0]
        );
        // P1 on +U axis (v-coord ~ 0, u-coord > 0).
        assert!(f[1][1].abs() < 1e-6, "P1 v-coord not zero: {:?}", f[1]);
        assert!(f[1][0] > 0.0, "P1 not on +U: {:?}", f[1]);

        // Edge lengths preserved.
        let orig = [2f64.sqrt(), 1.0, 2f64.sqrt(), 1.0];
        for i in 0..4 {
            let a = f[i];
            let b = f[(i + 1) % 4];
            assert!(
                (dist(a, b) - orig[i]).abs() < 1e-6,
                "edge {i} length {} != {}",
                dist(a, b),
                orig[i]
            );
        }
    }

    // --- Task 9: island welding helpers/tests ---

    /// Build a Geometry whose points are exactly the given coords (other fields
    /// empty); point index == position in `pts`.
    fn geom_from(pts: &[Point3D]) -> Geometry {
        Geometry {
            scales: Vec::new(),
            points: pts.to_vec(),
            edges: Vec::new(),
            faces: Vec::new(),
        }
    }

    /// Build a synthetic FaceModel from point indices (orig_uv/vert_refs filled
    /// with dummy values keyed off the indices).
    fn fm(face_idx: usize, indices: &[usize]) -> FaceModel {
        FaceModel {
            face_idx,
            point_indices: indices.to_vec(),
            orig_uv: indices.iter().map(|&i| [i as u16, 0]).collect(),
            vert_refs: indices.iter().map(|&i| (i * 24) as u16).collect(),
            recovered: false,
        }
    }

    #[test]
    fn two_coplanar_quads_sharing_edge_form_one_island() {
        // Two unit squares in the z=0 plane sharing the edge (points 1,2).
        //   p0(0,0) p1(1,0) p2(1,1) p3(0,1)  -- left quad
        //   p1(1,0) p4(2,0) p5(2,1) p2(1,1)  -- right quad (shares p1,p2)
        let pts = [
            Point3D { x: 0, y: 0, z: 0 }, // 0
            Point3D { x: 1, y: 0, z: 0 }, // 1
            Point3D { x: 1, y: 1, z: 0 }, // 2
            Point3D { x: 0, y: 1, z: 0 }, // 3
            Point3D { x: 2, y: 0, z: 0 }, // 4
            Point3D { x: 2, y: 1, z: 0 }, // 5
        ];
        let geom = geom_from(&pts);
        let models = vec![fm(0, &[0, 1, 2, 3]), fm(1, &[1, 4, 5, 2])];
        let islands = build_islands(&models, &geom, 0.0);
        assert_eq!(islands.len(), 1, "coplanar quads should weld at eps=0");
        assert_eq!(islands[0].face_pos.len(), 2);
    }

    #[test]
    fn perpendicular_third_quad_splits_or_merges_by_eps() {
        // Two coplanar quads in z=0 (faces 0,1) plus a third quad standing
        // perpendicular (in the y=0 plane, going up in z) sharing edge (1,4)
        // with the right quad.
        //   left:  p0,p1,p2,p3 (z=0)
        //   right: p1,p4,p5,p2 (z=0)
        //   perp:  p1,p4,p6,p7 (y=0, rising in z) -- shares p1,p4 with right
        let pts = [
            Point3D { x: 0, y: 0, z: 0 }, // 0
            Point3D { x: 1, y: 0, z: 0 }, // 1
            Point3D { x: 1, y: 1, z: 0 }, // 2
            Point3D { x: 0, y: 1, z: 0 }, // 3
            Point3D { x: 2, y: 0, z: 0 }, // 4
            Point3D { x: 2, y: 1, z: 0 }, // 5
            Point3D { x: 2, y: 0, z: 1 }, // 6
            Point3D { x: 1, y: 0, z: 1 }, // 7
        ];
        let geom = geom_from(&pts);
        let models = vec![
            fm(0, &[0, 1, 2, 3]),
            fm(1, &[1, 4, 5, 2]),
            fm(2, &[1, 4, 6, 7]),
        ];

        // At eps=0 the perpendicular quad stays separate -> 2 islands.
        let islands0 = build_islands(&models, &geom, 0.0);
        assert_eq!(
            islands0.len(),
            2,
            "perpendicular quad must not weld at eps=0"
        );

        // At eps=90 everything merges -> 1 island.
        let islands90 = build_islands(&models, &geom, 90.0);
        assert_eq!(islands90.len(), 1, "all faces should weld at eps=90");
        assert_eq!(islands90[0].face_pos.len(), 3);
    }

    // --- Task 10: unfold + pack helpers/tests ---

    #[test]
    fn unfold_two_coplanar_quads_shared_edge_coincides() {
        // Two unit squares in z=0 sharing edge (points 1,2). Already planar, so
        // the edge-matched unfold must reproduce the true plane: shared points
        // land at one position; each face keeps its shape.
        let pts = [
            Point3D { x: 0, y: 0, z: 0 }, // 0
            Point3D { x: 1, y: 0, z: 0 }, // 1
            Point3D { x: 1, y: 1, z: 0 }, // 2
            Point3D { x: 0, y: 1, z: 0 }, // 3
            Point3D { x: 2, y: 0, z: 0 }, // 4
            Point3D { x: 2, y: 1, z: 0 }, // 5
        ];
        let geom = geom_from(&pts);
        let models = vec![fm(0, &[0, 1, 2, 3]), fm(1, &[1, 4, 5, 2])];
        let island = Island {
            face_pos: vec![0, 1],
        };
        let placed = unfold_island(&island, &models, &geom);

        // Build point_index -> placed pos for each face, check shared coincide.
        let face0 = &placed.faces[0].1;
        let face1 = &placed.faces[1].1;
        // models[0].point_indices = [0,1,2,3]; index of point 1 is pos 1, point 2 is pos 2.
        // models[1].point_indices = [1,4,5,2]; point 1 is pos 0, point 2 is pos 3.
        let p1_f0 = face0[1];
        let p1_f1 = face1[0];
        let p2_f0 = face0[2];
        let p2_f1 = face1[3];
        assert!(dist(p1_f0, p1_f1) < 1e-9, "shared point 1 doesn't coincide");
        assert!(dist(p2_f0, p2_f1) < 1e-9, "shared point 2 doesn't coincide");

        // Each face keeps its 3D edge lengths (all unit edges here).
        for face in [face0, face1] {
            for k in 0..4 {
                let d = dist(face[k], face[(k + 1) % 4]);
                assert!((d - 1.0).abs() < 1e-9, "edge length changed: {d}");
            }
        }
        // Distortion-free island -> tiny stretch.
        assert!(placed.stretch.max_stretch_pct < 0.01);
    }

    #[test]
    fn unfold_folded_quads_flatten_gap_free() {
        // Right quad is folded up 90deg about the shared edge (points 1,2).
        // Unfolding must lay it flat in-plane with the shared edge coinciding,
        // and the unfolded distance from the seed's far edge must match the 3D
        // path (i.e. point 5 ends up 2 units along U from origin side).
        let pts = [
            Point3D { x: 0, y: 0, z: 0 }, // 0
            Point3D { x: 1, y: 0, z: 0 }, // 1
            Point3D { x: 1, y: 1, z: 0 }, // 2
            Point3D { x: 0, y: 1, z: 0 }, // 3
            // right quad folded up: shares edge p1(1,0,0)-p2(1,1,0), extends +z.
            Point3D { x: 1, y: 0, z: 1 }, // 4 (was (2,0,0) folded up)
            Point3D { x: 1, y: 1, z: 1 }, // 5 (was (2,1,0) folded up)
        ];
        let geom = geom_from(&pts);
        // right face uses points 1,4,5,2 so it shares edge 1-2 with the seed.
        let models = vec![fm(0, &[0, 1, 2, 3]), fm(1, &[1, 4, 5, 2])];
        let island = Island {
            face_pos: vec![0, 1],
        };
        let placed = unfold_island(&island, &models, &geom);
        let face0 = &placed.faces[0].1;
        let face1 = &placed.faces[1].1;

        // Shared edge coincides.
        assert!(dist(face0[1], face1[0]) < 1e-9, "shared p1 mismatch");
        assert!(dist(face0[2], face1[3]) < 1e-9, "shared p2 mismatch");

        // Each face still has unit edges (rigid copy preserved).
        for face in [face0, face1] {
            for k in 0..4 {
                let d = dist(face[k], face[(k + 1) % 4]);
                assert!((d - 1.0).abs() < 1e-9, "edge changed after unfold: {d}");
            }
        }

        // The two quads must lie on OPPOSITE sides of the shared edge after
        // unfolding (gap-free, non-overlapping): seed point 0 and right-face
        // point 4 should be separated by ~2 units across the fold line.
        let p0 = face0[0]; // seed corner away from shared edge
        let p4 = face1[1]; // right-face corner away from shared edge
        assert!(
            dist(p0, p4) > 1.9,
            "unfold collapsed instead of opening flat: dist={}",
            dist(p0, p4)
        );

        assert!(placed.stretch.max_stretch_pct < 0.01);
    }

    #[test]
    fn orient_shrinks_diagonal_rectangle_bbox() {
        // A thin rectangle 10 long x 1 wide, laid along a 45deg diagonal. Its
        // axis-aligned bbox is large (~ (11/sqrt2)^2 ≈ 60); the true rectangle
        // area is 10. After orient, the bbox area must collapse to ≈ 10.
        let s = std::f64::consts::FRAC_1_SQRT_2;
        // Corners of a 10x1 rectangle rotated 45deg.
        // local axes: along = (s, s), perp = (-s, s)
        let along = [s, s];
        let perp = [-s, s];
        let corner = |a: f64, b: f64| [a * along[0] + b * perp[0], a * along[1] + b * perp[1]];
        let pts = vec![
            corner(0.0, 0.0),
            corner(10.0, 0.0),
            corner(10.0, 1.0),
            corner(0.0, 1.0),
        ];

        // Axis-aligned bbox area before orient.
        let (mut x0, mut y0, mut x1, mut y1) = (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for &[x, y] in &pts {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        let aa_area = (x1 - x0) * (y1 - y0);

        // Build a one-face PlacedIsland and orient it.
        let mut isl = PlacedIsland {
            faces: vec![(0, pts.clone())],
            bbox: [x0, y0, x1, y1],
            stretch: IslandStretch {
                face_count: 1,
                max_stretch_pct: 0.0,
            },
        };
        orient_island(&mut isl);
        let ob = isl.bbox;
        let or_area = (ob[2] - ob[0]) * (ob[3] - ob[1]);

        assert!(
            aa_area > 3.0 * or_area,
            "axis-aligned area {aa_area} should be >> oriented {or_area}"
        );
        assert!(
            (or_area - 10.0).abs() < 0.1,
            "oriented bbox area {or_area} should ≈ true rectangle area 10"
        );
    }

    #[test]
    fn shelf_pack_fits_and_no_overlap_simple() {
        // Two equal squares; packer should place them side by side or stacked.
        let dims = [[10.0, 10.0], [10.0, 10.0]];
        let (offsets, _rot, fits) = shelf_pack(&dims, 1.0);
        assert!(fits, "two 10x10 squares should fit 256x164");
        // They must not occupy the same spot.
        assert!(offsets[0] != offsets[1], "islands overlap exactly");
    }
}
