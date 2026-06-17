# dinamika-cpu

A raster 2D renderer on the CPU.
The only dependencies are [`png`](https://docs.rs/png) (saving frames) and
[`ttf-parser`](https://docs.rs/ttf-parser) (reading glyph outlines for text).

## Features

- **`Pixmap`** — a pixel buffer in premultiplied RGBA format.
- **`Path` / `PathBuilder`** — vector contours with straight, quadratic and
  cubic Bézier curves; ready-made shapes (rectangle, ellipse, circle).
- **`Paint` / `Shader`** — solid color, linear and radial gradients
  (with `Pad`/`Repeat`/`Reflect` modes).
- **`BlendMode`** — Porter–Duff modes (`SourceOver`, `Xor`, `Plus`, …) and
  non-separable ones (`Multiply`, `Screen`, `Overlay`, `Darken`, `Lighten`,
  `HardLight`).
- **Fill** by `NonZero` and `EvenOdd` rules with anti-aliasing.
- **Stroke** (`Stroke`) with `Butt`/`Round`/`Square` caps and
  `Miter`/`Round`/`Bevel` joins.
- **`Transform`** — affine transformations (translation, scale, rotation, inversion).
- **Text** (`Font`) — TrueType/OpenType glyph outlines via `ttf-parser`, turned
  into a fillable `Path` (`Font::text_path`) or drawn directly with
  `Pixmap::fill_text`. Minimal horizontal layout: advance widths and `\n` line
  breaks, no kerning or shaping.

## Architecture

| Module      | Purpose                                                           |
|-------------|-------------------------------------------------------------------|
| `geom`      | `Point`, `Rect`, `Transform`                                      |
| `color`     | non-premultiplied and premultiplied colors, conversions          |
| `path`      | contours and flattening of curves into polylines                 |
| `paint`     | shaders, gradients, blending                                     |
| `stroke`    | building strokes from convex "stamps"                            |
| `raster`    | anti-aliased rasterizer (cover/area accumulation, as in AGG)     |
| `pixmap`    | pixel buffer, fill/stroke, blending                              |
| `text`      | font loading (`ttf-parser`), glyph outlines and string layout    |

## Example

```rust
use dinamika_cpu::*;

let mut pixmap = Pixmap::new(200, 200).unwrap();
pixmap.fill(Color::WHITE);

let path = PathBuilder::from_circle(100.0, 100.0, 80.0).unwrap();
let paint = Paint::from_color(Color::from_rgba8(220, 40, 90, 255));
pixmap.fill_path(&path, &paint, FillRule::NonZero, Transform::identity(), None);

// The ready pixels (premultiplied RGBA) are in `pixmap.data()`.
let pixels = pixmap.data();
```

### Text

```rust
use dinamika_cpu::*;

let data = std::fs::read("font.ttf")?;
let font = Font::from_slice(&data)?;

let mut pixmap = Pixmap::new(400, 120).unwrap();
pixmap.fill(Color::WHITE);

let paint = Paint::from_color(Color::BLACK);
// Baseline origin at (16, 80), em size 48px.
pixmap.fill_text(&font, "Hello", 48.0, 16.0, 80.0, &paint, Transform::identity(), None);
```

The demonstration scene can be rendered with the command:

```
cargo run -p dinamika
```
