//! Second layout pass: top-down, places children according to the
//! [`Justify`]/[`Align`] rules and fills rectangles onto the pixmap.
//!
//! Rotation accumulates down the tree: a parent's rotation applies to the whole
//! subtree. Opacity, on the other hand, is applied to each group as a single
//! whole — the subtree of a semi-transparent shape is first drawn into an
//! offscreen layer and then composited with its opacity. Otherwise overlapping
//! children under a semi-transparent parent would show through twice at the
//! intersection.

use std::collections::HashMap;

use dinamika_cpu::{FillRule, Paint, PathBuilder, Pixmap, Rect, Transform};

use crate::shape::{Align, Direction, Justify, Shape, ShapeKind};

use super::measure::measure;
use super::{clamp_axis, ShapePtr};

/// Draws a shape (and its subtree) into the rectangle `(x, y, w, h)`.
///
/// If the shape has children and its own opacity is < 1, the subtree is first
/// drawn into an offscreen layer at full opacity and then composited with that
/// opacity as a single whole — so overlapping children do not show through
/// twice. A single fill without children does not overlap itself, so it needs no
/// layer and the opacity is applied directly to the paint.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw(
    pixmap: &mut Pixmap,
    shape: &Shape,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    parent_transform: Transform,
    cache: &mut HashMap<ShapePtr, (f32, f32)>,
) {
    let (opacity, rotation, scale, has_children, is_text) = {
        let d = shape.inner.borrow();
        (
            d.opacity.get().clamp(0.0, 1.0),
            d.rotation.get(),
            d.scale.get(),
            !d.children.is_empty(),
            d.text.is_some(),
        )
    };
    if opacity <= 0.0 {
        return;
    }

    // Rotation and scale accumulate around the shape's center and apply to the
    // whole subtree.
    let mut transform = parent_transform;
    if rotation.abs() > 1e-6 {
        let cx = x + w * 0.5;
        let cy = y + h * 0.5;
        transform = transform.pre_concat(Transform::from_rotate_at(rotation, cx, cy));
    }
    if (scale - 1.0).abs() > 1e-6 {
        let cx = x + w * 0.5;
        let cy = y + h * 0.5;
        transform = transform.pre_concat(
            Transform::from_translate(cx, cy)
                .pre_concat(Transform::from_scale(scale, scale))
                .pre_concat(Transform::from_translate(-cx, -cy)),
        );
    }

    if opacity < 1.0 && (has_children || is_text) {
        // Group isolation: draw the subtree into a separate layer at full
        // opacity and composite the result as a whole. For text this is needed
        // so that the background and glyphs under a shared opacity do not show
        // through at the seam.
        let mut layer =
            Pixmap::new(pixmap.width(), pixmap.height()).expect("non-zero offscreen layer size");
        paint_shape(&mut layer, shape, x, y, w, h, transform, 1.0, cache);
        pixmap.draw_pixmap(&layer, 0, 0, opacity, dinamika_cpu::BlendMode::SourceOver);
    } else {
        paint_shape(pixmap, shape, x, y, w, h, transform, opacity, cache);
    }
}

