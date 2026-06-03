//! GP2-style symmetric layout.
//!
//! Instead of the dense angle-welded MaxRects unwrap, this reproduces GP2's own
//! organisation: faces are grouped into GP2's canonical islands (recovered from
//! the original `(u,v)` table via UV-weld), each island is assigned to a slice
//! (top / left / right) by its 3D geometry, and each is unwrapped by **planar
//! projection** (one plane per group, so shared vertices coincide and the group
//! stays connected with no per-face rotation). A group is split into connected
//! sub-pieces only where its projection would self-overlap or collapse a face,
//! so extreme bodywork stays clean. Every face is placed exactly once, so the
//! wireframe always contains all body faces.

use std::collections::{HashMap, HashSet};

use crate::core::dat::Geometry;
use crate::core::model::FaceModel;
use crate::core::unwrap::{IslandStretch, Unwrap};

const ATLAS_W: f64 = 256.0;
const ATLAS_H: f64 = 164.0;
const TX: f64 = 12.0; // |centroid x| below this => centerline group => top slice
const DEGEN: f64 = 0.35; // min projected/true area ratio before a face counts as collapsed
const MARGIN: f64 = 0.6; // SAT overlap margin (shared edges don't count)
const GUT: f64 = 1.0;

/// One projected piece: per-face `(model_pos, 2D coords)`.
type Placed = Vec<(usize, Vec<[f64; 2]>)>;

// ---- small vec3 helpers ----
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm3(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
fn unit3(a: [f64; 3]) -> [f64; 3] {
    let m = norm3(a);
    if m == 0.0 {
        [0.0, 0.0, 0.0]
    } else {
        [a[0] / m, a[1] / m, a[2] / m]
    }
}
/// Raw (un-halved) Newell normal; magnitude = 2 * face area.
fn newell(p: &[[f64; 3]]) -> [f64; 3] {
    let n = p.len();
    let (mut nx, mut ny, mut nz) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let a = p[i];
        let b = p[(i + 1) % n];
        nx += (a[1] - b[1]) * (a[2] + b[2]);
        ny += (a[2] - b[2]) * (a[0] + b[0]);
        nz += (a[0] - b[0]) * (a[1] + b[1]);
    }
    [nx, ny, nz]
}
fn centroid(p: &[[f64; 3]]) -> [f64; 3] {
    let n = p.len() as f64;
    let mut c = [0.0; 3];
    for q in p {
        for k in 0..3 {
            c[k] += q[k];
        }
    }
    [c[0] / n, c[1] / n, c[2] / n]
}
/// Signed-area magnitude of a 2D polygon.
fn area2(poly: &[[f64; 2]]) -> f64 {
    let n = poly.len();
    let mut s = 0.0;
    for k in 0..n {
        let a = poly[k];
        let b = poly[(k + 1) % n];
        s += a[0] * b[1] - b[0] * a[1];
    }
    s.abs() / 2.0
}

/// Convex polygons A,B overlap by more than `MARGIN` (shared edges don't count).
fn convex_overlap(a: &[[f64; 2]], b: &[[f64; 2]]) -> bool {
    for poly in [a, b] {
        let n = poly.len();
        for k in 0..n {
            let (x1, y1) = (poly[k][0], poly[k][1]);
            let (x2, y2) = (poly[(k + 1) % n][0], poly[(k + 1) % n][1]);
            let (mut ax, mut ay) = (-(y2 - y1), x2 - x1);
            let l = (ax * ax + ay * ay).sqrt();
            if l == 0.0 {
                continue;
            }
            ax /= l;
            ay /= l;
            let proj = |p: &[[f64; 2]]| {
                let mut lo = f64::INFINITY;
                let mut hi = f64::NEG_INFINITY;
                for q in p {
                    let d = ax * q[0] + ay * q[1];
                    lo = lo.min(d);
                    hi = hi.max(d);
                }
                (lo, hi)
            };
            let (amin, amax) = proj(a);
            let (bmin, bmax) = proj(b);
            if amin >= bmax - MARGIN || bmin >= amax - MARGIN {
                return false;
            }
        }
    }
    true
}

