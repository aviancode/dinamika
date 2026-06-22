//! `dinamika` — umbrella crate: re-exports the raster renderer
//! [`dinamika_cpu`] (as [`cpu`]) and the animation library
//! [`dinamika_core`] (as [`core`]).

pub use dinamika_cpu as cpu;
pub use dinamika_core as core;

// Convenient flat access to the public animation API.
pub use dinamika_core::*;
