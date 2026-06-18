# Changelog

All notable changes to `dinamika-cpu` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

#### Geometry & color

- `Point`, `Rect` and affine `Transform` primitives (translation, scale,
  rotation, inversion).
- Straight and premultiplied RGBA color types (`Color`, `ColorU8`,
  `PremultipliedColor`, `PremultipliedColorU8`) with conversions between them.

#### Paths

- Vector contours and a `PathBuilder` supporting straight, quadratic and cubic
  Bézier segments, plus ready-made shapes (rectangle, rounded rectangle,
  ellipse, circle).
- Curve flattening into polylines, memoized per `Transform` to avoid redundant
  work.

#### Stroking

- Path stroking with `Butt`/`Round`/`Square` caps and `Miter`/`Round`/`Bevel`
  joins, built from convex stamps.
- Dash pattern support.

#### Rasterization & masking

- Anti-aliased cover/area rasterizer (AGG-style accumulation).
- `Mask` for clipping (`overflow: hidden`).

#### Paint

- `Shader` and `Paint`, with solid-color fills.
- Porter–Duff blend modes (`SourceOver`, `Xor`, `Plus`, …) and non-separable
  W3C modes (`Multiply`, `Screen`, `Overlay`, `Darken`, `Lighten`,
  `HardLight`).
- Linear and radial gradients with `Pad`/`Repeat`/`Reflect` spread modes.
- Conic (sweep) gradient.
- Texture `Pattern` shader with selectable `FilterQuality`.
- Gradient color ramps are baked into a lookup table for faster sampling
  (performance).

#### Pixmap

- `Pixmap`: a premultiplied-RGBA pixel buffer.
- Rasterize and fill paths (`NonZero` and `EvenOdd` rules, anti-aliased).
- Stroke paths directly onto a pixmap.
- Composite one pixmap onto another with `draw_pixmap`.
- Decode PNG images (`Pixmap::decode_png`).
- Encode PNG images.

#### Text

- `Font` loading via `ttf-parser` and extraction of glyph outlines into a
  fillable `Path` (`Font::text_path`).
- String layout and direct drawing with `Pixmap::fill_text` (horizontal
  advance widths and `\n` line breaks).
- Glyph outlines cached by glyph id (performance).

### Documentation

- Documented the architecture, known limitations and usage in the crate-level
  docs and `README.md`.

### Known limitations

- Color is computed in sRGB rather than linear light: blending and gradient
  interpolation operate on gamma components directly.
- Stroke width is isotropic — non-uniform scaling or shearing yields a circular
  rather than elliptical stroke.
- Thin AA seams are possible at stroke stamp junctions (segment ↔ join).
- Text layout is minimal: glyphs are placed by horizontal advance only, with no
  kerning, shaping or bidi.

[Unreleased]: https://github.com/aviancode/dinamika
