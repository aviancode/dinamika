# Changelog

All notable changes to `dinamika-core` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

#### Easing

- `Easing` — a set of ready-made interpolation curves: `Linear`, plus `In`/
  `Out`/`InOut` variants of `Quad`, `Cubic`, `Quart`, `Sine`, `Expo`, `Back`,
  `Bounce` and `Elastic`.

#### Signals

- `Signal<T>` — a reactive value shared by reference (`Rc<RefCell<T>>`): read
  (`get`), write (`set`) and animate (`tween_to`) into a timeline `Action`.
- `Computed` — a read-only derived signal.
- `Tweenable` — the interpolation trait, implemented for the built-in numeric
  types and `Color`.

#### Shapes

- `Shape` — a scene node with CSS-like flex layout: `Direction`, `Justify`,
  `Align`, `gap`, `padding` (with the `Padding` shorthands) and `children`.
- Shape kinds (`ShapeKind`): rectangle (`Shape::rect`), circle/ellipse
  (`Shape::circle`), a backgroundless layout container (`Shape::layout`), text
  (`Shape::text`) and code (`Shape::code`).
- Signal-backed properties: position, width/height (as a `Length` — pixels or a
  percentage of the parent), min/max size bounds, background, corner radius,
  opacity, rotation, scale, gap and per-side padding.
- A fluent builder: every animatable setter sets the value immediately and
  returns a `Tween`/`PaddingTween` handle that dereferences back into the
  `Shape`, so the chain keeps flowing.
- `IntoChildren` — `children(...)` accepts a single nested shape or a
  collection/iterator of any `Into<Shape>`.

#### Text

- Text shapes with CSS-like style: font (`.ttf`/`.otf` bytes, or a `.ttc`
  collection face), font size, color, `TextAlign`, letter spacing and line
  height — all of the geometry and color animatable via `.over(...)`.
- Content editing — `content`, `append`, `prepend`, `insert` and `rewrite`
  (with `TextPos`: a character index, `line(n)` or `infinite()`) — returning a
  `TextEdit` that can animate the change (instant spawn, typing, smoothing).
- Highlighting — `highlight`/`clear_highlight` over a `[from, to)` range,
  returning a `HighlightEdit`; multiple ranges in one `parallel` merge into a
  single consistent transition.

#### Code

- Code shapes (`Shape::code`) — the same as text, but glyphs are colored
  per-character by syntax highlighting instead of a single color.
- `Palette` — a "token category → color" table built fluently (`keyword`,
  `string`, `comment`, `number`, `function`, `type_`, `constant`, `operator`,
  `variable`, `punctuation`) over a base color.
- `Language` — the grammar selector (`Rust`, `JavaScript`, `Python`, `C`,
  `Cpp`, `Go`, `Json`, `Html`, `Css`, `Java`, `Bash`, and `PlainText` for no
  parsing), backed by the built-in Sublime Text grammars from `syntect`.
- Per-character colors are cached by `(text, language, palette)`, so static code
  is not re-parsed every frame.

#### Timeline

- `Timeline` — composition of animations over time, with interior mutability
  (no `mut` needed): `pause`, `parallel`, `sequence` and `cascade` (a sequence
  with a fixed gap between neighbors).
- Shapes are registered for drawing via `Shape::on`; a registered shape
  remembers its timeline, so a single animation can be added by simply writing
  it as an expression — without wrapping it in `sequence`/`parallel`.
- `Action` and the free combinators `pause`, `delay`, `parallel`, `sequence`,
  `cascade`.
- Deterministic sampling: `seek` resets all signals to their baseline, then
  applies the active tweens in start order; the flattened sampling plan is
  cached between frames.
- `frame(t)` renders a single `Pixmap`; `duration` reports the total length.

#### Rendering

- `render_scene` — a two-pass flex layout (natural sizes, then placement) and
  rasterization of the shape tree onto a premultiplied-RGBA `Pixmap`, built on
  top of `dinamika-cpu`.
- Rotation and scale are applied around a shape's center, together with its
  whole subtree; nested opacity multiplies.

#### Output

- `Timeline::render(dir)` writes the whole animation as numbered PNG frames
  (`000001.png`, `000002.png`, …) at the timeline's frame rate.
- The `scene_dir!` and `render!` macros save frames into `outputs/<scene>` next
  to the calling scene's source file.

#### Re-exports

- The renderer is re-exported as `dinamika_core::cpu`, with the frequently used
  types (`Color`, `Pixmap`, `Paint`, `Transform`, gradients, …) lifted to the
  crate root for convenience.

### Documentation

- Documented the architecture and usage in the crate-level docs and `README.md`.

[Unreleased]: https://github.com/aviancode/dinamika
</content>
</invoke>
