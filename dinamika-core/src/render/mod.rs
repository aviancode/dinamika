//! Layout of the shape tree and rasterization into a [`Pixmap`].
//!
//! Layout is two-pass:
//! 1. [`measure`](measure::measure) — bottom-up, computes the "natural" size of
//!    each shape (by content, if the axis is "auto");
//! 2. [`draw`](paint::draw) — top-down, places children according to the
//!    [`Justify`](crate::Justify)/[`Align`](crate::Align) rules and fills
//!    rectangles onto the pixmap.
//!
//! Accordingly the submodules split the passes: [`measure`] is the first,
//! [`paint`] the second (filling with rotation accumulation and group opacity
//! isolation).

use std::cell::RefCell;
use std::collections::HashMap;

use dinamika_cpu::{Color, Pixmap, Transform};

use crate::shape::{Shape, ShapeData};

mod measure;
mod paint;

use measure::measure;
use paint::draw;

/// Size-cache key — shape identity by the address of its cell.
type ShapePtr = *const RefCell<ShapeData>;

/// Clamps an axis size into the bounds `[min, max]`. A value `<= 0` for a bound
/// means "no limit". On conflict (`min > max`) the minimum takes priority — it
/// is applied last, as in CSS.
pub(super) fn clamp_axis(value: f32, min: f32, max: f32) -> f32 {
    let mut v = value;
    if max > 0.0 {
        v = v.min(max);
    }
    if min > 0.0 {
        v = v.max(min);
    }
    v
}

