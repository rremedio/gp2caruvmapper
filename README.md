# gp2-car-uv-mapper

A small cross-platform desktop app (Rust + egui) for **GP2 car modders** who reshape a
car's `.dat` geometry. When you move a car's vertices, the original UV table baked into
`GP2.EXE` no longer fits the new shape and the livery smears. This tool reads the reshaped
`.dat`, computes a fresh **non-distorting UV unwrap** from the new geometry, exports a
256x164 BMP painting template (a wireframe you paint your livery on top of), and patches
the SVGA UV table inside `GP2.EXE` **in place** so the game maps your texture cleanly onto
the new shape.

## How it works

The architecture is **exe-anchored**: the original `GP2.EXE` is the source of truth for
*which* `.dat` point each UV vertex belongs to and in *what order*, while the reshaped
`.dat` supplies the new positions.

1. **Read the SVGA UV table** (`dword_49DFFC`) directly out of `GP2.EXE` (decrypting it on
   the way). A face is a real **body** face when its `.dat` selector is `jam_id 530` —
   **122 faces**; the rest (damage, shadow, no-selector) are textured by other GP2 systems
   and left untouched. For each body face we recover, per vertex, its `vertRef` and the
   original `(u,v)`. **`vertRef / 24` is the global `.dat` point index**, and the vertex
   order is preserved.
2. **Read the reshaped `.dat`** for the *new* point positions only (no `.dat` edge/face
   walk is needed — just the point coordinates).
3. **Recompute `(u,v)`** as a uniform-scale rigid flatten of each face's points (the same
   shape GP2 itself uses), weld faces into islands by shared-point adjacency within a
   tunable angle tolerance, and pack the islands into the 256x164 atlas at a single global
   scale (no per-face distortion).
4. **Write a BMP** wireframe template, and **patch the exe in place**: only the `(u,v)`
   bytes change. `vertRef`, entry structure, default sharing, and every byte outside the
   UV block are preserved exactly. The file size never changes.

## Usage

1. **Open GP2.EXE** — the app verifies it's the right file and loads the UV table.
2. **Open your reshaped .dat** — the car you've been editing.
3. **Tune the weld-angle slider** — controls how aggressively coplanar adjacent faces are
   merged into a single UV island. Watch the preview update.
4. **Choose a packing strategy** (Shelf / Skyline / **MaxRects**) and the **Rotate islands**
   toggle. MaxRects + rotate (the default) packs densest — it beats GP2's own layout.
5. **Save BMP template** — writes the 256x164 paletted wireframe. Open it in your paint
   tool and draw your livery on top of the wireframe.
6. **Patch GP2.EXE** — writes the recomputed UV table back into the exe. A timestamped
   backup (`GP2.EXE.bak-<timestamp>`) is created automatically before anything is written.
   Leave **"Also install 3D geometry from this .dat"** checked (default) to write the car
   shape too — see below.

## Geometry + UVs: the two blocks

A reshaped car needs **two** things in `GP2.EXE`, in two non-overlapping regions:

| What | Region | Written by |
|---|---|---|
| 3D geometry (the `.dat`, 54,536 bytes) | `0x14C4A8` | this tool (optional) **or** a GP2 car editor |
| UV mapping (our table) | `0x4B1250` (`49DFFC`) | this tool |

GP2 pairs each UV coordinate with a 3D vertex by index, so **the geometry running in the
exe must match the `.dat` you unwrapped** — otherwise textures land on the wrong-shaped
polygons. With **"Also install 3D geometry"** on, the tool writes both blocks from the same
loaded `.dat` in one pass, so they're guaranteed consistent. Turn it off only if that exact
geometry is already installed (e.g. via a car editor) and you want to change UVs alone.

## Safety notes

- **Always backs up first.** The original file is copied to a timestamped `.bak-<...>`
  before any byte is patched.
- **Verifies exe identity** before touching it: the file must be exactly **5,702,937
  bytes** and carry the car-section magic **0x8002**. A wrong or already-modified-size exe
  is rejected.
- **Round-trip self-test before writing.** The newly encoded UV block is decrypted and
  decoded back in memory and checked against what was intended; the on-disk write only
  happens if that self-test passes.
