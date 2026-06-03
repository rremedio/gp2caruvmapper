//! GP2 car .dat parsing.
//!
//! A GP2 car `.dat` is a binary 3D model: a header with section offsets, then
//! sections for scales, "texture commands" (faces), points, and "vertices"
//! (edges). Ported faithfully from the Python reference scripts
//! (`car_parse.py` + `car_geom.py`).

/// Little-endian unsigned 16-bit read.
#[inline]
fn u16(b: &[u8], o: usize) -> u16 {
    (b[o] as u16) | ((b[o + 1] as u16) << 8)
}

/// Little-endian signed 16-bit read.
#[inline]
fn i16le(b: &[u8], o: usize) -> i16 {
    u16(b, o) as i16
}

/// Little-endian signed 32-bit read.
#[inline]
fn i32le(b: &[u8], o: usize) -> i32 {
    i32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// Header layout: `<hh` + 11×`i32` + `hhh` (all little-endian).
/// Mirrors `car_parse.parse_header`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub magic: i16,
    pub id: i16,
    pub scale_begin: i32,
    pub scale_end: i32,
    pub texture_begin: i32,
    pub points_begin: i32,
    pub vertex_begin: i32,
    pub texture_end: i32,
    pub vertex_end: i32,
    pub file_end: i32,
    pub file_end2: i32,
    pub always0: i32,
    pub always1: i32,
    pub unk: i16,
    pub size: i16,
    pub size8: i16,
}

impl Header {
    /// Total byte length of the header on disk: `2+2 + 11*4 + 2+2+2 = 54`.
    pub const SIZE: usize = 2 + 2 + 11 * 4 + 2 + 2 + 2;

    /// Parse the fixed-layout header. Returns `None` if `bytes` is too short.
    pub fn parse(bytes: &[u8]) -> Option<Header> {
        if bytes.len() < Self::SIZE {
            return None;
        }
        Some(Header {
            magic: i16le(bytes, 0),
            id: i16le(bytes, 2),
            scale_begin: i32le(bytes, 4),
            scale_end: i32le(bytes, 8),
            texture_begin: i32le(bytes, 12),
            points_begin: i32le(bytes, 16),
            vertex_begin: i32le(bytes, 20),
            texture_end: i32le(bytes, 24),
            vertex_end: i32le(bytes, 28),
            file_end: i32le(bytes, 32),
            file_end2: i32le(bytes, 36),
            always0: i32le(bytes, 40),
            always1: i32le(bytes, 44),
            unk: i16le(bytes, 48),
            size: i16le(bytes, 50),
            size8: i16le(bytes, 52),
        })
    }
}

/// A resolved 3D point (model coordinates).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point3D {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

/// An edge: two point-index endpoints. Mirrors `car_geom` edges (`vertices`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    pub from: u8,
    pub to: u8,
}

/// One parsed texture command (a "face" in the texture-command stream).
///
/// Mirrors `car_parse.parse_faces`: `numl`/`numh` are the command's position
/// bytes, `cmd` is the opcode, and `args` are its raw argument bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Face {
    pub numl: u8,
    pub numh: u8,
    pub cmd: u8,
    pub args: Vec<u8>,
}

/// Fixed argument-byte count per texture-command opcode (Car.cpp `updateData`).
/// Returns `None` for unknown opcodes (signals a wrong `start`). Mirrors the
/// `FIXED` dict in `car_parse.py`.
fn fixed_args(cmd: u8) -> Option<usize> {
    match cmd {
        0x80 | 0x90 => Some(7),
        0x13 => Some(15),
        0x18 | 0x11 | 0x1a | 0x16 | 0x17 | 0x12 | 0x15 | 0x10 | 0x00 => Some(11),
        0x0a => Some(5),
        _ => None,
    }
}

