# dinamika

A declarative 2D animation toolkit for Rust, rendered on the CPU — no GPU
dependency. In the spirit of [Motion Canvas](https://motioncanvas.io/): a scene
of flex-laid-out shapes whose properties are reactive signals, animated on a
timeline and rendered to PNG frames.

## Workspace

The project is a Cargo workspace of three crates, layered from the renderer up:

| Crate                                     | Description                                                                 |
|-------------------------------------------|-----------------------------------------------------------------------------|
| [`dinamika-cpu`](dinamika-cpu/)           | Raster 2D renderer: pixmap, paths, paint and anti-aliased drawing.          |
| [`dinamika-core`](dinamika-core/)         | Declarative animation: flex shapes, reactive signals and a timeline.        |
| [`dinamika`](dinamika/)                   | Umbrella crate that re-exports both under one dependency.                    |

```
dinamika  ──►  dinamika-core  ──►  dinamika-cpu
(umbrella)     (animation)         (rendering)
```

Most users only need the umbrella crate:

```toml
[dependencies]
dinamika = "0.1"
```

Depend on the individual crates instead if you want a single layer —
[`dinamika-core`](https://crates.io/crates/dinamika-core) for animation, or
[`dinamika-cpu`](https://crates.io/crates/dinamika-cpu) for raster rendering
alone.

## Example

```rust
use dinamika::*;

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

// Animate two properties in parallel, pause, then render to PNG frames.
tl.parallel(vec![
    a.rotation(180.0).over(1.0, Easing::CubicInOut),
    b.background(Color::from_rgba8(97, 175, 239, 255)).over(1.0, Easing::Linear),
]);
tl.pause(0.25);

tl.render("outputs/demo").unwrap();
```

Render the demonstration scene with:

```
cargo run -p dinamika
```

## Documentation

- [`dinamika`](https://docs.rs/dinamika) — umbrella crate API.
- [`dinamika-core`](https://docs.rs/dinamika-core) — shapes, signals, timeline,
  text and code with syntax highlighting.
- [`dinamika-cpu`](https://docs.rs/dinamika-cpu) — drawing primitives.

See [CHANGELOG.md](CHANGELOG.md) for the release history.

## License

Licensed under the [MIT license](LICENSE).
