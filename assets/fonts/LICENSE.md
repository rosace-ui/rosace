# Vendored fonts

## DejaVuSans.ttf
- **Family:** DejaVu Sans
- **License:** Bitstream Vera Fonts License + DejaVu changes (permissive; free
  to use, embed, and redistribute; may not be sold by itself).
  See https://dejavu-fonts.github.io/License.html
- **Source:** https://github.com/dejavu-fonts/dejavu-fonts
- **Use in TEZZERA:** compiled into the binary via `FontCache::embedded()` as
  the fallback font when no system font is available (always the case on the
  web/wasm target). Swappable — replace this file and the `include_bytes!`
  path in `tezzera-render/src/font.rs`.