struct Uf {
    p: Vec<usize>,
}
impl Uf {
    fn new(n: usize) -> Self {
        Self {
            p: (0..n).collect(),
        }
    }
    fn find(&mut self, a: usize) -> usize {
        let mut a = a;
        while self.p[a] != a {
            self.p[a] = self.p[self.p[a]];
            a = self.p[a];
        }
        a
    }
    fn union(&mut self, a: usize, b: usize) {
        let (x, y) = (self.find(a), self.find(b));
        if x != y {
            self.p[x] = y;
        }
    }
}

/// Planar projection of a group onto its area-weighted best-fit plane, viewed
/// from outside (screen-up = car-up, screen-right = up x normal; v grows down so
/// car-up renders at the top). Returns per-face `(model_pos, coords)`.
fn project_group(grp: &[usize], pts3: &[Vec<[f64; 3]>]) -> Placed {
    let mut nsum = [0.0; 3];
    let mut csum = [0.0; 3];
    let mut cnt = 0.0;
    for &fi in grp {
        let nn = newell(&pts3[fi]);
        for k in 0..3 {
            nsum[k] += nn[k];
        }
        for p in &pts3[fi] {
            for k in 0..3 {
                csum[k] += p[k];
            }
            cnt += 1.0;
        }
    }
    let mut n = unit3(nsum);
    let c = [csum[0] / cnt, csum[1] / cnt, csum[2] / cnt];
    if dot(c, n) < 0.0 {
        n = [-n[0], -n[1], -n[2]]; // outward
    }
    let proj_axis = |v: [f64; 3]| {
        let d = dot(v, n);
        [v[0] - d * n[0], v[1] - d * n[1], v[2] - d * n[2]]
    };
    let mut su = proj_axis([0.0, 0.0, 1.0]); // car-up
    if norm3(su) < 0.3 {
        su = proj_axis([1.0, 0.0, 0.0]); // top/floor faces: width-as-up => length horizontal
    }
    let su = unit3(su);
    let sr = cross(su, n);
    grp.iter()
        .map(|&fi| {
            let coords = pts3[fi]
                .iter()
                .map(|p| [dot(*p, sr), -dot(*p, su)])
                .collect();
            (fi, coords)
        })
        .collect()
}

/// A projected group is OK if no face collapses and none overlap.
fn proj_ok(pl: &[(usize, Vec<[f64; 2]>)], pts3: &[Vec<[f64; 3]>]) -> bool {
    for (fi, poly) in pl {
        let ta = norm3(newell(&pts3[*fi])) / 2.0;
        if ta > 1.0 && area2(poly) / ta < DEGEN {
            return false;
        }
    }
    for i in 0..pl.len() {
        for j in (i + 1)..pl.len() {
            if convex_overlap(&pl[i].1, &pl[j].1) {
                return false;
            }
        }
    }
    true
}

/// Grow connected sub-groups that each project cleanly; a group that projects
/// fine whole stays one piece.
fn project_pieces(
    grp: &[usize],
    pts3: &[Vec<[f64; 3]>],
    pidx: &[HashSet<usize>],
) -> Vec<Placed> {
    let mut remaining: Vec<usize> = grp.to_vec();
    let mut pieces = Vec::new();
    while !remaining.is_empty() {
        let mut sub = vec![remaining[0]];
        let mut changed = true;
        while changed {
            changed = false;
            for &f in &remaining {
                if sub.contains(&f) {
                    continue;
                }
                let connected = sub
                    .iter()
                    .any(|&g| pidx[f].intersection(&pidx[g]).count() >= 2);
                if !connected {
                    continue;
                }
                let mut trial = sub.clone();
                trial.push(f);
                if proj_ok(&project_group(&trial, pts3), pts3) {
                    sub.push(f);
                    changed = true;
                }
            }
        }
        pieces.push(project_group(&sub, pts3));
        remaining.retain(|f| !sub.contains(f));
    }
    pieces
}

