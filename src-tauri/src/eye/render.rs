//! Direct tiny-skia renderer for the Eye tray icon (prototype "variant D").
//!
//! One pure function, `render_frame`, turns an [`EyeFrame`] pose into an
//! opaque-black-on-transparent macOS template image. No SVG, no PNG assets:
//! the whole Eye is a ring + an iris + pupil (+ optional cracks / closed lid)
//! drawn parametrically so the animator can later sweep the iris + pupil.

use tauri::image::Image;
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform,
};

/// Physical pixel size of the rendered icon: 18pt logical rendered @2x retina.
const PX: u32 = 36;
/// The prototype geometry is authored in a 0..100 viewBox.
const VB: f32 = 100.0;
/// viewBox unit -> physical pixel (36 / 100 = 0.36).
const SCALE: f32 = PX as f32 / VB;

/// One renderable Eye pose.
///
/// `pupil` is an offset in viewBox (0..100) units measured from the eye centre
/// (50, 50); `(0.0, 0.0)` is the centred/static pose the static Eye ships with.
pub struct EyeFrame {
    pub pupil: (f64, f64),
    pub lid_closed: bool,
    pub shattered: bool,
}

impl EyeFrame {
    /// The static "open, centred, calm" pose shown before any animation runs.
    pub fn static_open() -> Self {
        Self {
            pupil: (0.0, 0.0),
            lid_closed: false,
            shattered: false,
        }
    }
}

/// viewBox unit -> physical pixel.
fn s(v: f32) -> f32 {
    v * SCALE
}

/// Render one Eye pose to an opaque-black-on-transparent template image.
///
/// tiny-skia's [`Pixmap::data`] is premultiplied RGBA8; for pure black
/// (`R = G = B = 0`) the premultiplied bytes equal the straight bytes, so the
/// buffer is handed straight to [`Image::new_owned`] with no conversion.
pub fn render_frame(frame: &EyeFrame) -> Image<'static> {
    let mut pm = Pixmap::new(PX, PX).expect("36x36 pixmap allocation");

    let mut paint = Paint::default();
    paint.set_color(Color::BLACK);
    paint.anti_alias = true;

    // A closed lid replaces the whole eye with a single horizontal line.
    if frame.lid_closed {
        draw_closed_lid(&mut pm, &paint);
        return finish(pm);
    }

    draw_ring(&mut pm, &paint);
    draw_iris_and_pupil(&mut pm, &paint, frame.pupil);
    if frame.shattered {
        draw_cracks(&mut pm, &paint);
    }

    finish(pm)
}

fn finish(pm: Pixmap) -> Image<'static> {
    Image::new_owned(pm.data().to_vec(), PX, PX)
}

/// Bold ring: circle centre (50, 50), r43, stroke-width 10 (viewBox units).
/// Sized to fill most of the 36px icon so the Eye reads large in the menubar.
fn draw_ring(pm: &mut Pixmap, paint: &Paint) {
    let ring = PathBuilder::from_circle(s(50.0), s(50.0), s(43.0)).expect("ring circle path");
    let stroke = Stroke {
        width: s(10.0),
        ..Default::default()
    };
    pm.stroke_path(&ring, paint, &stroke, Transform::identity(), None);
}

/// Iris + pupil (prototype "variant B"): a STROKED iris ring (radius 13,
/// stroke-width 3.5) around a FILLED pupil dot (radius 6), both centred on the
/// same `pupil` offset so they move as one unit. The iris keeps the old solid
/// pupil's radius 13, so the animator's SAFE_RADIUS / ring clamp is unchanged.
fn draw_iris_and_pupil(pm: &mut Pixmap, paint: &Paint, pupil: (f64, f64)) {
    let cx = s(50.0 + pupil.0 as f32);
    let cy = s(50.0 + pupil.1 as f32);

    // Iris: a stroked ring at radius 13 (the old solid pupil's outer extent).
    let iris = PathBuilder::from_circle(cx, cy, s(13.0)).expect("iris circle path");
    let stroke = Stroke {
        width: s(3.5),
        ..Default::default()
    };
    pm.stroke_path(&iris, paint, &stroke, Transform::identity(), None);

    // Pupil: a filled dot at the same centre.
    let pupil_dot = PathBuilder::from_circle(cx, cy, s(6.0)).expect("pupil circle path");
    pm.fill_path(
        &pupil_dot,
        paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );
}

/// Closed lid: horizontal line x18 -> x82 at y50, stroke-width 10, round caps.
fn draw_closed_lid(pm: &mut Pixmap, paint: &Paint) {
    let mut pb = PathBuilder::new();
    pb.move_to(s(18.0), s(50.0));
    pb.line_to(s(82.0), s(50.0));
    let Some(line) = pb.finish() else { return };
    let stroke = Stroke {
        width: s(10.0),
        line_cap: LineCap::Round,
        ..Default::default()
    };
    pm.stroke_path(&line, paint, &stroke, Transform::identity(), None);
}

/// Shattered cracks: three interior polylines, stroke-width 6, round joins/caps,
/// placed so the bold ring does not swallow them (legible at 18px).
fn draw_cracks(pm: &mut Pixmap, paint: &Paint) {
    const CRACKS: [[(f32, f32); 3]; 3] = [
        [(50.0, 22.0), (57.0, 36.0), (49.0, 48.0)],
        [(76.0, 66.0), (64.0, 58.0), (56.0, 50.0)],
        [(26.0, 70.0), (38.0, 60.0), (46.0, 54.0)],
    ];
    let stroke = Stroke {
        width: s(6.0),
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Default::default()
    };
    for crack in CRACKS {
        let mut pb = PathBuilder::new();
        pb.move_to(s(crack[0].0), s(crack[0].1));
        pb.line_to(s(crack[1].0), s(crack[1].1));
        pb.line_to(s(crack[2].0), s(crack[2].1));
        if let Some(path) = pb.finish() {
            pm.stroke_path(&path, paint, &stroke, Transform::identity(), None);
        }
    }
}
