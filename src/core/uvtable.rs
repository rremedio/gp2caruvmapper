//! Decode GP2's SVGA per-face UV table (`dword_49DFFC`).
//!
//! Format: `base = u32_le[0]`; an offset table of `u16` per face lives at `base`;
//! `nfaces = min_nonzero_offset / 2`. For each face, `off = u16[base + face*2]`;
//! if `off == 0` the face is absent; otherwise the entry at `base + off` is
//! `[u16 count][ (u16 u, u16 v, u16 vert_ref) * (count/4) ]`. Most faces share a
//! single "default" entry (the offset value used by the most faces).

use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UvVert { pub u: u16, pub v: u16, pub vert_ref: u16 }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FaceUv { pub verts: Vec<UvVert> }

#[derive(Clone, Debug)]
pub struct UvTable {
    pub base: usize,
    faces: BTreeMap<usize, FaceUv>,
    offsets: BTreeMap<usize, u16>, // faceIdx -> raw offset (to detect the shared default)
    default_offset: u16,
}

fn u16le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// Decode a decrypted SVGA block into a [`UvTable`].
pub fn decode(dec: &[u8]) -> Option<UvTable> {
    if dec.len() < 4 { return None; }
    let base = u32le(dec, 0) as usize;

    // Find nfaces: scan the offset table tracking the minimum nonzero offset;
    // the table ends where the first (lowest-offset) entry begins.
    let mut min_nonzero: usize = usize::MAX;
    let mut i = 0usize;
    loop {
        let o = base + i * 2;
        if o + 2 > dec.len() { return None; }
        // Stop once we've reached the start of the lowest entry.
        if i * 2 >= min_nonzero { break; }
        let off = u16le(dec, o) as usize;
        if off != 0 && off < min_nonzero {
            min_nonzero = off;
        }
        i += 1;
    }
    if min_nonzero == usize::MAX || !min_nonzero.is_multiple_of(2) { return None; }
    let nfaces = min_nonzero / 2;

    let mut faces: BTreeMap<usize, FaceUv> = BTreeMap::new();
    let mut offsets: BTreeMap<usize, u16> = BTreeMap::new();

    for f in 0..nfaces {
        let off = u16le(dec, base + f * 2);
        if off == 0 { continue; }
        let ent = base + off as usize;
        if ent + 2 > dec.len() { return None; }
        let count = u16le(dec, ent) as usize;
        let nv = count / 4;
        let need = ent + 2 + nv * 6;
        if need > dec.len() { return None; }
        let mut verts = Vec::with_capacity(nv);
        for v in 0..nv {
            let p = ent + 2 + v * 6;
            verts.push(UvVert {
                u: u16le(dec, p),
                v: u16le(dec, p + 2),
                vert_ref: u16le(dec, p + 4),
            });
        }
        faces.insert(f, FaceUv { verts });
        offsets.insert(f, off);
    }

    // The default offset is the offset value shared by the most faces.
    let mut counts: BTreeMap<u16, usize> = BTreeMap::new();
    for &off in offsets.values() {
        *counts.entry(off).or_insert(0) += 1;
    }
    let (mut default_offset, mut max_count) = (0u16, 0usize);
    for (&off, &c) in &counts {
        if c > max_count {
            max_count = c;
            default_offset = off;
        }
    }
    if max_count <= 1 {
        default_offset = 0;
    }

    Some(UvTable { base, faces, offsets, default_offset })
}

pub const SVGA_FILE_OFF: usize = 0x49DFFC + 0x63254; // 0x4B1250
pub const SVGA_LEN: usize = 11476;

/// Slice the SVGA block out of GP2.EXE, decrypt it with JAM, and decode it.
pub fn read_svga_from_exe(exe: &[u8]) -> Option<UvTable> {
    let end = SVGA_FILE_OFF + SVGA_LEN;
    if exe.len() < end { return None; }
    let mut block = exe[SVGA_FILE_OFF..end].to_vec();
    crate::core::jam::jam_xor(&mut block); // decrypt
    decode(&block)
}