/// Opcodes whose argument list is terminated by a `00 00` pair (read pairs
/// until the terminator). Mirrors `PAIRTERM` in `car_parse.py`.
fn is_pairterm(cmd: u8) -> bool {
    matches!(
        cmd,
        0x18 | 0x11 | 0x1a | 0x16 | 0x17 | 0x12 | 0x15 | 0x10 | 0x00 | 0x0a
    )
}

/// Parse the texture-command stream into a list of [`Face`]s, exactly mirroring
/// `car_parse.parse_faces`. Requires the stream to land precisely on
/// `texture_end` (returns `None` otherwise — the signal of a wrong `start`).
fn parse_faces(bytes: &[u8], header: &Header, start: i32) -> Option<Vec<Face>> {
    let base = start - header.scale_begin;
    let count0 = (base + header.texture_begin) as i64;
    let end = (base + header.texture_end) as i64;
    if count0 < 0 || end < count0 || end > bytes.len() as i64 {
        return None;
    }
    let mut count = count0 as usize;
    let end = end as usize;
    let mut faces = Vec::new();
    while count < end {
        if count + 3 > bytes.len() {
            break;
        }
        let numl = bytes[count];
        let numh = bytes[count + 1];
        let cmd = bytes[count + 2];
        count += 3;
        let nfix = fixed_args(cmd)?; // unknown cmd -> wrong 'start'
        if count + nfix > bytes.len() {
            return None;
        }
        let mut args: Vec<u8> = bytes[count..count + nfix].to_vec();
        count += nfix;
        if cmd == 0x13 && args.len() > 3 && args[3] == 0x80 {
            if count + 2 > bytes.len() {
                return None;
            }
            args.extend_from_slice(&bytes[count..count + 2]);
            count += 2;
        }
        if is_pairterm(cmd) {
            while !(args.len() >= 2 && args[args.len() - 2] == 0 && args[args.len() - 1] == 0) {
                if count + 2 > bytes.len() {
                    return None;
                }
                args.push(bytes[count]);
                args.push(bytes[count + 1]);
                count += 2;
            }
        }
        faces.push(Face {
            numl,
            numh,
            cmd,
            args,
        });
    }
    if count != end {
        // must land exactly on texture_end
        return None;
    }
    Some(faces)
}

/// Extract the signed point references from a face's args, mirroring
/// `car_parse.ptslist`. `startArg = 5` (+4 if `args[3] == 0x80`; `= 1` for cmds
/// 0x00/0x0a); read (lo,hi) `u16` pairs as signed `i16`, skipping zeros.
fn ptslist(cmd: u8, args: &[u8]) -> Vec<i16> {
    if cmd >= 0x7F {
        return Vec::new(); // 0x80/0x90 are not faces
    }
    let mut start_arg = 5usize;
    if args.len() > 3 && args[3] == 0x80 {
        start_arg += 4;
    }
    if cmd == 0x0a || cmd == 0x00 {
        start_arg = 1;
    }
    let mut pts = Vec::new();
    let mut i = start_arg;
    while i + 1 < args.len() {
        let val = (args[i] as u16) | ((args[i + 1] as u16) << 8);
        if val != 0 {
            pts.push(val as i16);
        }
        i += 2;
    }
    pts
}

/// Resolved geometry section: scales, points, edges. Mirrors `car_geom.resolve`.
/// Also carries the parsed texture-command [`Face`]s for edge-walk recovery.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Geometry {
    pub scales: Vec<i32>,
    pub points: Vec<Point3D>,
    pub edges: Vec<Edge>,
    pub(crate) faces: Vec<Face>,
}

