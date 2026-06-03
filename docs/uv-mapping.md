# GP2 Car UV Mapping — Reference

Everything currently known about how **Grand Prix 2** (Geoff Crammond, 1996) maps a car's
3D faces onto its team texture, reverse-engineered for the GP2 Car UV Mapper and
**validated in-game**. Reference C++ for the geometry format is `paulhoad/gp2careditor`;
GP2's own code confirms the texture slicing.

---

## 1. Where the data lives (in `GP2.EXE`)

Addresses are IDA addresses unless noted. **File offset = IDA address + `0x63254`.**

| Data | Location | Size | Notes |
|---|---|---|---|
| Car 3D geometry (`DEF_CAR_START`) | file `0x14C4A8` | 54,536 B | **plain** bytes; identical to a `.dat` file |
| SVGA per-face UV table (`dword_49DFFC`) | IDA `0x49DFFC` → file `0x4B1250` | 11,476 B | JAM-encrypted |
| VGA / low-res UV table (`aTvGOri`) | IDA `0x4B782C` | 6,512 B | JAM-encrypted, downscaled coords |
| Team textures | `gamejams\<team>.j` | — | per-team JAM atlas |

A stock `GP2.EXE` is **5,702,937 bytes**; the car block begins with Magic `0x8002`.

---

## 2. The `.dat` 3D geometry

Header (section-offset table) → **scale → texture(=faces) → points → vertex(=edges)**.
Resolved into three indirection levels:

- **Point** `{x,y,z}` — a 3D coordinate (scale-table encoded, with `0x8000` back-references).
  Hi-nose and lo-nose variants share most data; hi-nose shifts point indices by
  `OFFSET_PTS = 194` (388 points = 2 × 194).
- **Edge** `{from, to}` — an ordered pair of point indices (1 byte each).
- **Face** — a *texture command*: `numl, numh, cmd, args…`. **187 faces (0–186)** for one car.
  A face's perimeter is an **edge loop** (`args` after the texture params are signed edge
  indices; the sign selects edge direction).

Per-face fields that matter for texturing:

- **`jam_id` = `args[1..2]`** — the texture **selector** (which texture/part this face uses):
  `530` = body (remapped to `530 + team` at runtime), `405–409` = damage, `225` = shadow, …
  **`cmd 0x00` and `cmd 0x0a` have no selector** (`jam_id` is absent — those bytes are point data).
- **`numl`** — the face's index **into the UV table** (`faceIndex` in the draw code). Usually
  equal to the face's position, but **some cars remap it** to reassign which UV slot a face uses
  (29 of 99 surveyed shapes do this — it's an editable "texture id"). The hi/lo-nose cars share
  these ids, which is why ids can exceed 186 in remapped cars.

---

## 3. JAM textures (the team atlas)

A car JAM is an **8-bit paletted atlas, 256 px wide** (body image = 256×164).

- **Encryption** (also used for the UV tables): symmetric streaming XOR. Key
  `0xB082F165` (`= 0xB082F164 | 1`); each dword is XORed, then `key *= 5`; trailing bytes use
  the low bytes of the key. Applying it twice is the identity (encrypt == decrypt).
- **Layout:** `{u16 num_items; u16 image_total_size}` → `num_items` × 32-byte entries
  (each a sub-image with `x,y,w,h,jam_id,…`) → 4 shading sub-palettes per entry → the image.
- **Pixel → colour:** `pixel → sub-palette → global 256-colour GP2 palette → RGB`
  (palette index 0 = `{151,171,127}`, the green "transparent"/unused marker).
- A **team body JAM has one sub-image** (`jam_id = 530 + team`, 256×164). The non-body parts
  (damage, shadow, …) live in **other** JAMs (`DAMAGE.JAM` = `405–409`, `CSHAD1.JAM` = `225`, …).

---

## 4. The SVGA per-face UV table (`dword_49DFFC`) — the core mechanism

JAM-decrypt the block, then:

```
base   = u32_le[0]                      (= 0x1f50)
off    = u16_le[base + faceIdx*2]       faceIdx = the face's numl
entry  = base + off
count  = u16_le[entry]                  ( = numVerts * 4 )
per vertex (6 bytes): u16 u, u16 v, u16 vertRef
```

Validated against `GP2.EXE` + the stock car:

- **`vertRef / 24` is the global `.dat` point index** (every real vertex resolves in `0..387`).
- **`(u,v)` are direct 1:1 texels** into the 256×164 atlas (origin top-left).
- **`(u,v)` is a uniform-scale rigid flatten of the `vertRef` points** — i.e. the texture
  rectangle is the panel's own shape, scaled by one global factor (≈ 4.3×). Procrustes residual
  of GP2's stored `(u,v)` vs the flattened geometry is **< 10 % for 91 / 122 faces** (< 20 % for
  110/122; the rest is integer-texel rounding on small panels). GP2 then maps that texel rectangle
  onto the projected polygon with classic **affine per-scanline** texturing.

