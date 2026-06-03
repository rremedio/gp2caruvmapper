//! Patch GP2.EXE's encrypted SVGA UV block in place.
//!
//! The block is edited byte-for-byte: only each real face's `(u,v)` is
//! overwritten in the decrypted block; everything else (vert_ref, counts,
//! offset table, pre-base structured data) is preserved. The block size and
//! file size never change. A backup of the original file is written first.

use std::path::{Path, PathBuf};

use crate::core::jam::jam_xor;
use crate::core::uvtable::{self, UvTable, SVGA_FILE_OFF, SVGA_LEN};

/// Expected size of a stock GP2.EXE.
const EXE_LEN: usize = 5_702_937;
/// File offset of the embedded car 3D block (== the bytes of a `.dat` file).
/// The car `Magic` word is the first u16 of this block.
pub const CAR_GEOM_OFF: usize = 0x14C4A8;
/// Length of the embedded car 3D block (`DEF_CAR_LENGTH`).
pub const CAR_GEOM_LEN: usize = 54_536;
/// File offset of the car `Magic` word (start of the geometry block).
const CAR_MAGIC_OFF: usize = CAR_GEOM_OFF;
/// Expected car `Magic` value (u16 LE).
const CAR_MAGIC: u16 = 0x8002;

pub struct PatchReport {
    pub faces: usize,
    pub backup_path: PathBuf,
    pub block_len: usize,
    /// True if the 3D geometry block was also written from the `.dat`.
    pub geometry_installed: bool,
}

#[derive(Debug)]
pub enum PatchError {
    Io(String),
    BadExe(String),
    RoundTrip(String),
    Patch(String),
}

/// Validate that `exe` looks like a stock GP2.EXE we can patch.
pub fn verify_exe(exe: &[u8]) -> Result<(), PatchError> {
    if exe.len() != EXE_LEN {
        return Err(PatchError::BadExe(format!(
            "size {} != expected {}",
            exe.len(),
            EXE_LEN
        )));
    }
    if CAR_MAGIC_OFF + 2 > exe.len() {
        return Err(PatchError::BadExe("magic offset out of range".into()));
    }
    let magic = u16::from_le_bytes([exe[CAR_MAGIC_OFF], exe[CAR_MAGIC_OFF + 1]]);
    if magic != CAR_MAGIC {
        return Err(PatchError::BadExe(format!(
            "car magic {magic:#06x} != expected {CAR_MAGIC:#06x}"
        )));
    }
    Ok(())
}

/// Validate that `dat` is a car 3D block we can install (right length + Magic).
pub fn verify_dat(dat: &[u8]) -> Result<(), PatchError> {
    if dat.len() != CAR_GEOM_LEN {
        return Err(PatchError::BadExe(format!(
            ".dat size {} != expected car block {CAR_GEOM_LEN}",
            dat.len()
        )));
    }
    let magic = u16::from_le_bytes([dat[0], dat[1]]);
    if magic != CAR_MAGIC {
        return Err(PatchError::BadExe(format!(
            ".dat magic {magic:#06x} != expected {CAR_MAGIC:#06x}"
        )));
    }
    Ok(())
}

/// Patch the SVGA UV block of `path` in place, writing a timestamped backup
/// first. Returns a report on success.
pub fn patch_svga(
    path: &Path,
    table: &UvTable,
    body_faces: &[usize],
    timestamp: &str,
) -> Result<PatchReport, PatchError> {
    patch_exe(path, table, None, body_faces, timestamp)
}

/// Patch GP2.EXE in one read-modify-write: always update the SVGA UV table
/// (in place); if `geom_dat` is `Some`, also install that car 3D block at
/// `CAR_GEOM_OFF` (a plain byte splice — the `.dat` is byte-identical to the
/// exe block). Both regions are non-overlapping. A timestamped backup of the
/// original file is written before the exe is overwritten.
pub fn patch_exe(
    path: &Path,
    table: &UvTable,
    geom_dat: Option<&[u8]>,
    body_faces: &[usize],
    timestamp: &str,
) -> Result<PatchReport, PatchError> {
    // 1. read + verify the exe.
    let mut exe = std::fs::read(path).map_err(|e| PatchError::Io(e.to_string()))?;
    verify_exe(&exe)?;

    // 2. optional geometry block: validate, then splice the .dat in.
    if let Some(dat) = geom_dat {
        verify_dat(dat)?;
        exe[CAR_GEOM_OFF..CAR_GEOM_OFF + CAR_GEOM_LEN].copy_from_slice(dat);
    }

    // 3. slice + decrypt the UV block.
    let mut block = exe[SVGA_FILE_OFF..SVGA_FILE_OFF + SVGA_LEN].to_vec();
    jam_xor(&mut block);

    // 4. edit (u,v) in place.
    uvtable::patch_block_uv(&mut block, table, body_faces).map_err(PatchError::Patch)?;

    // 5. re-encrypt.
    jam_xor(&mut block);

    // 6. round-trip self-test: decrypt a clone and confirm what decode reads.
    {
        let mut chk = block.clone();
        jam_xor(&mut chk);
        let decoded = uvtable::decode(&chk)
            .ok_or_else(|| PatchError::RoundTrip("decode failed".into()))?;
        for &idx in body_faces {
            let got = decoded
                .face(idx)
                .ok_or_else(|| PatchError::RoundTrip(format!("face {idx} missing")))?;
            let want = table
                .face(idx)
                .ok_or_else(|| PatchError::RoundTrip(format!("table face {idx} missing")))?;
            if got.verts != want.verts {
                return Err(PatchError::RoundTrip(format!("face {idx} mismatch")));
            }
        }
    }

    // 7. write backup (copy the original file bytes, still untouched on disk).
    let mut backup_name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    backup_name.push(format!(".bak-{timestamp}"));
    let backup_path = path.with_file_name(backup_name);
    std::fs::copy(path, &backup_path).map_err(|e| PatchError::Io(e.to_string()))?;

    // 8. splice the UV block back in and write the exe once.
    exe[SVGA_FILE_OFF..SVGA_FILE_OFF + SVGA_LEN].copy_from_slice(&block);
    std::fs::write(path, &exe).map_err(|e| PatchError::Io(e.to_string()))?;

    Ok(PatchReport {
        faces: body_faces.len(),
        backup_path,
        block_len: SVGA_LEN,
        geometry_installed: geom_dat.is_some(),
    })
}

/// Restore a backup over the target file.
pub fn restore(backup: &Path, target: &Path) -> Result<(), PatchError> {
    std::fs::copy(backup, target).map_err(|e| PatchError::Io(e.to_string()))?;
    Ok(())
}
