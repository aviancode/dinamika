# dinamika-core

A declarative animation library built on top of the
[`dinamika-cpu`](https://docs.rs/dinamika-cpu) raster renderer. In the spirit of
[Motion Canvas](https://motioncanvas.io/): a scene of flex-laid-out shapes whose
properties are reactive signals, animated on a timeline and rendered to PNG
frames.

It consists of three parts:

- **Shapes** (`Shape`) — scene nodes with CSS-like flex layout and
  signal-backed properties.
- **Signals** (`Signal`, `Computed`) — reactive values that can be read,
  written and animated.
- **Timeline** (`Timeline`) — composition of animations over time: pause,
  parallel and sequence/cascade.

## Features

- **Shapes** — rectangle (`Shape::rect`), circle/ellipse (`Shape::circle`), a
  backgroundless layout container (`Shape::layout`), text (`Shape::text`) and
  code (`Shape::code`).
- **Flex layout** — `Direction`, `Justify`, `Align`, `gap`, per-side `padding`
  (with CSS-like shorthands), `children`, and sizes as a `Length` (pixels or a
  percentage of the parent) with min/max bounds.
- **Animatable properties** — position, size, background, corner radius,
  opacity, rotation, scale, gap and padding. Each setter sets the value
  immediately and returns a handle; append `.over(duration, easing)` to animate.
- **Text** — CSS-like style (font, font size, color, alignment, letter spacing,
  line height), content edits (spawn/typing/smoothing) and range highlighting.
- **Code** — the same text, but colored per-character by syntax highlighting via
  a manually configured `Palette` and a `Language` (powered by `syntect`).
- **Easing** — a full set of curves (`Quad`/`Cubic`/`Quart`/`Sine`/`Expo`/
  `Back`/`Bounce`/`Elastic`, each in `In`/`Out`/`InOut`).
- **Timeline** — `pause`, `parallel`, `sequence` and `cascade`; a registered
  shape's single animation can be added as a plain expression.
- **Output** — render the whole animation to numbered PNG frames, directly or
  via the `scene_dir!`/`render!` macros.

## Architecture

| Module     | Purpose                                                            |
|------------|-------------------------------------------------------------------|
| `easing`   | interpolation curves (`Easing`)                                   |
| `signal`   | reactive values (`Signal`, `Computed`, `Tweenable`)              |
| `shape`    | scene nodes, flex layout, text and code                          |
| `timeline` | composition of animations over time and scene sampling          |
| `render`   | two-pass flex layout and rasterization onto a `Pixmap`          |
| `output`   | saving frames to PNG (`render`, `scene_dir!`/`render!`)          |

The renderer is re-exported as `dinamika_core::cpu`, with frequently used types
(`Color`, `Pixmap`, `Paint`, `Transform`, gradients, …) lifted to the crate root.

## Example

```rust
use dinamika_core::*;

// The timeline is created first (interior mutability — no `mut`).
let tl = Timeline::new(320, 160, Color::from_rgba8(20, 20, 24, 255), 30.0);

// Scene: a row container with two squares, registered on the timeline.
let a = Shape::rect().background(Color::from_rgba8(229, 192, 123, 255)).size(60.0, 60.0);
let b = Shape::rect().background(Color::from_rgba8(152, 195, 121, 255)).size(60.0, 60.0);
let _row = Shape::rect()
    .at(20.0, 20.0)
    .background(Color::from_rgba8(40, 44, 52, 255))
    .radius(12.0)
    .direction(Direction::Row)
    .gap(20.0)
    .padding(20.0)
    .align(Align::Center)
    .child(a.clone())
    .child(b.clone())
    .on(&tl);

// Move and recolor in parallel, wait, then a sequence of opacities.
tl.parallel(vec![
    a.rotation(180.0).over(1.0, Easing::CubicInOut),
    b.background(Color::from_rgba8(97, 175, 239, 255)).over(1.0, Easing::Linear),
]);
tl.pause(0.25);
tl.sequence(vec![
    a.opacity(0.2).over(0.5, Easing::QuadOut),
    b.opacity(0.2).over(0.5, Easing::QuadOut),
]);

// A single frame (RGBA, premultiplied alpha)…
let frame = tl.frame(0.5);
assert_eq!(frame.width(), 320);

// …or render the whole animation to PNG frames.
tl.render("outputs/demo").unwrap();
```

### Set a value or animate it

Each animatable property has one setter that takes a **value**. It applies the
value immediately and returns a lightweight handle that dereferences back into
the `Shape`, so the builder chain keeps flowing. The same setter, with
`.over(...)`, builds a tween for the timeline:

```rust
use dinamika_core::*;

let card = Shape::rect().at(40.0, 40.0).size(320.0, 120.0);

// Set now:
card.background(Color::from_rgba8(40, 44, 52, 255)).radius(16.0);

// Animate (on the timeline):
let _move = card.x(120.0).over(1.0, Easing::CubicInOut);
```

### Code with syntax highlighting

```rust,no_run
use dinamika_core::*;

let bytes = std::fs::read("Consolas.ttf").unwrap();
let snippet = Shape::code("fn main() {\n    println!(\"hi\");\n}")
    .font(bytes)
    .font_size(28.0)
    .language(Language::Rust)
    .palette(
        Palette::new(Color::from_rgba8(212, 212, 212, 255))
            .keyword(Color::from_rgba8(197, 134, 192, 255))
            .string(Color::from_rgba8(206, 145, 120, 255))
            .number(Color::from_rgba8(181, 206, 168, 255)),
    );
```

The demonstration scene can be rendered with the command:

```
cargo run -p dinamika
```
</content>
</invoke>