/// One placed group, normalised to its own origin.
struct Cluster {
    faces: Vec<(usize, Vec<[f64; 2]>)>, // (model_pos, local coords)
    w: f64,
    h: f64,
    cy: f64,          // mean car-length (for front-to-back ordering)
    slice: u8,        // 0 top, 1 left, 2 right
    foreshorten: f64, // worst proj/true ratio in [0,1]
}

/// Slice into which a whole island falls, by geometry.
fn classify(grp: &[usize], pts3: &[Vec<[f64; 3]>]) -> u8 {
    let mut cx = 0.0;
    let mut cnt = 0.0;
    let mut nsum = [0.0; 3];
    for &fi in grp {
        let c = centroid(&pts3[fi]);
        let mut nn = unit3(newell(&pts3[fi]));
        if dot(c, nn) < 0.0 {
            nn = [-nn[0], -nn[1], -nn[2]];
        }
        for k in 0..3 {
            nsum[k] += nn[k];
        }
        cx += c[0];
        cnt += 1.0;
    }
    cx /= cnt;
    let n = unit3(nsum);
    if n[2].abs() >= n[0].abs() && n[2].abs() >= n[1].abs() && n[2] > 0.0 {
        return 0; // up-facing -> top
    }
    if cx.abs() < TX {
        return 0; // centerline -> top
    }
    if cx < 0.0 {
        1
    } else {
        2
    }
}

/// Bounding box of a placed piece: (min, max).
fn bbox(piece: &Placed) -> ([f64; 2], [f64; 2]) {
    let mut lo = [f64::INFINITY, f64::INFINITY];
    let mut hi = [f64::NEG_INFINITY, f64::NEG_INFINITY];
    for (_, poly) in piece {
        for p in poly {
            lo[0] = lo[0].min(p[0]);
            lo[1] = lo[1].min(p[1]);
            hi[0] = hi[0].max(p[0]);
            hi[1] = hi[1].max(p[1]);
        }
    }
    (lo, hi)
}

fn map_piece(piece: Placed, f: impl Fn([f64; 2]) -> [f64; 2]) -> Placed {
    piece
        .into_iter()
        .map(|(fi, poly)| (fi, poly.iter().map(|&p| f(p)).collect()))
        .collect()
}

/// Axis-align a projected piece: de-tilt to its min-area angle, force landscape,
/// keep the up side up, normalise to origin. A lossless rigid rotation of the
/// whole piece (no UV distortion) so groups sit paintable/axis-aligned even on
/// tilted-plane bodywork (e.g. a raked nose). Returns `(piece, w, h)`.
fn orient_piece(piece: Placed) -> (Placed, f64, f64) {
    let allp: Vec<[f64; 2]> = piece.iter().flat_map(|(_, p)| p.iter().copied()).collect();
    let n = allp.len() as f64;
    let ctr0 = [
        allp.iter().map(|p| p[0]).sum::<f64>() / n,
        allp.iter().map(|p| p[1]).sum::<f64>() / n,
    ];
    let up0 = *allp
        .iter()
        .min_by(|a, b| a[1].partial_cmp(&b[1]).unwrap())
        .unwrap();
    let ang = crate::core::unwrap::min_area_rect_angle(&allp);
    let (c, s) = (ang.cos(), ang.sin());
    let rot = move |p: [f64; 2]| [p[0] * c - p[1] * s, p[0] * s + p[1] * c];
    let mut piece = map_piece(piece, rot);
    let mut um = rot(up0);
    let mut ctr = rot(ctr0);
    let (lo, hi) = bbox(&piece);
    if hi[1] - lo[1] > hi[0] - lo[0] {
        // force landscape: (x,y) -> (y,-x)
        piece = map_piece(piece, |p| [p[1], -p[0]]);
        um = [um[1], -um[0]];
        ctr = [ctr[1], -ctr[0]];
    }
    if um[1] > ctr[1] {
        // keep the up side up: (x,y) -> (-x,-y)
        piece = map_piece(piece, |p| [-p[0], -p[1]]);
    }
    let (lo, _) = bbox(&piece);
    let piece = map_piece(piece, move |p| [p[0] - lo[0], p[1] - lo[1]]);
    let (_, hi) = bbox(&piece);
    (piece, hi[0], hi[1])
}

