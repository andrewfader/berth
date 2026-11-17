use tiny_skia::{
    Color, FillRule, FilterQuality, Paint, Path, PathBuilder, Pattern, Pixmap, PixmapMut, SpreadMode, Stroke,
    Transform,
};

use crate::geometry::Rect;

const ARC: f32 = 0.447_715_25;

pub fn rounded(rect: Rect, radius: f32) -> Option<Path> {
    let radius = radius.min(rect.width / 2.0).min(rect.height / 2.0).max(0.0);
    let (left, top, right, bottom) = (rect.x, rect.y, rect.right(), rect.bottom());
    let bend = radius * ARC;
    let mut path = PathBuilder::new();
    path.move_to(left + radius, top);
    path.line_to(right - radius, top);
    path.cubic_to(right - bend, top, right, top + bend, right, top + radius);
    path.line_to(right, bottom - radius);
    path.cubic_to(right, bottom - bend, right - bend, bottom, right - radius, bottom);
    path.line_to(left + radius, bottom);
    path.cubic_to(left + bend, bottom, left, bottom - bend, left, bottom - radius);
    path.line_to(left, top + radius);
    path.cubic_to(left, top + bend, left + bend, top, left + radius, top);
    path.close();
    path.finish()
}

pub fn tint(red: u8, green: u8, blue: u8, alpha: f32) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(
        red,
        green,
        blue,
        (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
    ));
    paint.anti_alias = true;
    paint
}

pub fn fill_rounded(canvas: &mut PixmapMut, rect: Rect, radius: f32, paint: &Paint, scale: f32) {
    if let Some(path) = rounded(rect, radius) {
        canvas.fill_path(
            &path,
            paint,
            FillRule::Winding,
            Transform::from_scale(scale, scale),
            None,
        );
    }
}

pub fn stroke_rounded(
    canvas: &mut PixmapMut,
    rect: Rect,
    radius: f32,
    width: f32,
    paint: &Paint,
    scale: f32,
) {
    if let Some(path) = rounded(rect, radius) {
        let stroke = Stroke {
            width,
            ..Stroke::default()
        };
        canvas.stroke_path(&path, paint, &stroke, Transform::from_scale(scale, scale), None);
    }
}

pub fn picture(canvas: &mut PixmapMut, image: &Pixmap, into: Rect, radius: f32, opacity: f32, scale: f32) {
    let stretch = Transform::from_translate(into.x, into.y).pre_scale(
        into.width / image.width() as f32,
        into.height / image.height() as f32,
    );
    let paint = Paint {
        shader: Pattern::new(
            image.as_ref(),
            SpreadMode::Pad,
            FilterQuality::Bilinear,
            opacity,
            stretch,
        ),
        anti_alias: true,
        ..Paint::default()
    };
    fill_rounded(canvas, into, radius, &paint, scale);
}

pub fn rounded_rows(rect: Rect, radius: f32) -> Vec<(i32, i32, i32, i32)> {
    let (left, top) = (rect.x.round() as i32, rect.y.round() as i32);
    let (width, height) = (rect.width.round() as i32, rect.height.round() as i32);
    let radius = (radius.round() as i32).min(width / 2).min(height / 2).max(0);
    let mut rows = Vec::with_capacity(2 * radius as usize + 1);
    for row in 0..radius {
        let rise = radius as f32 - row as f32 - 0.5;
        let inset = (radius as f32 - (radius.pow(2) as f32 - rise * rise).sqrt()).ceil() as i32;
        rows.push((left + inset, top + row, width - 2 * inset, 1));
        rows.push((left + inset, top + height - 1 - row, width - 2 * inset, 1));
    }
    if height > 2 * radius {
        rows.push((left, top + radius, width, height - 2 * radius));
    }
    rows
}

#[cfg(test)]
pub mod probes {
    use tiny_skia::Pixmap;

    pub fn rgba(pixmap: &Pixmap, x: f32, y: f32) -> [u8; 4] {
        let pixel = pixmap.pixel(x as u32, y as u32).unwrap().demultiply();
        [pixel.red(), pixel.green(), pixel.blue(), pixel.alpha()]
    }

    pub fn alpha(pixmap: &Pixmap, x: f32, y: f32) -> u8 {
        pixmap.pixel(x as u32, y as u32).unwrap().alpha()
    }

    pub fn solid(size: u32, red: u8, green: u8, blue: u8) -> Pixmap {
        let mut pixmap = Pixmap::new(size, size).unwrap();
        pixmap.fill(tiny_skia::Color::from_rgba8(red, green, blue, 255));
        pixmap
    }
}

#[cfg(test)]
mod tests {
    use super::probes::*;
    use super::*;

    #[test]
    fn a_rounded_fill_covers_the_middle_and_leaves_the_corners_clear() {
        let mut pixmap = Pixmap::new(100, 60).unwrap();

        fill_rounded(
            &mut pixmap.as_mut(),
            Rect::new(0.0, 0.0, 100.0, 60.0),
            20.0,
            &tint(255, 255, 255, 1.0),
            1.0,
        );

        assert_eq!(alpha(&pixmap, 50.0, 30.0), 255);
        assert_eq!(alpha(&pixmap, 50.0, 0.0), 255);
        assert_eq!(alpha(&pixmap, 1.0, 1.0), 0);
        assert_eq!(alpha(&pixmap, 98.0, 58.0), 0);
    }

    #[test]
    fn a_picture_is_stretched_into_its_frame_at_the_canvas_scale() {
        let mut pixmap = Pixmap::new(200, 200).unwrap();

        picture(
            &mut pixmap.as_mut(),
            &solid(16, 255, 0, 0),
            Rect::new(10.0, 10.0, 50.0, 50.0),
            0.0,
            1.0,
            2.0,
        );

        assert_eq!(rgba(&pixmap, 21.0, 21.0), [255, 0, 0, 255]);
        assert_eq!(rgba(&pixmap, 118.0, 118.0), [255, 0, 0, 255]);
        assert_eq!(alpha(&pixmap, 18.0, 18.0), 0);
        assert_eq!(alpha(&pixmap, 122.0, 122.0), 0);
    }

    #[test]
    fn rows_approximating_a_rounded_shape_stay_inside_it_and_cover_its_body() {
        let rows = rounded_rows(Rect::new(10.0, 20.0, 100.0, 40.0), 12.0);

        let area: i32 = rows.iter().map(|(_, _, width, height)| width * height).sum();
        assert!(
            rows.iter()
                .all(|(x, y, width, height)| *x >= 10 && x + width <= 110 && *y >= 20 && y + height <= 60)
        );
        assert!(rows.contains(&(10, 32, 100, 16)));
        assert!(area > 3800 && area < 4000, "{area}");
        assert!(rows.iter().any(|(x, y, _, _)| *y == 20 && *x > 10));
    }

    #[test]
    fn a_square_cornered_shape_is_a_single_row() {
        assert_eq!(
            rounded_rows(Rect::new(0.0, 0.0, 30.0, 10.0), 0.0),
            [(0, 0, 30, 10)]
        );
    }
}