/// Draws a scene of root shapes over the `background`.
pub fn render_scene(width: u32, height: u32, background: Color, shapes: &[Shape]) -> Pixmap {
    let mut pixmap = Pixmap::new(width.max(1), height.max(1)).expect("non-zero pixmap size");
    pixmap.fill(background);

    // The canvas plays the role of "parent" for root shapes: their percentage
    // size ([`Length::percent`]) is resolved relative to its dimensions. For
    // children this is done by the second layout pass, but the root has no
    // parent, so we resolve its fraction here — otherwise `width(percent(100))`
    // would collapse to the content size, and `Justify`/`Align` would have no
    // free space.
    let canvas_w = width as f32;
    let canvas_h = height as f32;

    let mut cache: HashMap<ShapePtr, (f32, f32)> = HashMap::new();
    for shape in shapes {
        let (nat_w, nat_h) = measure(shape, &mut cache);
        let (x, y, w, h) = {
            let d = shape.inner.borrow();
            let w = match d.width_percent {
                Some(p) => clamp_axis(p * canvas_w, d.min_width.get(), d.max_width.get()),
                None => nat_w,
            };
            let h = match d.height_percent {
                Some(p) => clamp_axis(p * canvas_h, d.min_height.get(), d.max_height.get()),
                None => nat_h,
            };
            (d.x.get(), d.y.get(), w, h)
        };
        draw(&mut pixmap, shape, x, y, w, h, Transform::identity(), &mut cache);
    }
    pixmap
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::{Align, Direction, Justify, Length};

    fn alpha_at(pm: &Pixmap, x: u32, y: u32) -> u8 {
        pm.pixel(x, y).unwrap().alpha()
    }

    #[test]
    fn fills_rect_pixels() {
        let s = Shape::rect()
            .at(10.0, 10.0)
            .size(40.0, 40.0)
            .background(Color::from_rgba8(255, 0, 0, 255));
        let pm = render_scene(80, 80, Color::TRANSPARENT, std::slice::from_ref(&s));
        assert_eq!(alpha_at(&pm, 30, 30), 255); // inside
        assert_eq!(alpha_at(&pm, 70, 70), 0); // outside
    }

    #[test]
    fn centers_child_in_container() {
        // 100x100 container at (0,0), one 20x20 child, justify+align Center.
        let child = Shape::rect().size(20.0, 20.0).background(Color::from_rgba8(0, 0, 255, 255));
        let container = Shape::rect()
            .size(100.0, 100.0)
            .background(Color::TRANSPARENT)
            .justify(Justify::Center)
            .align(Align::Center)
            .child(child);
        let pm = render_scene(100, 100, Color::TRANSPARENT, std::slice::from_ref(&container));
        // The child should be centered: roughly (40..60, 40..60).
        assert_eq!(alpha_at(&pm, 50, 50), 255);
        assert_eq!(alpha_at(&pm, 10, 10), 0);
        assert_eq!(alpha_at(&pm, 90, 90), 0);
    }

    #[test]
    fn root_percent_size_fills_canvas_and_centers_child() {
        // A root layout at 100% on both axes should expand to the canvas size
        // rather than collapse to its content — then Justify/Align center the
        // child across the whole frame instead of leaving it in the corner.
        let child = Shape::rect().size(20.0, 20.0).background(Color::from_rgba8(0, 0, 255, 255));
        let root = Shape::layout()
            .width(Length::percent(100.0))
            .height(Length::percent(100.0))
            .justify(Justify::Center)
            .align(Align::Center)
            .child(child);
        let pm = render_scene(100, 100, Color::TRANSPARENT, std::slice::from_ref(&root));
        // The 20x20 child is centered in the 100x100 canvas: box ~ (40..60, 40..60).
        assert_eq!(alpha_at(&pm, 50, 50), 255);
        assert_eq!(alpha_at(&pm, 10, 10), 0); // not in the corner
        assert_eq!(alpha_at(&pm, 90, 90), 0);
    }

    #[test]
    fn scale_enlarges_shape_around_center() {
        // A 20x20 shape at the center of an 80x80 field, scaled 2x, fills pixels
        // beyond its original box (around the center 40,40).
        let s = Shape::rect()
            .at(30.0, 30.0)
            .size(20.0, 20.0)
            .scale(2.0)
            .background(Color::from_rgba8(0, 255, 0, 255));
        let pm = render_scene(80, 80, Color::TRANSPARENT, std::slice::from_ref(&s));
        assert_eq!(alpha_at(&pm, 40, 40), 255); // center
        // Point (25,40) is outside the original box (30..50) but inside after scaling.
        assert_eq!(alpha_at(&pm, 25, 40), 255);
        assert_eq!(alpha_at(&pm, 10, 40), 0); // still outside
    }

    #[test]
    fn percent_fills_parent_content_area() {
        // 100x100 container, a single child at 100% on both axes with an opaque
        // background — fills the parent's whole content area.
        let child = Shape::rect()
            .width(Length::percent(100.0))
            .height(Length::percent(100.0))
            .background(Color::from_rgba8(0, 200, 0, 255));
        let container = Shape::rect()
            .size(100.0, 100.0)
            .background(Color::TRANSPARENT)
            .child(child);
        let pm = render_scene(100, 100, Color::TRANSPARENT, std::slice::from_ref(&container));
        assert_eq!(alpha_at(&pm, 5, 5), 255);
        assert_eq!(alpha_at(&pm, 95, 95), 255);
    }

    #[test]
    fn max_clamps_stretched_cross_size() {
        // 100x100 container, Column + Stretch. The child fills 100% of the height
        // (main axis), while the width stretch is clamped by max_width=40.
        let child = Shape::rect()
            .height(Length::percent(100.0))
            .max_width(40.0)
            .background(Color::from_rgba8(0, 0, 200, 255));
        let container = Shape::rect()
            .size(100.0, 100.0)
            .background(Color::TRANSPARENT)
            .direction(Direction::Column)
            .align(Align::Stretch)
            .child(child);
        let pm = render_scene(100, 100, Color::TRANSPARENT, std::slice::from_ref(&container));
        assert_eq!(alpha_at(&pm, 20, 50), 255); // inside the clamped 40px width
        assert_eq!(alpha_at(&pm, 60, 50), 0); // beyond it
        assert_eq!(alpha_at(&pm, 20, 95), 255); // height filled to 100%
    }

    #[test]
    fn group_opacity_applies_to_subtree() {
        let child = Shape::rect().size(20.0, 20.0).background(Color::from_rgba8(255, 0, 0, 255));
        let parent = Shape::rect()
            .at(0.0, 0.0)
            .size(20.0, 20.0)
            .background(Color::TRANSPARENT)
            .opacity(0.5)
            .child(child);
        let pm = render_scene(20, 20, Color::TRANSPARENT, std::slice::from_ref(&parent));
        let a = alpha_at(&pm, 10, 10);
        // ~50% of 255
        assert!((a as i32 - 128).abs() <= 4, "alpha={a}");
    }

    #[test]
    fn overlapping_children_do_not_double_show_through() {
        // Two opaque children overlap (negative gap) under a semi-transparent
        // (50%) group. At the intersection the opacity should stay ~50%, not
        // accumulate into a higher opacity.
        let red = Color::from_rgba8(255, 0, 0, 255);
        let parent = Shape::rect()
            .at(0.0, 0.0)
            .background(Color::TRANSPARENT)
            .opacity(0.5)
            .direction(Direction::Row)
            .gap(-10.0)
            .child(Shape::rect().size(20.0, 20.0).background(red))
            .child(Shape::rect().size(20.0, 20.0).background(red));
        let pm = render_scene(40, 20, Color::TRANSPARENT, std::slice::from_ref(&parent));
        let solo = alpha_at(&pm, 5, 10); // only one child
        let overlap = alpha_at(&pm, 15, 10); // overlap of two children
        assert!((solo as i32 - 128).abs() <= 4, "solo={solo}");
        assert!((overlap as i32 - 128).abs() <= 4, "overlap={overlap}");
    }
}