/// Build a clustered, normalised layout unit from one (folded) GP2 island.
fn build_cluster(grp: &[usize], pts3: &[Vec<[f64; 3]>], pidx: &[HashSet<usize>]) -> Cluster {
    // project (splitting where needed); normalise each piece to origin.
    let mut prepped: Vec<(Placed, f64, f64)> = Vec::new();
    for piece in project_pieces(grp, pts3, pidx) {
        prepped.push(orient_piece(piece));
    }
    // pack pieces into a compact ~square cluster (shelf, tallest first).
    prepped.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());
    let tot: f64 = prepped.iter().map(|p| p.1 * p.2).sum();
    let maxw = prepped.iter().map(|p| p.1).fold(tot.sqrt() * 1.3, f64::max);
    let mut faces: Vec<(usize, Vec<[f64; 2]>)> = Vec::new();
    let (mut x, mut y, mut rowh) = (0.0_f64, 0.0_f64, 0.0_f64);
    for (piece, w, h) in &prepped {
        if x > 0.0 && x + w > maxw {
            x = 0.0;
            y += rowh + GUT;
            rowh = 0.0;
        }
        for (fi, poly) in piece {
            let np: Vec<[f64; 2]> = poly.iter().map(|p| [x + p[0], y + p[1]]).collect();
            faces.push((*fi, np));
        }
        x += w + GUT;
        rowh = rowh.max(*h);
    }
    let mut cw = 0.0_f64;
    let mut ch = 0.0_f64;
    for (_, poly) in &faces {
        for p in poly {
            cw = cw.max(p[0]);
            ch = ch.max(p[1]);
        }
    }
    // mean car-length (3D y) + worst foreshorten across faces.
    let mut cy = 0.0;
    let mut worst = 1.0_f64;
    for &fi in grp {
        cy += centroid(&pts3[fi])[1];
    }
    cy /= grp.len() as f64;
    for (fi, poly) in &faces {
        let ta = norm3(newell(&pts3[*fi])) / 2.0;
        if ta > 1.0 {
            worst = worst.min(area2(poly) / ta);
        }
    }
    Cluster {
        faces,
        w: cw,
        h: ch,
        cy,
        slice: classify(grp, pts3),
        foreshorten: worst,
    }
}

/// Full-width skyline pack of clusters (front-to-back by cy), unlimited height.
/// Returns scaled-space placements per cluster and the used height.
fn slice_pack(infos: &[&Cluster], maxw: f64, sc: f64) -> (Vec<[f64; 2]>, f64) {
    let mut segs: Vec<[f64; 3]> = vec![[0.0, maxw, 0.0]];
    let mut order: Vec<usize> = (0..infos.len()).collect();
    order.sort_by(|&a, &b| infos[a].cy.partial_cmp(&infos[b].cy).unwrap());
    let maxy = |segs: &[[f64; 3]], x: f64, w: f64| -> f64 {
        let mut y = 0.0_f64;
        for s in segs {
            if s[1] > x && s[0] < x + w {
                y = y.max(s[2]);
            }
        }
        y
    };
    let mut place = vec![[0.0, 0.0]; infos.len()];
    for &k in &order {
        let w = infos[k].w * sc + GUT;
        let hh = infos[k].h * sc + GUT;
        let mut best: Option<(f64, f64)> = None;
        for s in &segs {
            let x = s[0];
            if x + w > maxw + 1e-6 {
                continue;
            }
            let y = maxy(&segs, x, w);
            if best.is_none_or(|(by, _)| y < by) {
                best = Some((y, x));
            }
        }
        let (y, x) = best.unwrap_or((maxy(&segs, 0.0, w), 0.0));
        place[k] = [x, y];
        // clip segments under [x, x+w], insert raised segment, re-sort.
        let mut next: Vec<[f64; 3]> = Vec::new();
        for s in &segs {
            if s[1] <= x || s[0] >= x + w {
                next.push(*s);
            } else {
                if s[0] < x {
                    next.push([s[0], x, s[2]]);
                }
                if s[1] > x + w {
                    next.push([x + w, s[1], s[2]]);
                }
            }
        }
        next.push([x, x + w, y + hh]);
        next.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap());
        segs = next;
    }
    let used = segs.iter().map(|s| s[2]).fold(0.0, f64::max);
    (place, used)
}