impl Geometry {
    /// Resolve scales/points/edges from `bytes`, using `start` as the absolute
    /// offset of the scale section. Ported exactly from `car_geom.resolve`.
    pub fn parse(bytes: &[u8], start: i32) -> Option<Geometry> {
        let header = Header::parse(bytes)?;

        let base = start - header.scale_begin;
        let nscale = (header.scale_end - header.scale_begin) / 2;
        let npoints = (header.vertex_begin - header.points_begin) / 8;
        let nedges = (header.vertex_end - header.vertex_begin) / 4;

        if nscale < 0 || npoints < 0 || nedges < 0 {
            return None;
        }

        // Guard allocations: counts come straight from header bytes, so a
        // malformed `.dat` could drive `vec!`/`with_capacity` to a multi-GB
        // (process-aborting) allocation before any per-byte bounds check below.
        // Validate each region fits the buffer using i64 so negative/overflow
        // values are rejected instead of wrapping into huge `usize`s.
        let len = bytes.len() as i64;
        let start64 = start as i64;
        let base64 = base as i64;
        // scales: read at start + i*2 for i in 0..nscale.
        if start64 + (nscale as i64) * 2 > len {
            return None;
        }
        // edges: read at base + vertex_begin + i*4 for i in 0..nedges.
        if base64 + (header.vertex_begin as i64) + (nedges as i64) * 4 > len {
            return None;
        }
        // points: read at base + points_begin + i*8 for i in 0..npoints.
        if base64 + (header.points_begin as i64) + (npoints as i64) * 8 > len {
            return None;
        }

        let nscale = nscale as usize;
        let npoints = npoints as usize;
        let nedges = nedges as usize;

        // scales: nscale signed-16 LE values read from absolute offset start + i*2.
        let mut scales = Vec::with_capacity(nscale);
        for i in 0..nscale {
            let o = (start + (i as i32) * 2) as usize;
            if o + 2 > bytes.len() {
                return None;
            }
            scales.push(i16le(bytes, o) as i32);
        }

        // edges (vertices): read at base + vertex_begin + i*4, bytes [0],[1] = from,to.
        let vbase = base + header.vertex_begin;
        let mut edges = Vec::with_capacity(nedges);
        for i in 0..nedges {
            let o = (vbase + (i as i32) * 4) as usize;
            if o + 2 > bytes.len() {
                return None;
            }
            edges.push(Edge {
                from: bytes[o],
                to: bytes[o + 1],
            });
        }

        // points: two-pass resolution with the scval helper and the 0x8000
        // back-reference, exactly as in car_geom.resolve.
        let pbase = base + header.points_begin;

        // scval(raw): closure capturing `scales`.
        let scval = |raw: u16, scales: &[i32]| -> i32 {
            let raw = raw as i32;
            if 0x80 < raw && raw < 0xFF {
                let idx = ((raw - 0x84) / 4) as usize;
                if idx < scales.len() {
                    -scales[idx]
                } else {
                    0
                }
            } else if raw > 0 {
                let idx = ((raw - 0x04) / 4) as usize;
                if idx < scales.len() {
                    scales[idx]
                } else {
                    0
                }
            } else {
                raw
            }
        };

        let mut points = vec![Point3D { x: 0, y: 0, z: 0 }; npoints];
        for _pass in 0..2 {
            for i in 0..npoints {
                let o = (pbase + (i as i32) * 8) as usize;
                if o + 6 > bytes.len() {
                    return None;
                }
                let xraw = u16(bytes, o);
                let yraw = u16(bytes, o + 2);
                let zraw = u16(bytes, o + 4);
                // z sign-extends from 16-bit.
                let z = if zraw as u32 > 0x8000 {
                    -(0x10000 - zraw as i32)
                } else {
                    zraw as i32
                };
                let (x, y);
                if (xraw as u32) < 0x8000 {
                    x = scval(xraw, &scales);
                    y = scval(yraw, &scales);
                } else {
                    let pidx = (xraw as usize) - 0x8000;
                    if pidx < points.len() {
                        x = points[pidx].x;
                        y = points[pidx].y;
                    } else {
                        x = 0;
                        y = 0;
                    }
                }
                points[i] = Point3D { x, y, z };
            }
        }

        // Parse the texture-command stream with the SAME `start`. This must
        // land exactly on texture_end (mirrors parse_faces); a wrong `start`
        // yields None here and rejects the whole geometry, matching the Python
        // reference's auto-detect (geometry + faces share one `start`).
        let faces = parse_faces(bytes, &header, start)?;

        Some(Geometry {
            scales,
            points,
            edges,
            faces,
        })
    }

