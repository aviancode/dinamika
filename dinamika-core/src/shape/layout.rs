//! Flex-layout parameters for children: axis ([`Direction`]), distribution
//! ([`Justify`]), alignment ([`Align`]) and padding ([`Padding`]).

/// The children's layout axis (analogous to `flex-direction`).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum Direction {
    /// Left to right (the main axis is horizontal).
    #[default]
    Row,
    /// Top to bottom (the main axis is vertical).
    Column,
}

/// Distribution along the main axis (analogous to `justify-content`).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum Justify {
    #[default]
    Start,
    Center,
    End,
    /// The outer items are pressed to the edges, the gaps are equal.
    SpaceBetween,
    /// Equal gaps around each item.
    SpaceAround,
}

/// Alignment along the cross axis (analogous to `align-items`).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
    /// Stretch across the whole cross axis of the container.
    Stretch,
}

/// The shape's inner padding by side.
///
/// Built from a value in the style of CSS shorthands:
///
/// - `20.0` — the same on all sides;
/// - `(10.0, 20.0)` — `(vertical, horizontal)`;
/// - `(5.0, 10.0, 15.0, 20.0)` — `(top, right, bottom, left)`.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Padding {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl From<f32> for Padding {
    /// The same padding on all sides.
    fn from(v: f32) -> Self {
        Padding { top: v, right: v, bottom: v, left: v }
    }
}

impl From<(f32, f32)> for Padding {
    /// `(vertical, horizontal)` — like the two-value CSS shorthand.
    fn from((vertical, horizontal): (f32, f32)) -> Self {
        Padding { top: vertical, right: horizontal, bottom: vertical, left: horizontal }
    }
}

impl From<(f32, f32, f32, f32)> for Padding {
    /// `(top, right, bottom, left)` — like the four-value CSS shorthand.
    fn from((top, right, bottom, left): (f32, f32, f32, f32)) -> Self {
        Padding { top, right, bottom, left }
    }
}

/// A length along one axis: absolute in pixels, or a fraction of the parent's
/// content area in percent. Passed to [`width`](crate::Shape::width) and
/// [`height`](crate::Shape::height).
///
/// Constructed with the constructor methods:
///
/// - [`Length::pixel`] — an absolute size in pixels; participates in animation
///   (via [`over`](crate::Tween::over)) and in the parent's natural size;
/// - [`Length::percent`] — a fraction of the parent in percent (`100.0` — 100%,
///   `50.0` — half); resolved on the second layout pass relative to the parent
///   and overrides the pixel size. The fraction itself is not animated.
///
/// ```
/// # use dinamika_core::*;
/// let full = Shape::rect().width(Length::percent(100.0)); // the parent's full width
/// let fixed = Shape::rect().width(Length::pixel(120.0));  // exactly 120 px
/// ```
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Length {
    /// An absolute size in pixels.
    Pixel(f32),
    /// A fraction of the parent's content area in percent (`100.0` — 100%).
    Percent(f32),
}

impl Length {
    /// An absolute length in pixels (`<= 0` — "auto", size by content).
    pub fn pixel(value: f32) -> Length {
        Length::Pixel(value)
    }

    /// A length as a fraction of the parent in percent: `percent(100.0)` — 100%,
    /// `percent(50.0)` — half.
    pub fn percent(value: f32) -> Length {
        Length::Percent(value)
    }
}