- **Size-neutral patch.** Only `(u,v)` bytes inside the **11,476-byte** UV block change
  (plus the **54,536-byte** geometry block if you opt in). Both are fixed-length splices —
  no byte outside those regions is ever touched, and the file length is unchanged. The
  `.dat` is validated (length + magic `0x8002`) before its geometry is installed.
- **SVGA only.** This v1 targets the SVGA UV table; the VGA table is out of scope.
- **Assumes stock topology.** The reshaped car must keep the original point/face structure
  — you may *move* vertices, but not add or remove geometry.

## Build

This project uses the standard `release` profile tuned for a small static binary:
`opt-level = "z"`, `lto = true`, `strip = true`, `panic = "abort"`. No runtime or DLLs are
required (BMP is written by hand — no image codecs pulled in).

### Linux

```sh
cargo build --release
# -> target/release/gp2-car-uv-mapper
```

Measured in this environment: **5.6 MB** stripped binary.

### Windows (cross-compile from Linux)

Prerequisites:

```sh
rustup target add x86_64-pc-windows-gnu
sudo apt-get install mingw-w64        # provides x86_64-w64-mingw32-gcc + dlltool
```

The GNU linker is selected via [`.cargo/config.toml`](.cargo/config.toml):

```toml
[target.x86_64-pc-windows-gnu]
linker = "x86_64-w64-mingw32-gcc"
```

Then:

```sh
cargo build --release --target x86_64-pc-windows-gnu
# -> target/x86_64-pc-windows-gnu/release/gp2-car-uv-mapper.exe
```

**Verified:** the full eframe/glow GUI stack cross-links cleanly with mingw-w64. The
resulting `.exe` is **~3.2 MB**, needs no runtime or DLLs, and runs as a windowed app — the
console window is suppressed in release builds via `#![windows_subsystem = "windows"]` in
`src/main.rs`. If you don't have mingw-w64, you can instead build on Windows directly or use
the MSVC target (`x86_64-pc-windows-msvc`, with the MSVC build tools).

## Testing

```sh
cargo test
```

Most tests run against committed fixtures and need no external files. A few patch / exe
tests are gated behind the `GP2_EXE` environment variable and **skip cleanly when it is
unset**. To run them, point it at a real (unmodified) game exe:

```sh
GP2_EXE=/path/to/GP2.EXE cargo test
```

## Status / scope

Reverse-engineering reference: [`docs/uv-mapping.md`](docs/uv-mapping.md) — the full,
in-game-validated writeup of GP2's car UV-mapping format.

**Solved / working:**

- SVGA car texturing end-to-end: read + decrypt the `49DFFC` table, recompute a
  uniform-scale unwrap from reshaped geometry, export the BMP template, and patch the exe
  in place with full preservation of non-UV bytes.
- Validated congruence: the unwrap this tool produces matches GP2's own stored flatten —
  Procrustes residual **< 10% for 91 of 122** body faces (< 20% for 110/122; median ~5.6%).
- **In-game validated**: a reshaped car patched by this tool (geometry + UVs) renders
  correctly in GP2.

**Deferred (out of scope for v1):**

- VGA UV table (SVGA only for now).
- JAM texture packing and RCR sprites.
- Per-island (non-global) UV scale.
- `.dat`-derived vertex membership (superseded by the validated exe-anchored
  `vertRef / 24` mapping — the `.dat` edge-walk is not on the critical path).

**Known gap:** ~23 *no-selector* faces (`cmd 0x0a`/`0x00`, no `jam_id`) are textured by an
"inherit" mechanism that isn't part of the body UV table; the tool correctly leaves them
alone. They are visible cockpit/rear panels — supporting them is future work.

## Download

Prebuilt Windows binary: see [**Releases**](https://github.com/rremedio/gp2caruvmapper/releases).
Or build from source (above). You also need a copy of `GP2.EXE` (not distributed here).

## License

MIT — see [LICENSE](LICENSE). Not affiliated with or endorsed by the makers of Grand Prix 2;
"GP2"/"Grand Prix 2" are referenced for interoperability only.
