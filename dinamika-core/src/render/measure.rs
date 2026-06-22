//! First layout pass: bottom-up, computes the "natural" size of each shape
//! (by content, if the axis is "auto").

use std::collections::HashMap;
use std::rc::Rc;

use crate::shape::{Direction, Shape};

use super::{clamp_axis, ShapePtr};

/// Computes the natural size of a shape, accounting for content and padding.
///
/// The size on each axis is clamped into the `min`/`max` bounds
/// ([`min_width`](crate::Shape::min_width), etc.). Percentage sizes
/// ([`Length::percent`](crate::Length)) are not taken into account here — they
/// are resolved on the second pass relative to the parent, so the natural size
/// (used for the parent's auto-size) is driven by content.
pub(super) fn measure(shape: &Shape, cache: &mut HashMap<ShapePtr, (f32, f32)>) -> (f32, f32) {
    let ptr = Rc::as_ptr(&shape.inner);
    if let Some(size) = cache.get(&ptr) {
        return *size;
    }

    let d = shape.inner.borrow();
    let (mut content_w, mut content_h) = (0.0_f32, 0.0_f32);
    if let Some(text) = &d.text {
        // Text shape: natural size is by content (no children).
        let (tw, th) = text.natural_size();
        content_w = tw;
        content_h = th;
    }
    let n = d.children.len();
    if n > 0 {
        let gap = d.gap.get();
        let mut main = 0.0_f32;
        let mut cross = 0.0_f32;
        for (i, c) in d.children.iter().enumerate() {
            let (cw, ch) = measure(c, cache);
            let (child_main, child_cross) = match d.direction {
                Direction::Row => (cw, ch),
                Direction::Column => (ch, cw),
            };
            main += child_main;
            if i + 1 < n {
                main += gap;
            }
            cross = cross.max(child_cross);
        }
        match d.direction {
            Direction::Row => {
                content_w = main;
                content_h = cross;
            }
            Direction::Column => {
                content_h = main;
                content_w = cross;
            }
        }
    }

    let pad_l = d.pad_left.get();
    let pad_r = d.pad_right.get();
    let pad_t = d.pad_top.get();
    let pad_b = d.pad_bottom.get();
    let wsig = d.width.get();
    let hsig = d.height.get();
    let w = if wsig > 0.0 { wsig } else { content_w + pad_l + pad_r };
    let h = if hsig > 0.0 { hsig } else { content_h + pad_t + pad_b };
    let w = clamp_axis(w, d.min_width.get(), d.max_width.get());
    let h = clamp_axis(h, d.min_height.get(), d.max_height.get());
    drop(d);

    cache.insert(ptr, (w, h));
    (w, h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::Length;

    #[test]
    fn measures_auto_size_from_children() {
        // Row container, gap 10, padding 5, two children 40x30 and 20x50.
        let container = Shape::rect()
            .direction(Direction::Row)
            .gap(10.0)
            .padding(5.0)
            .child(Shape::rect().size(40.0, 30.0))
            .child(Shape::rect().size(20.0, 50.0));
        let mut cache = HashMap::new();
        let (w, h) = measure(&container, &mut cache);
        // width: 40 + 10 + 20 + 5*2 = 80; height: max(30,50) + 5*2 = 60
        assert!((w - 80.0).abs() < 1e-3, "w={w}");
        assert!((h - 60.0).abs() < 1e-3, "h={h}");
    }

    #[test]
    fn clamps_explicit_size_to_min_max() {
        // Explicit 200x50 is clamped: width capped at 120, height raised to 80.
        let s = Shape::rect().size(200.0, 50.0).max_width(120.0).min_height(80.0);
        let mut cache = HashMap::new();
        let (w, h) = measure(&s, &mut cache);
        assert!((w - 120.0).abs() < 1e-3, "w={w}");
        assert!((h - 80.0).abs() < 1e-3, "h={h}");
    }

    #[test]
    fn min_wins_over_max_on_conflict() {
        // On conflict (min > max) the minimum takes priority, as in CSS.
        let s = Shape::rect().size(50.0, 50.0).max_width(80.0).min_width(120.0);
        let mut cache = HashMap::new();
        let (w, _) = measure(&s, &mut cache);
        assert!((w - 120.0).abs() < 1e-3, "w={w}");
    }

    #[test]
    fn percent_is_ignored_in_natural_size() {
        // Percentage size does not affect the natural size (it is resolved on
        // the second pass): an auto shape without content stays zero.
        let s = Shape::rect()
            .width(Length::percent(100.0))
            .height(Length::percent(100.0));
        let mut cache = HashMap::new();
        let (w, h) = measure(&s, &mut cache);
        assert!((w).abs() < 1e-3 && (h).abs() < 1e-3, "w={w} h={h}");
    }
}
