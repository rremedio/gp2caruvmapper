#!/usr/bin/env python3
"""Parse tests/fixtures/Pallette.h -> src/core/palette.rs (PALETTE: [[u8;3];256])."""
import re, pathlib
src = pathlib.Path("tests/fixtures/Pallette.h").read_text()
src = re.sub(r"/\*.*?\*/", "", src, flags=re.S)  # drop /* */ block comments (alt palette)
rows = []
for line in src.splitlines():
    code = line.split("//")[0]
    m = re.search(r"\{\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*\}", code)
    if m and "myRGB" not in code:
        rows.append(tuple(int(x) for x in m.groups()))
assert len(rows) == 256, f"expected 256 colours, got {len(rows)}"
out = ["//! GP2 global 256-colour palette. GENERATED from tests/fixtures/Pallette.h.",
       "//! Regenerate with: python3 tools/gen_palette.py",
       "pub const PALETTE: [[u8; 3]; 256] = ["]
for (r, g, b) in rows:
    out.append(f"    [{r}, {g}, {b}],")
out.append("];")
pathlib.Path("src/core/palette.rs").write_text("\n".join(out) + "\n")
print(f"wrote {len(rows)} colours")
