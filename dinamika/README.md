# dinamika

The umbrella crate of the **dinamika** project: a single dependency that brings
together the whole stack — the [`dinamika-cpu`](https://docs.rs/dinamika-cpu)
raster renderer and the [`dinamika-core`](https://docs.rs/dinamika-core)
declarative animation library.

In the spirit of [Motion Canvas](https://motioncanvas.io/): a scene of
flex-laid-out shapes whose properties are reactive signals, animated on a
timeline and rendered to PNG frames — all on the CPU, with no GPU dependency.

## Layout

This crate re-exports the two underlying crates and lifts the animation API to
its root:

| Path                    | What it is                                            |
|-------------------------|-------------------------------------------------------|
| `dinamika::core`        | the `dinamika-core` animation library                 |
| `dinamika::cpu`         | the `dinamika-cpu` raster renderer                    |
| `dinamika::*`           | flat re-export of the `dinamika-core` public API      |

So `use dinamika::*` is equivalent to `use dinamika_core::*`, and the renderer
stays reachable as `dinamika::cpu` (or `dinamika::core::cpu`).

## Install

```toml
[dependencies]
dinamika = "0.1"
```

Prefer the individual crates if you only need one layer:
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

For the full animation API (shapes, signals, the timeline, text and code with
syntax highlighting) see the [`dinamika-core`](https://docs.rs/dinamika-core)
documentation; for the drawing primitives see
[`dinamika-cpu`](https://docs.rs/dinamika-cpu).

The demonstration scene can be rendered with the command:

```
cargo run -p dinamika
```

## License

Licensed under the [MIT license](LICENSE).