The earlier project notes' "pixel-perfect SOLVED" only checked the **texture-space** `(u,v)`
layout; this `vertRef ↔ geometry` pairing is what makes the table usable to *generate* a mapping.

---

## 5. Which faces are real body faces

> **Rule: a face is a real body face iff its `.dat` `jam_id == 530`.** → **122 faces** (in-game validated).

- 120 of them have their own UV entry; **face 3** shares the "default" entry (below). All 122
  are `jam_id 530`; nothing else covered by the body table is.
- A naïve "exclude faces that share the most-common UV offset" heuristic is **wrong** — it drops
  face 3 (a genuine body panel). Use the `jam_id` rule.

### The "default" entry and the face 2 / face 3 mirror

**58 faces** point their offset-table slot at one shared entry (offset `0x166`: `(u,v)` =
`(65–124, 144–163)`, a 59×19 bottom-centre rect; `vertRef = {30,31,32,33}`). It looks like a
placeholder, but it is **face 3's real entry**:

- The car is symmetric. **Face 2** = the left side panel (points `{11,12,13,14}`); **face 3** =
  its mirror, the right side panel (points `{30,31,32,33}`). The default entry's `vertRef` is
  exactly `{30,31,32,33}` = face 3's geometry.
- That bottom-centre texel rect contains the **hand-painted mirrored right-panel livery** (face
  2's left-panel region, flipped). So **GP2 does not runtime-mirror** — the artist painted both
  panels into the atlas, and face 3 samples its own.
- The other **57** faces sharing offset `0x166` are non-530 *freeloaders* (damage, no-selector)
  that point there as an unused default but are textured by their own systems.

---

## 6. Texture sources for non-body faces

| Faces | `jam_id` | Source | Visible? |
|---|---|---|---|
| Body (122, incl. face 3) | `530` | team JAM + this UV table | yes — the paintable livery |
| Damage parts (~36) | `405–409` | `DAMAGE.JAM` | only in accidents |
| Shadow (face 186) | `225` | `CSHAD1.JAM` | under-car shadow |
| **No-selector (~23)** | **none** (`cmd 0x0a`/`0x00`) | **inherit mechanism — UNKNOWN** | yes (cockpit + rear clusters, incl. face 177) |

The **no-selector faces** are the one open problem: `cmd 0x0a`/`0x00` carry no `jam_id` and have
no entry in the body table, yet they render textured in-game (e.g. face 177). They cluster in the
cockpit and rear. How they obtain a UV (likely an "inherit the current texture" rasterizer state,
not a stored table) is **still to be reverse-engineered**.

---

## 7. Patching a new mapping into the exe

- **UV table — edit in place.** Decrypt the 11,476-byte block, overwrite *only* the `(u,v)` bytes
  of each body face's vertices (`base + off + 2 + k*6`), keep `vertRef`/counts/offset-table, then
  re-encrypt. **Never rebuild the block from scratch:** bytes `[4 .. 0x1f50]` (~8 KB, ~59 % nonzero)
  *before* the table base are real data GP2 uses — zeroing them corrupts the exe. The patch never
  changes the block size.
- **Geometry — plain splice.** The `.dat` is byte-identical to `0x14C4A8`, so installing a reshaped
  car is a direct 54,536-byte copy (no encryption). Doing both the geometry and the UV table from
  the *same* `.dat` guarantees the rendered shape and the UVs match.
- **Safety:** verify exe size `5,702,937` + Magic `0x8002`; both regions are non-overlapping and
  fixed-length; always back up first; round-trip self-test (decrypt+decode) before writing.

---

## 8. Status

| Item | Status |
|---|---|
| JAM decryption + format | solved |
| `.dat` geometry format | solved (binary-accurate vs editor) |
| SVGA UV table format + `vertRef↔point` + `(u,v)`=flatten | solved, in-game validated |
| Real-body-face rule (`jam_id 530`) incl. face 3 mirror | solved, **in-game validated** |
| In-place patch + geometry install | solved, in-game validated |
| Damage / shadow sources | identified (separate JAMs) |
| **No-selector (`0x0a`/`0x00`) face UV** | **OPEN** (~23 faces) |
| VGA table (`aTvGOri`) coordinate scaling | not pursued |
