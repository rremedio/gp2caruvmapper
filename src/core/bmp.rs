//! 8-bit paletted Windows BMP writer for the unwrap wireframe template.
//!
//! Renders the unwrap's per-face polygons as Bresenham outlines on a 256x164
//! top-origin canvas (index 0 = green background), optionally labels each face
//! with its decimal index using a tiny inline 3x5 digit font, and encodes the
//! result as a standard 8bpp BMP (bottom-up rows, 256-colour palette).

use crate::core::palette::PALETTE;
use crate::core::unwrap::Unwrap;

const W: usize = 256;
const H: usize = 164;

pub struct Opts {
    pub labels: bool,
    pub wire_idx: u8,
    pub label_idx: u8,
}

impl Default for Opts {
    fn default() -> Self {
        Self {
            labels: true,
            wire_idx: 1, // dark
            label_idx: 1,
        }
    }
}

/// 3x5 bitmap font for digits 0-9. Each digit is 5 rows of 3 bits (MSB = left).
const FONT: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111], // 0
    [0b010, 0b110, 0b010, 0b010, 0b111], // 1
    [0b111, 0b001, 0b111, 0b100, 0b111], // 2
    [0b111, 0b001, 0b111, 0b001, 0b111], // 3
    [0b101, 0b101, 0b111, 0b001, 0b001], // 4
    [0b111, 0b100, 0b111, 0b001, 0b111], // 5
    [0b111, 0b100, 0b111, 0b101, 0b111], // 6
    [0b111, 0b001, 0b010, 0b010, 0b010], // 7
    [0b111, 0b101, 0b111, 0b101, 0b111], // 8
    [0b111, 0b101, 0b111, 0b001, 0b111], // 9
];

#[inline]
fn put(buf: &mut [u8], x: i32, y: i32, idx: u8) {
    if x >= 0 && y >= 0 && (x as usize) < W && (y as usize) < H {
        buf[y as usize * W + x as usize] = idx;
    }
}

/// Integer Bresenham line.
fn line(buf: &mut [u8], mut x0: i32, mut y0: i32, x1: i32, y1: i32, idx: u8) {
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        put(buf, x0, y0, idx);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

/// Render a digit's 3x5 glyph with top-left at (x, y).
fn glyph(buf: &mut [u8], d: u8, x: i32, y: i32, idx: u8) {
    let rows = &FONT[d as usize];
    for (ry, &row) in rows.iter().enumerate() {
        for rx in 0..3i32 {
            if row & (0b100 >> rx) != 0 {
                put(buf, x + rx, y + ry as i32, idx);
            }
        }
    }
}

/// Render a decimal number left-to-right starting at (x, y).
fn number(buf: &mut [u8], mut n: usize, x: i32, y: i32, idx: u8) {
    let mut digits = Vec::new();
    if n == 0 {
        digits.push(0u8);
    } else {
        while n > 0 {
            digits.push((n % 10) as u8);
            n /= 10;
        }
        digits.reverse();
    }
    let mut cx = x;
    for d in digits {
        glyph(buf, d, cx, y, idx);
        cx += 4; // 3px glyph + 1px spacing
    }
}

/// Render the unwrap as an 8bpp paletted BMP, returned as raw file bytes.
pub fn write_template(uw: &Unwrap, opts: &Opts) -> Vec<u8> {
    let mut buf = vec![0u8; W * H];

    for (idx, coords) in uw.iter_faces() {
        let n = coords.len();
        if n == 0 {
            continue;
        }
        // Closed outline.
        for i in 0..n {
            let a = coords[i];
            let b = coords[(i + 1) % n];
            line(&mut buf, a[0], a[1], b[0], b[1], opts.wire_idx);
        }
        if opts.labels {
            // Centroid (integer average).
            let mut sx = 0i64;
            let mut sy = 0i64;
            for c in coords {
                sx += c[0] as i64;
                sy += c[1] as i64;
            }
            let cx = (sx / n as i64) as i32;
            let cy = (sy / n as i64) as i32;
            // Centre the label roughly on the centroid.
            let ndig = if idx == 0 { 1 } else { (idx as f64).log10() as i32 + 1 };
            let label_w = ndig * 4 - 1;
            number(&mut buf, idx, cx - label_w / 2, cy - 2, opts.label_idx);
        }
    }

    encode_bmp(&buf)
}

/// Encode a top-origin 256x164 index buffer into an 8bpp BMP file.
fn encode_bmp(buf: &[u8]) -> Vec<u8> {
    let row_stride = (W + 3) & !3; // pad each row to a multiple of 4 bytes
    let pixel_data_off = 14 + 40 + 1024;
    let image_size = row_stride * H;
    let file_size = pixel_data_off + image_size;

    let mut out = Vec::with_capacity(file_size);

    // BITMAPFILEHEADER (14 bytes)
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(file_size as u32).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved1
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved2
    out.extend_from_slice(&(pixel_data_off as u32).to_le_bytes());

    // BITMAPINFOHEADER (40 bytes)
    out.extend_from_slice(&40u32.to_le_bytes()); // header size
    out.extend_from_slice(&(W as i32).to_le_bytes()); // width
    out.extend_from_slice(&(H as i32).to_le_bytes()); // height (positive = bottom-up)
    out.extend_from_slice(&1u16.to_le_bytes()); // planes
    out.extend_from_slice(&8u16.to_le_bytes()); // bitcount
    out.extend_from_slice(&0u32.to_le_bytes()); // compression (BI_RGB)
    out.extend_from_slice(&0u32.to_le_bytes()); // sizeimage
    out.extend_from_slice(&0i32.to_le_bytes()); // xppm
    out.extend_from_slice(&0i32.to_le_bytes()); // yppm
    out.extend_from_slice(&256u32.to_le_bytes()); // clrused
    out.extend_from_slice(&0u32.to_le_bytes()); // clrimportant

    // Palette: 256 entries, B, G, R, 0.
    for rgb in PALETTE.iter() {
        out.push(rgb[2]); // B
        out.push(rgb[1]); // G
        out.push(rgb[0]); // R
        out.push(0);
    }

    // Pixel rows, bottom-up: write top-origin row v from 163 down to 0.
    for y in (0..H).rev() {
        let start = y * W;
        out.extend_from_slice(&buf[start..start + W]);
        out.resize(out.len() + (row_stride - W), 0);
    }

    out
}