    /// The parsed texture-command faces (indexed by command position).
    pub fn faces(&self) -> &[Face] {
        &self.faces
    }

    /// .dat texture selector for a face (`args[1] | args[2] << 8`).
    ///
    /// `None` for the no-selector commands (`cmd == 0x00`, `cmd == 0x0a`) or
    /// when the face has too few argument bytes. This is the "jam id" that
    /// chooses which texture a face renders from; body faces use jam id 530.
    pub fn jam_id(&self, face_idx: usize) -> Option<u16> {
        let f = self.faces.get(face_idx)?;
        if f.cmd == 0x00 || f.cmd == 0x0a {
            return None;
        }
        if f.args.len() < 3 {
            return None;
        }
        Some(f.args[1] as u16 | ((f.args[2] as u16) << 8))
    }

    /// Indices of real body faces (jam_id == 530), sorted ascending.
    ///
    /// These are the faces our tool unwraps + patches: the body selector is
    /// 530. (Faces that merely share the SVGA default UV entry but belong to
    /// other systems — e.g. damage — are NOT included, since their jam id
    /// differs.)
    pub fn body_face_indices(&self) -> Vec<usize> {
        (0..self.faces.len())
            .filter(|&i| self.jam_id(i) == Some(530))
            .collect()
    }

    /// Edge-walk the polygon for the face at command-position `face_idx`.
    ///
    /// Mirrors `car_geom.face_points`: for each signed `val` in the face's
    /// `ptslist`, `ei = |val| - 1`; the point index is `edges[ei].from` when
    /// `val > 0`, else `edges[ei].to`. Out-of-range edge/point indices are
    /// skipped (matching the Python guards). Returns `None` if there's no face
    /// at that position. The command position equals the UV table's face index.
    pub fn edge_walk(&self, face_idx: usize) -> Option<Vec<usize>> {
        let face = self.faces.get(face_idx)?;
        let mut out = Vec::new();
        for val in ptslist(face.cmd, &face.args) {
            let ei = (val.unsigned_abs() as usize).checked_sub(1)?;
            if ei < self.edges.len() {
                let pi = if val > 0 {
                    self.edges[ei].from as usize
                } else {
                    self.edges[ei].to as usize
                };
                if pi < self.points.len() {
                    out.push(pi);
                }
            }
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A malformed header whose `points_begin`/`vertex_begin` spread implies an
    /// absurd point count must be rejected (returns `None`) WITHOUT allocating
    /// gigabytes or panicking. The crafted buffer stays tiny.
    #[test]
    fn parse_rejects_bogus_counts() {
        // Build a 54-byte header buffer (exactly Header::SIZE) with i32 fields
        // set so npoints = (vertex_begin - points_begin)/8 is enormous while the
        // buffer is only 54 bytes long.
        let mut bytes = vec![0u8; Header::SIZE];
        let put_i32 = |b: &mut [u8], o: usize, v: i32| {
            b[o..o + 4].copy_from_slice(&v.to_le_bytes());
        };
        // scale_begin@4 = 0 so base = start.
        put_i32(&mut bytes, 4, 0); // scale_begin
        put_i32(&mut bytes, 8, 0); // scale_end -> nscale = 0
        put_i32(&mut bytes, 12, 0); // texture_begin
        put_i32(&mut bytes, 16, 0); // points_begin
        put_i32(&mut bytes, 20, 2_000_000_000); // vertex_begin (huge)
        put_i32(&mut bytes, 24, 0); // texture_end
        put_i32(&mut bytes, 28, 2_000_000_004); // vertex_end -> nedges = 1

        // npoints = (2_000_000_000 - 0) / 8 = 250_000_000 -> ~6 GB of Point3D.
        // The fit-check must reject this before any allocation.
        let g = Geometry::parse(&bytes, 0);
        assert!(g.is_none(), "bogus huge point count must be rejected");
    }
}