/// Fills the shape itself with opacity `self_opacity` and lays out its children.
///
/// `transform` already includes the accumulated rotation. The children's opacity
/// is determined by their own properties (via the recursive [`draw`]), not
/// inherited as a multiplier: inheritance is collapsed at the group-layer level
/// in [`draw`].
#[allow(clippy::too_many_arguments)]
fn paint_shape(
    pixmap: &mut Pixmap,
    shape: &Shape,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    transform: Transform,
    self_opacity: f32,
    cache: &mut HashMap<ShapePtr, (f32, f32)>,
) {
    let d = shape.inner.borrow();

    // Fill of the shape itself. A layout container ([`ShapeKind::Layout`]) has
    // no background — it only positions children without drawing anything itself.
    if w > 0.0 && h > 0.0 {
        if let Some(rect) = Rect::from_xywh(x, y, w, h) {
            let path = match d.kind {
                ShapeKind::Rect => Some(PathBuilder::from_round_rect(rect, d.radius.get())),
                ShapeKind::Circle => {
                    let mut b = PathBuilder::new();
                    b.push_oval(rect);
                    Some(b.finish().expect("a non-empty oval contour"))
                }
                ShapeKind::Layout => None,
                // For text and code the background is drawn like a rectangle
                // (with rounding), but only when it is opaque — transparent by
                // default.
                ShapeKind::Text | ShapeKind::Code => {
                    if d.background.get().alpha() > 0.0 {
                        Some(PathBuilder::from_round_rect(rect, d.radius.get()))
                    } else {
                        None
                    }
                }
            };
            if let Some(path) = path {
                let mut paint = Paint::from_color(d.background.get());
                paint.opacity = self_opacity;
                pixmap.fill_path(&path, &paint, FillRule::NonZero, transform, None);
            }
        }
    }

    // Text shape: draw the glyphs over the (optional) background and return —
    // text has no children.
    if let Some(text) = &d.text {
        let pad_l = d.pad_left.get();
        let pad_r = d.pad_right.get();
        let pad_t = d.pad_top.get();
        let content_x = x + pad_l;
        let content_y = y + pad_t;
        let content_w = (w - pad_l - pad_r).max(0.0);
        // The path is given in content-area coordinates — shift it by the
        // area's position on top of the accumulated rotation/scale.
        let tf = transform.pre_concat(Transform::from_translate(content_x, content_y));
        // A code shape colors glyphs per-character by highlight: one path per
        // color. Plain text uses one color for the whole frame (with animatable
        // `color`).
        if let Some(code) = &d.code {
            let default = code.foreground();
            for (path, color, alpha) in
                text.draw_layers_colored(content_w, default, &|s| code.char_colors(s))
            {
                let mut paint = Paint::from_color(color);
                paint.opacity = self_opacity * alpha;
                pixmap.fill_path(&path, &paint, FillRule::NonZero, tf, None);
            }
        } else {
            // Frame layers: static/typing produce one layer; smoothing produces
            // the opaque shared parts plus a crossfade of the changed middle
            // with its own alphas.
            for (path, alpha) in text.draw_layers(content_w) {
                let mut paint = Paint::from_color(text.color());
                paint.opacity = self_opacity * alpha;
                pixmap.fill_path(&path, &paint, FillRule::NonZero, tf, None);
            }
        }
        return;
    }

    let n = d.children.len();
    if n == 0 {
        return;
    }

    // Content area minus padding.
    let pad_l = d.pad_left.get();
    let pad_r = d.pad_right.get();
    let pad_t = d.pad_top.get();
    let pad_b = d.pad_bottom.get();
    let gap = d.gap.get();
    let content_x = x + pad_l;
    let content_y = y + pad_t;
    let content_w = (w - pad_l - pad_r).max(0.0);
    let content_h = (h - pad_t - pad_b).max(0.0);
    let (content_main, content_cross) = match d.direction {
        Direction::Row => (content_w, content_h),
        Direction::Column => (content_h, content_w),
    };

    // Natural sizes of the children and their final size along the main axis.
    // The percentage size (the fraction from Length::percent) is resolved here,
    // relative to the parent's content area, and overrides the natural size.
    let sizes: Vec<(f32, f32)> = d.children.iter().map(|c| measure(c, cache)).collect();
    let mains: Vec<f32> = d
        .children
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let natural_main = match d.direction {
                Direction::Row => sizes[i].0,
                Direction::Column => sizes[i].1,
            };
            resolve_main(c, d.direction, natural_main, content_main)
        })
        .collect();

    let total: f32 = mains.iter().sum::<f32>() + gap * (n.saturating_sub(1) as f32);
    let free = content_main - total;
    let (mut cursor, between) = match d.justify {
        Justify::Start => (0.0, gap),
        Justify::Center => (free * 0.5, gap),
        Justify::End => (free, gap),
        Justify::SpaceBetween => {
            if n > 1 {
                (0.0, gap + free / (n - 1) as f32)
            } else {
                (free * 0.5, gap)
            }
        }
        Justify::SpaceAround => {
            let unit = free / n as f32;
            (unit * 0.5, gap + unit)
        }
    };

    for (i, c) in d.children.iter().enumerate() {
        let main_size = mains[i];
        let cross_size = resolve_cross(c, d.direction, d.align, sizes[i], content_cross);
        let cross_pos = match d.align {
            Align::Start | Align::Stretch => 0.0,
            Align::Center => (content_cross - cross_size) * 0.5,
            Align::End => content_cross - cross_size,
        };

        let (cx, cy, cw, ch) = match d.direction {
            Direction::Row => (content_x + cursor, content_y + cross_pos, main_size, cross_size),
            Direction::Column => (content_x + cross_pos, content_y + cursor, cross_size, main_size),
        };

        draw(pixmap, c, cx, cy, cw, ch, transform, cache);
        cursor += main_size + between;
    }
}

/// The child's final size along the main axis. The child's percentage size
/// ([`Length::percent`](crate::Length) on the axis that is the parent's main
/// one) overrides the natural size and is taken as a fraction of the parent's
/// content area `content_main`; the result is clamped to the child's `min`/`max`
/// bounds. `natural_main` is already clamped during the `measure` pass.
fn resolve_main(child: &Shape, direction: Direction, natural_main: f32, content_main: f32) -> f32 {
    let d = child.inner.borrow();
    let (percent, min, max) = match direction {
        Direction::Row => (d.width_percent, d.min_width.get(), d.max_width.get()),
        Direction::Column => (d.height_percent, d.min_height.get(), d.max_height.get()),
    };
    match percent {
        Some(p) => clamp_axis(p * content_main, min, max),
        None => natural_main,
    }
}

/// The child's final size along the cross axis. The child's percentage size
/// overrides everything; otherwise [`Align::Stretch`] stretches it across the
/// whole cross axis `content_cross`, while other alignments keep the natural
/// size. The result is clamped to the child's `min`/`max` bounds.
fn resolve_cross(
    child: &Shape,
    direction: Direction,
    align: Align,
    natural: (f32, f32),
    content_cross: f32,
) -> f32 {
    let (nw, nh) = natural;
    let d = child.inner.borrow();
    let (percent, min, max, natural_cross) = match direction {
        Direction::Row => (d.height_percent, d.min_height.get(), d.max_height.get(), nh),
        Direction::Column => (d.width_percent, d.min_width.get(), d.max_width.get(), nw),
    };
    let raw = match percent {
        Some(p) => p * content_cross,
        None if align == Align::Stretch => content_cross,
        None => natural_cross,
    };
    clamp_axis(raw, min, max)
}