/// GP2-style symmetric layout. Returns an [`Unwrap`] like [`crate::core::unwrap::unwrap`].
#[allow(clippy::needless_range_loop)] // singleton-fold needs both island indices to merge
pub fn unwrap_symmetric(models: &[FaceModel], geom: &Geometry) -> Unwrap {
    let nf = models.len();
    let pts3: Vec<Vec<[f64; 3]>> = models
        .iter()
        .map(|m| {
            m.point_indices
                .iter()
                .map(|&p| {
                    let q = geom.points[p];
                    [q.x as f64, q.y as f64, q.z as f64]
                })
                .collect()
        })
        .collect();
    let pidx: Vec<HashSet<usize>> = models
        .iter()
        .map(|m| m.point_indices.iter().copied().collect())
        .collect();
    // point_index -> orig (u,v) per face, for GP2 UV-weld.
    let ptuv: Vec<HashMap<usize, [i32; 2]>> = models
        .iter()
        .map(|m| {
            let mut h = HashMap::new();
            for (k, &pi) in m.point_indices.iter().enumerate() {
                h.entry(pi)
                    .or_insert([m.orig_uv[k][0] as i32, m.orig_uv[k][1] as i32]);
            }
            h
        })
        .collect();

    // ---- GP2 canonical islands: weld faces sharing >=2 points with matching (u,v) ----
    let mut uf = Uf::new(nf);
    for a in 0..nf {
        for b in (a + 1)..nf {
            let mut m = 0;
            for (p, uva) in &ptuv[a] {
                if let Some(uvb) = ptuv[b].get(p) {
                    if (uva[0] - uvb[0]).abs() <= 1 && (uva[1] - uvb[1]).abs() <= 1 {
                        m += 1;
                    }
                }
            }
            if m >= 2 {
                uf.union(a, b);
            }
        }
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for fi in 0..nf {
        let r = uf.find(fi);
        groups.entry(r).or_default().push(fi);
    }
    let mut islands: Vec<Vec<usize>> = groups.into_values().collect();
    islands.sort_by_key(|g| g.iter().copied().min().unwrap_or(0));

    // ---- fold single-face islands into an adjacent multi-face island ----
    let mut absorb: HashMap<usize, usize> = HashMap::new();
    for i in 0..islands.len() {
        if islands[i].len() != 1 {
            continue;
        }
        let f = islands[i][0];
        for j in 0..islands.len() {
            if j != i
                && islands[j].len() > 1
                && islands[j]
                    .iter()
                    .any(|&g| pidx[f].intersection(&pidx[g]).count() >= 2)
            {
                absorb.insert(i, j);
                break;
            }
        }
    }
    let mut folded: Vec<Vec<usize>> = Vec::new();
    let mut idx_map: HashMap<usize, usize> = HashMap::new();
    for i in 0..islands.len() {
        if absorb.contains_key(&i) {
            continue;
        }
        idx_map.insert(i, folded.len());
        folded.push(islands[i].clone());
    }
    for (&i, &j) in &absorb {
        if let Some(&dst) = idx_map.get(&j) {
            folded[dst].extend(islands[i].clone());
        }
    }

    // ---- build clusters, assign slices ----
    let clusters: Vec<Cluster> = folded
        .iter()
        .map(|grp| build_cluster(grp, &pts3, &pidx))
        .collect();
    let by_slice = |s: u8| -> Vec<&Cluster> { clusters.iter().filter(|c| c.slice == s).collect() };
    let top = by_slice(0);
    let left = by_slice(1);
    let right = by_slice(2);

    // ---- binary-search the global scale so the three slices fit the atlas ----
    let used_h = |sc: f64| -> (f64, f64, f64) {
        let ht = if top.is_empty() {
            0.0
        } else {
            slice_pack(&top, ATLAS_W, sc).1
        };
        let hl = if left.is_empty() {
            0.0
        } else {
            slice_pack(&left, ATLAS_W, sc).1
        };
        let hr = if right.is_empty() {
            0.0
        } else {
            slice_pack(&right, ATLAS_W, sc).1
        };
        (ht, hl, hr)
    };
    let mut lo = 1e-3;
    let mut hi = 50.0;
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        let (ht, hl, hr) = used_h(mid);
        if ht + hl + hr + 6.0 <= ATLAS_H {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let sc = lo;

    // ---- place every slice; assemble per-face atlas coords ----
    let mut faces: HashMap<usize, Vec<[i32; 2]>> = HashMap::new();
    let mut order: Vec<usize> = Vec::new();
    let mut island_boxes: Vec<[i32; 4]> = Vec::new();
    let mut stretches: Vec<IslandStretch> = Vec::new();

    let (pt, ht) = if top.is_empty() {
        (Vec::new(), 0.0)
    } else {
        slice_pack(&top, ATLAS_W, sc)
    };
    for (k, &cl) in top.iter().enumerate() {
        emit(
            cl,
            &pt[k],
            1.0,
            sc,
            models,
            &mut faces,
            &mut order,
            &mut island_boxes,
            &mut stretches,
        );
    }
    let ymid = ht + 2.0;
    let (pl, hl) = if left.is_empty() {
        (Vec::new(), 0.0)
    } else {
        slice_pack(&left, ATLAS_W, sc)
    };
    for (k, &cl) in left.iter().enumerate() {
        emit(
            cl,
            &pl[k],
            ymid,
            sc,
            models,
            &mut faces,
            &mut order,
            &mut island_boxes,
            &mut stretches,
        );
    }
    let ybot = ymid + hl + 2.0;
    let (pr, _hr) = if right.is_empty() {
        (Vec::new(), 0.0)
    } else {
        slice_pack(&right, ATLAS_W, sc)
    };
    for (k, &cl) in right.iter().enumerate() {
        emit(
            cl,
            &pr[k],
            ybot,
            sc,
            models,
            &mut faces,
            &mut order,
            &mut island_boxes,
            &mut stretches,
        );
    }

    // Coverage is guaranteed by construction (every model face is in exactly one
    // island -> one cluster -> placed); assert it so regressions fail loudly.
    debug_assert_eq!(faces.len(), nf, "symmetric layout dropped faces");

    Unwrap::from_parts(faces, order, island_boxes, stretches, true)
}

/// Place one cluster's faces into the atlas at `place` (scaled-space) + `y_off`.
#[allow(clippy::too_many_arguments)]
fn emit(
    cl: &Cluster,
    place: &[f64; 2],
    y_off: f64,
    sc: f64,
    models: &[FaceModel],
    faces: &mut HashMap<usize, Vec<[i32; 2]>>,
    order: &mut Vec<usize>,
    island_boxes: &mut Vec<[i32; 4]>,
    stretches: &mut Vec<IslandStretch>,
) {
    let mut bb = [i32::MAX, i32::MAX, i32::MIN, i32::MIN];
    for (fi, poly) in &cl.faces {
        let face_idx = models[*fi].face_idx;
        let out: Vec<[i32; 2]> = poly
            .iter()
            .map(|p| {
                let ax = (place[0] + p[0] * sc).round() as i32;
                let ay = (y_off + place[1] + p[1] * sc).round() as i32;
                let u = ax.clamp(0, 255);
                let v = ay.clamp(0, 163);
                bb[0] = bb[0].min(u);
                bb[1] = bb[1].min(v);
                bb[2] = bb[2].max(u + 1);
                bb[3] = bb[3].max(v + 1);
                [u, v]
            })
            .collect();
        faces.insert(face_idx, out);
        order.push(face_idx);
    }
    island_boxes.push(bb);
    stretches.push(IslandStretch {
        face_count: cl.faces.len(),
        max_stretch_pct: (1.0 - cl.foreshorten) * 100.0,
    });
}