/// Build a patched table: for each REAL face, replace each vertex's `(u,v)`
/// with the unwrap's atlas coords AND its `vert_ref` with the face model's
/// `vert_refs` (the edge-walk value for recovered faces, the original value
/// otherwise — a no-op for non-recovered faces). Order/count preserved (counts
/// must match). Default faces and overall structure are left untouched.
pub fn patched_table(
    orig: &UvTable,
    models: &[crate::core::model::FaceModel],
    uw: &crate::core::unwrap::Unwrap,
) -> UvTable {
    let mut out = orig.clone();
    for m in models {
        let idx = m.face_idx;
        let coords = match uw.coords(idx) {
            Some(c) => c,
            None => continue, // no unwrap coords: keep original uv
        };
        if let Some(fu) = out.faces.get_mut(&idx) {
            if coords.len() != fu.verts.len() {
                continue; // count mismatch: keep original uv
            }
            for (k, (vert, c)) in fu.verts.iter_mut().zip(coords.iter()).enumerate() {
                vert.u = c[0] as u16;
                vert.v = c[1] as u16;
                if let Some(&new_ref) = m.vert_refs.get(k) {
                    vert.vert_ref = new_ref;
                }
            }
        }
    }
    out
}

/// Overwrite each real face's vertex `(u, v, vert_ref)` in a DECRYPTED block,
/// in place. Preserves all other bytes (counts, offset table, pre-base data).
/// `table` must have been derived from this same block (same layout/offsets).
///
/// For non-recovered faces the `vert_ref` written equals the original, so it's
/// a safe no-op; for recovered faces it installs the edge-walk reference. The
/// vertex count is unchanged (an in-place patch).
pub fn patch_block_uv(
    block: &mut [u8],
    table: &UvTable,
    indices: &[usize],
) -> Result<(), String> {
    let base = table.base;
    for &idx in indices {
        if base + idx * 2 + 2 > block.len() {
            return Err(format!("face {idx}: offset table OOB"));
        }
        let off = u16le(block, base + idx * 2) as usize; // same read decode used
        let ent = base + off;
        if ent + 2 > block.len() {
            return Err(format!("face {idx}: entry OOB"));
        }
        let count = u16le(block, ent) as usize;
        let nv = count / 4;
        let f = table.face(idx).ok_or("missing face")?;
        if f.verts.len() != nv {
            return Err(format!("face {idx} vcount {} != {}", f.verts.len(), nv));
        }
        for (k, v) in f.verts.iter().enumerate() {
            let pos = ent + 2 + k * 6; // u@pos, v@pos+2, vert_ref@pos+4
            if pos + 6 > block.len() {
                return Err("oob".into());
            }
            block[pos..pos + 2].copy_from_slice(&v.u.to_le_bytes());
            block[pos + 2..pos + 4].copy_from_slice(&v.v.to_le_bytes());
            block[pos + 4..pos + 6].copy_from_slice(&v.vert_ref.to_le_bytes());
        }
    }
    Ok(())
}

