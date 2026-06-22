//! Easing functions for interpolating animations.
//!
//! [`Easing`] is an enumeration of ready-made curves. Each takes a normalized
//! time `0.0..=1.0` and returns the adjusted progress (usually also `0..=1`,
//! but `Back`/`Elastic` may overshoot the bounds — that's expected).

use std::f32::consts::PI;

/// Animation easing curve.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum Easing {
    /// Linear (no smoothing).
    #[default]
    Linear,

    QuadIn,
    QuadOut,
    QuadInOut,

    CubicIn,
    CubicOut,
    CubicInOut,

    QuartIn,
    QuartOut,
    QuartInOut,

    SineIn,
    SineOut,
    SineInOut,

    ExpoIn,
    ExpoOut,
    ExpoInOut,

    /// Slight backward "overshoot" at the start.
    BackIn,
    /// Slight "overshoot" at the end.
    BackOut,
    BackInOut,

    /// Damped "bounce" at the start.
    BounceIn,
    /// Damped "bounce" at the end.
    BounceOut,
    BounceInOut,

    /// Spring at the start.
    ElasticIn,
    /// Spring at the end.
    ElasticOut,
    ElasticInOut,
}

impl Easing {
    /// Applies the curve to the normalized time `t` (clamped to `0..=1`).
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,

            Easing::QuadIn => t * t,
            Easing::QuadOut => 1.0 - (1.0 - t) * (1.0 - t),
            Easing::QuadInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - 2.0 * (1.0 - t) * (1.0 - t)
                }
            }

            Easing::CubicIn => t * t * t,
            Easing::CubicOut => {
                let u = 1.0 - t;
                1.0 - u * u * u
            }
            Easing::CubicInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    let u = -2.0 * t + 2.0;
                    1.0 - u * u * u / 2.0
                }
            }

            Easing::QuartIn => t * t * t * t,
            Easing::QuartOut => {
                let u = 1.0 - t;
                1.0 - u * u * u * u
            }
            Easing::QuartInOut => {
                if t < 0.5 {
                    8.0 * t * t * t * t
                } else {
                    let u = -2.0 * t + 2.0;
                    1.0 - u * u * u * u / 2.0
                }
            }

            Easing::SineIn => 1.0 - (t * PI / 2.0).cos(),
            Easing::SineOut => (t * PI / 2.0).sin(),
            Easing::SineInOut => -((PI * t).cos() - 1.0) / 2.0,

            Easing::ExpoIn => {
                if t <= 0.0 {
                    0.0
                } else {
                    2f32.powf(10.0 * t - 10.0)
                }
            }
            Easing::ExpoOut => {
                if t >= 1.0 {
                    1.0
                } else {
                    1.0 - 2f32.powf(-10.0 * t)
                }
            }
            Easing::ExpoInOut => {
                if t <= 0.0 {
                    0.0
                } else if t >= 1.0 {
                    1.0
                } else if t < 0.5 {
                    2f32.powf(20.0 * t - 10.0) / 2.0
                } else {
                    (2.0 - 2f32.powf(-20.0 * t + 10.0)) / 2.0
                }
            }

            Easing::BackIn => {
                let c1 = 1.70158;
                let c3 = c1 + 1.0;
                c3 * t * t * t - c1 * t * t
            }
            Easing::BackOut => {
                let c1 = 1.70158;
                let c3 = c1 + 1.0;
                let u = t - 1.0;
                1.0 + c3 * u * u * u + c1 * u * u
            }
            Easing::BackInOut => {
                let c1 = 1.70158;
                let c2 = c1 * 1.525;
                if t < 0.5 {
                    let u = 2.0 * t;
                    (u * u * ((c2 + 1.0) * u - c2)) / 2.0
                } else {
                    let u = 2.0 * t - 2.0;
                    (u * u * ((c2 + 1.0) * u + c2) + 2.0) / 2.0
                }
            }

            Easing::BounceIn => 1.0 - bounce_out(1.0 - t),
            Easing::BounceOut => bounce_out(t),
            Easing::BounceInOut => {
                if t < 0.5 {
                    (1.0 - bounce_out(1.0 - 2.0 * t)) / 2.0
                } else {
                    (1.0 + bounce_out(2.0 * t - 1.0)) / 2.0
                }
            }

            Easing::ElasticIn => {
                if t <= 0.0 {
                    0.0
                } else if t >= 1.0 {
                    1.0
                } else {
                    let c4 = (2.0 * PI) / 3.0;
                    -(2f32.powf(10.0 * t - 10.0)) * ((10.0 * t - 10.75) * c4).sin()
                }
            }
            Easing::ElasticOut => {
                if t <= 0.0 {
                    0.0
                } else if t >= 1.0 {
                    1.0
                } else {
                    let c4 = (2.0 * PI) / 3.0;
                    2f32.powf(-10.0 * t) * ((10.0 * t - 0.75) * c4).sin() + 1.0
                }
            }
            Easing::ElasticInOut => {
                if t <= 0.0 {
                    0.0
                } else if t >= 1.0 {
                    1.0
                } else {
                    let c5 = (2.0 * PI) / 4.5;
                    if t < 0.5 {
                        -(2f32.powf(20.0 * t - 10.0) * ((20.0 * t - 11.125) * c5).sin()) / 2.0
                    } else {
                        (2f32.powf(-20.0 * t + 10.0) * ((20.0 * t - 11.125) * c5).sin()) / 2.0 + 1.0
                    }
                }
            }
        }
    }
}

fn bounce_out(t: f32) -> f32 {
    let n1 = 7.5625;
    let d1 = 2.75;
    if t < 1.0 / d1 {
        n1 * t * t
    } else if t < 2.0 / d1 {
        let t = t - 1.5 / d1;
        n1 * t * t + 0.75
    } else if t < 2.5 / d1 {
        let t = t - 2.25 / d1;
        n1 * t * t + 0.9375
    } else {
        let t = t - 2.625 / d1;
        n1 * t * t + 0.984375
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_are_anchored() {
        for e in [
            Easing::Linear,
            Easing::QuadInOut,
            Easing::CubicInOut,
            Easing::SineInOut,
            Easing::ExpoInOut,
            Easing::BounceOut,
            Easing::ElasticOut,
        ] {
            assert!((e.apply(0.0) - 0.0).abs() < 1e-3, "{e:?} at 0");
            assert!((e.apply(1.0) - 1.0).abs() < 1e-3, "{e:?} at 1");
        }
    }

    #[test]
    fn linear_is_identity() {
        assert!((Easing::Linear.apply(0.37) - 0.37).abs() < 1e-6);
    }

    #[test]
    fn clamps_out_of_range() {
        assert_eq!(Easing::Linear.apply(-1.0), 0.0);
        assert_eq!(Easing::Linear.apply(2.0), 1.0);
    }
}
