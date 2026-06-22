//! Saving timeline frames to disk.
//!
//! Frames are written as PNG with numeric names (`000001.png`, `000002.png`, …)
//! into the output directory. The directory can be set explicitly via
//! [`Timeline::render`], or saved to `outputs/<scene>` next to the scene's
//! `.rs` file with a single call of the [`render!`](crate::render) macro.

use std::io;
use std::path::{Path, PathBuf};

use crate::timeline::Timeline;

/// Frame names: `000001.png`. The width is widened beyond this if needed.
const FRAME_DIGITS: usize = 6;

impl Timeline {
    /// Renders the whole animation and saves the PNG frames into the `dir` directory.
    ///
    /// The directory (and its parents) is created if needed. Frames are numbered
    /// from one: `000001.png`, `000002.png`, … Returns the number of frames
    /// written. The frame size, background and frame rate are taken from
    /// [`Timeline::new`].
    ///
    /// To save into `outputs/<scene>` next to the scene file, it is more
    /// convenient to call the [`render!`](crate::render) macro — it fills in the
    /// directory itself:
    ///
    /// ```no_run
    /// # use dinamika_core::*;
    /// let tl = Timeline::new(480, 240, Color::from_rgba8(20, 20, 24, 255), 30.0);
    /// // Next to this .rs file: outputs/demo/000001.png …
    /// render!(tl, "demo").unwrap();
    /// // Or a directory of your own directly:
    /// tl.render("D:/render/demo").unwrap();
    /// ```
    pub fn render(&self, dir: impl AsRef<Path>) -> io::Result<usize> {
        let dir = dir.as_ref();
        std::fs::create_dir_all(dir)?;

        let frames = self.frames();
        for (i, frame) in frames.iter().enumerate() {
            let name = format!("{:0width$}.png", i + 1, width = FRAME_DIGITS);
            frame.save_png(dir.join(name))?;
        }
        Ok(frames.len())
    }
}

/// Default output directory for a scene: `<source-dir>/outputs/<scene>`.
///
/// Usually called not directly, but via the [`scene_dir!`](crate::scene_dir)
/// macro, which substitutes `env!("CARGO_MANIFEST_DIR")` and `file!()` at the
/// call site.
pub fn scene_output_dir(manifest_dir: &str, source_file: &str, scene: &str) -> PathBuf {
    resolve_source_dir(manifest_dir, source_file).join("outputs").join(scene)
}

/// Reconstructs the absolute directory of the source file from `file!()` and
/// `CARGO_MANIFEST_DIR`.
///
/// Depending on the Cargo version and build method, `file!()` can be different:
/// absolute, relative to the package (`src/main.rs`) or to the workspace root
/// (`dinamika/src/main.rs`). We try the candidates and take the first existing
/// one, otherwise the most likely one.
fn resolve_source_dir(manifest_dir: &str, source_file: &str) -> PathBuf {
    let file = Path::new(source_file);

    let candidates: Vec<PathBuf> = if file.is_absolute() {
        vec![file.to_path_buf()]
    } else {
        let manifest = Path::new(manifest_dir);
        let mut v = vec![manifest.join(file)];
        if let Some(workspace) = manifest.parent() {
            v.push(workspace.join(file));
        }
        v.push(file.to_path_buf()); // relative to the current directory
        v
    };

    let resolved = candidates
        .iter()
        .find(|p| p.exists())
        .cloned()
        .unwrap_or_else(|| candidates[0].clone());

    resolved.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."))
}

/// Default output directory for a scene — `outputs/<scene>` next to the
/// `.rs` file where the macro is called.
///
/// Expands into a [`PathBuf`] that can be passed to [`Timeline::render`]:
///
/// ```no_run
/// # use dinamika_core::*;
/// # let tl = Timeline::new(480, 240, Color::BLACK, 30.0);
/// tl.render(scene_dir!("demo")).unwrap();
/// ```
///
/// To pick the directory and render in a single call, see [`render!`](crate::render).
#[macro_export]
macro_rules! scene_dir {
    ($scene:expr $(,)?) => {
        $crate::scene_output_dir(env!("CARGO_MANIFEST_DIR"), file!(), $scene)
    };
}

/// Renders the whole timeline animation into the `outputs/<scene>` directory
/// next to the `.rs` file where the macro is called; returns a
/// [`std::io::Result`] carrying only success/failure. The number of saved
/// frames is not returned — if you need it, call [`Timeline::render`] directly.
///
/// A convenient wrapper over [`Timeline::render`] and
/// [`scene_dir!`](crate::scene_dir): the directory is chosen automatically, so a
/// separate path macro call is not needed.
///
/// ```no_run
/// # use dinamika_core::*;
/// let tl = Timeline::new(480, 240, Color::BLACK, 30.0);
/// render!(tl, "demo").unwrap(); // → outputs/demo next to the .rs
/// ```
#[macro_export]
macro_rules! render {
    ($timeline:expr, $scene:expr $(,)?) => {
        $timeline.render($crate::scene_dir!($scene)).map(|_| ())
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_dir_sits_next_to_source() {
        // Non-existent paths: we test the pure joining logic (fallback to the
        // first candidate — `manifest/source`).
        let dir = scene_output_dir("/proj/crate", "src/main.rs", "demo");
        assert!(dir.ends_with("outputs/demo") || dir.ends_with("outputs\\demo"));
        // The source directory is src, next to outputs.
        assert!(dir.to_string_lossy().contains("src"));
    }

    #[test]
    fn absolute_source_file_is_used_directly() {
        let dir = scene_output_dir("/ignored", "/abs/scenes/intro.rs", "intro");
        let s = dir.to_string_lossy().replace('\\', "/");
        assert_eq!(s, "/abs/scenes/outputs/intro");
    }
}