/// Encode a [`UvTable`] back into a decrypted block, byte-compatible with
/// [`decode`]. Preserves default-entry sharing (one shared entry for all
/// `is_default` faces) and round-trips exactly for real faces.
///
/// NOTE: diagnostic/round-trip helper only — NOT used to patch the exe (the
/// patcher edits (u,v) in place via patch_block_uv to preserve the pre-base
/// data region). The <=11476 length check in tests is informational.
pub fn encode(table: &UvTable) -> Vec<u8> {
    // nfaces = max present face idx + 1.
    let max_idx = table.faces.keys().copied().max().unwrap_or(0);
    let nfaces = max_idx + 1;

    // decode reads the offset table AT byte `base` and entries at `base + off`,
    // inferring `nfaces = min_nonzero_off / 2`. So the offset table must sit at
    // `base`, entries directly after it, and the first entry's off == 2*nfaces.
    // Place the offset table immediately after the leading u32 -> base = 4.
    let table_bytes = 2 * nfaces;
    let base = 4;

    // Encode a single entry's bytes: [u16 count=nv*4][(u16 u,u16 v,u16 ref)*nv].
    let entry_bytes = |fu: &FaceUv| -> Vec<u8> {
        let nv = fu.verts.len();
        let mut e = Vec::with_capacity(2 + nv * 6);
        e.extend_from_slice(&((nv * 4) as u16).to_le_bytes());
        for v in &fu.verts {
            e.extend_from_slice(&v.u.to_le_bytes());
            e.extend_from_slice(&v.v.to_le_bytes());
            e.extend_from_slice(&v.vert_ref.to_le_bytes());
        }
        e
    };

    // Build the entry blob and remember each face's offset (relative to base).
    // Default faces all share ONE entry. Entries begin at off == table_bytes.
    let mut blob: Vec<u8> = Vec::new();
    let mut offs: Vec<u16> = vec![0u16; nfaces]; // 0 = absent
    let mut default_off: Option<u16> = None;

    // Emit the shared default entry first (if any default face exists).
    let has_default = table.default_offset != 0
        && (0..nfaces).any(|f| table.faces.contains_key(&f) && table.is_default(f));
    if has_default {
        // Use the original default face's FaceUv (any default face shares it).
        if let Some(fu) = (0..nfaces)
            .find(|&f| table.faces.contains_key(&f) && table.is_default(f))
            .and_then(|f| table.faces.get(&f))
        {
            let off = (table_bytes + blob.len()) as u16;
            blob.extend_from_slice(&entry_bytes(fu));
            default_off = Some(off);
        }
    }

    for (f, slot) in offs.iter_mut().enumerate() {
        let fu = match table.faces.get(&f) {
            Some(fu) => fu,
            None => continue, // absent -> offset stays 0
        };
        if table.is_default(f) {
            if let Some(off) = default_off {
                *slot = off;
                continue;
            }
        }
        // Real face (or default with no shared entry): its own entry.
        let off = (table_bytes + blob.len()) as u16;
        blob.extend_from_slice(&entry_bytes(fu));
        *slot = off;
    }

    // Assemble: [u32 base][offset table][entries...].
    let mut out = Vec::with_capacity(base + table_bytes + blob.len());
    out.extend_from_slice(&(base as u32).to_le_bytes());
    for &o in &offs {
        out.extend_from_slice(&o.to_le_bytes());
    }
    debug_assert_eq!(out.len(), base + table_bytes);
    out.extend_from_slice(&blob);
    out
}

impl UvTable {
    /// Number of faces with an entry present in the table.
    pub fn faces_present(&self) -> usize {
        self.faces.len()
    }

    /// The UV entry for a face, if present.
    pub fn face(&self, idx: usize) -> Option<&FaceUv> {
        self.faces.get(&idx)
    }

    /// Indices of "real" faces: those whose offset differs from the shared default.
    pub fn real_face_indices(&self) -> Vec<usize> {
        self.offsets
            .iter()
            .filter(|(_, &off)| off != self.default_offset)
            .map(|(&idx, _)| idx)
            .collect()
    }

    /// Whether a face uses the shared default entry.
    pub fn is_default(&self, idx: usize) -> bool {
        self.offsets.get(&idx) == Some(&self.default_offset)
    }

    /// The offset value shared by the most faces (0 if none is shared).
    pub fn default_offset(&self) -> u16 {
        self.default_offset
    }

    /// Iterate over the real (non-default) faces.
    pub fn iter_real(&self) -> impl Iterator<Item = (usize, &FaceUv)> {
        self.offsets
            .iter()
            .filter(move |(_, &off)| off != self.default_offset)
            .filter_map(move |(&idx, _)| self.faces.get(&idx).map(|f| (idx, f)))
    }
}
