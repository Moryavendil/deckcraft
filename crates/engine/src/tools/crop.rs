//! Crop mode (Picture Format › Crop): the picture's frame gets crop handles, and the whole image
//! shows around it. Dragging a crop handle moves that side of the frame over the image (inward
//! crops, outward uncrops or pads); dragging the picture moves the image under a fixed frame.
//!
//! Everything here works in the picture's own coordinates (its unrotated, unflipped box), where
//! the renderer draws the image at [`deckcraft_render::crop_dest`]: rotation and flips then apply
//! to frame and image alike, so the same math serves every picture.

use super::{Point, Rect, Vec2, Xfrm};

/// The smallest frame side a crop drag leaves, in points.
const MIN_SIDE: f64 = 1.0;
/// How far a side may be cropped (fraction of the image), as `picture.crop` allows.
const MAX_CROP: f64 = 0.95;
/// How far a side may be padded outward (fraction of the image).
const MIN_CROP: f64 = -10.0;

/// Where the whole image lies in the picture's own coordinates, for a `w` × `h` frame cropped by
/// `crop` (left, top, right, bottom fractions).
pub fn image_rect(w: f64, h: f64, crop: [f64; 4]) -> Rect {
    deckcraft_render::crop_dest(Rect::new(0.0, 0.0, w, h), crop)
}

/// The crop that shows `frame` (in the picture's own coordinates) of an image lying at `img`.
fn crop_of(frame: Rect, img: Rect) -> Option<[f64; 4]> {
    let (iw, ih) = (img.width(), img.height());
    if !(iw > 1e-9 && ih > 1e-9) {
        return None;
    }
    let c = [(frame.x0 - img.x0) / iw, (frame.y0 - img.y0) / ih, (img.x1 - frame.x1) / iw, (img.y1 - frame.y1) / ih];
    c.iter().all(|v| v.is_finite()).then_some(c)
}

/// Which sides crop handle `handle` (0..8: NW, N, NE, E, SE, S, SW, W) moves: left, top, right,
/// bottom.
fn sides(handle: u8) -> [bool; 4] {
    match handle {
        0 => [true, true, false, false],
        1 => [false, true, false, false],
        2 => [false, true, true, false],
        3 => [false, false, true, false],
        4 => [false, false, true, true],
        5 => [false, false, false, true],
        6 => [true, false, false, true],
        _ => [true, false, false, false],
    }
}

/// `v` pulled onto `target` when it is within `snap` of it.
fn snap_to(v: f64, target: f64, snap: f64) -> f64 {
    if (v - target).abs() <= snap { target } else { v }
}

/// Drag crop handle `handle` of a picture whose box was `orig` and crop `crop` at the press, by
/// `d` in the picture's own coordinates. A side snaps to the image's edge within `snap` points, so
/// a crop can be taken back exactly. Returns the new box (in the box's parent space) and crop.
pub fn drag_edge(orig: &Xfrm, crop: [f64; 4], handle: u8, d: Vec2, snap: f64) -> Option<(Xfrm, [f64; 4])> {
    if !(orig.w > 0.0 && orig.h > 0.0 && d.x.is_finite() && d.y.is_finite()) {
        return None;
    }
    let img = image_rect(orig.w, orig.h, crop);
    let [ml, mt, mr, mb] = sides(handle);
    let (iw, ih) = (img.width(), img.height());
    // Each side stays where `picture.crop` can store it, and the frame keeps a minimum size.
    let mut x0 = 0.0;
    let mut y0 = 0.0;
    let mut x1 = orig.w;
    let mut y1 = orig.h;
    if ml {
        x0 = snap_to(d.x, img.x0, snap).clamp(img.x0 + MIN_CROP * iw, img.x0 + MAX_CROP * iw).min(x1 - MIN_SIDE);
    }
    if mr {
        x1 = snap_to(orig.w + d.x, img.x1, snap).clamp(img.x1 - MAX_CROP * iw, img.x1 - MIN_CROP * iw).max(x0 + MIN_SIDE);
    }
    if mt {
        y0 = snap_to(d.y, img.y0, snap).clamp(img.y0 + MIN_CROP * ih, img.y0 + MAX_CROP * ih).min(y1 - MIN_SIDE);
    }
    if mb {
        y1 = snap_to(orig.h + d.y, img.y1, snap).clamp(img.y1 - MAX_CROP * ih, img.y1 - MIN_CROP * ih).max(y0 + MIN_SIDE);
    }
    let frame = Rect::new(x0, y0, x1, y1);
    let c = crop_of(frame, img)?;
    // The new box has the same rotation and flips, centred where the new frame's centre lands.
    let centre = orig.affine() * Point::new((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (w, h) = (x1 - x0, y1 - y0);
    let x = Xfrm { x: centre.x - w / 2.0, y: centre.y - h / 2.0, w, h, ..*orig };
    (x.x.is_finite() && x.y.is_finite()).then_some((x, c))
}

/// Move the image of a picture cropped by `crop` (box `w` × `h`) by `d` in the picture's own
/// coordinates, under its fixed frame. An image edge snaps to the frame's edge within `snap`
/// points. Returns the new crop.
pub fn pan(w: f64, h: f64, crop: [f64; 4], d: Vec2, snap: f64) -> Option<[f64; 4]> {
    if !(w > 0.0 && h > 0.0 && d.x.is_finite() && d.y.is_finite()) {
        return None;
    }
    let img = image_rect(w, h, crop);
    let (iw, ih) = (img.width(), img.height());
    let shift = |d: f64, lo_crop: f64, hi_crop: f64, size: f64, frame: f64, lo_edge: f64| {
        // Snap the leading or trailing image edge onto the frame edge.
        let mut d = d;
        let lead = lo_edge + d;
        if (lead - 0.0).abs() <= snap {
            d = -lo_edge;
        } else if (lead + size - frame).abs() <= snap {
            d = frame - size - lo_edge;
        }
        // Keep both sides' crops in range: moving the image by `d` takes `d / size` off the
        // near side's crop and adds it to the far side's.
        let min_d = ((lo_crop - MAX_CROP) * size).max(-(hi_crop - MIN_CROP) * size);
        let max_d = ((lo_crop - MIN_CROP) * size).min((MAX_CROP - hi_crop) * size);
        if min_d <= max_d { d.clamp(min_d, max_d) } else { 0.0 }
    };
    let dx = shift(d.x, crop[0], crop[2], iw, w, img.x0);
    let dy = shift(d.y, crop[1], crop[3], ih, h, img.y0);
    let moved = Rect::new(img.x0 + dx, img.y0 + dy, img.x1 + dx, img.y1 + dy);
    crop_of(Rect::new(0.0, 0.0, w, h), moved)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f64; 4], b: [f64; 4]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9)
    }

    #[test]
    fn dragging_the_right_handle_inward_crops_the_right_side() {
        let o = Xfrm::new(100.0, 100.0, 200.0, 100.0);
        let (x, c) = drag_edge(&o, [0.0; 4], 3, Vec2::new(-50.0, 0.0), 0.0).unwrap_or_default();
        assert!(close(c, [0.0, 0.0, 0.25, 0.0]), "{c:?}");
        assert!((x.x - 100.0).abs() < 1e-9 && (x.w - 150.0).abs() < 1e-9 && (x.h - 100.0).abs() < 1e-9, "{x:?}");
    }

    #[test]
    fn dragging_the_top_left_corner_crops_two_sides_and_keeps_the_image_in_place() {
        let o = Xfrm::new(0.0, 0.0, 100.0, 100.0);
        let (x, c) = drag_edge(&o, [0.0; 4], 0, Vec2::new(10.0, 20.0), 0.0).unwrap_or_default();
        assert!(close(c, [0.1, 0.2, 0.0, 0.0]), "{c:?}");
        assert!((x.x - 10.0).abs() < 1e-9 && (x.y - 20.0).abs() < 1e-9 && (x.w - 90.0).abs() < 1e-9 && (x.h - 80.0).abs() < 1e-9);
        // The image itself has not moved on the slide.
        let img = image_rect(x.w, x.h, c);
        assert!((x.x + img.x0).abs() < 1e-9 && (x.y + img.y0).abs() < 1e-9 && (img.width() - 100.0).abs() < 1e-9);
    }

    #[test]
    fn dragging_outward_uncrops_and_snaps_onto_the_image_edge() {
        let o = Xfrm::new(0.0, 0.0, 80.0, 100.0);
        // Cropped 20% on the left: the image starts 20 pt left of the frame.
        let crop = [0.2, 0.0, 0.0, 0.0];
        let (_, c) = drag_edge(&o, crop, 7, Vec2::new(-19.0, 0.0), 2.0).unwrap_or_default();
        assert!(close(c, [0.0; 4]), "snaps exactly back to uncropped: {c:?}");
        // Past the edge it pads (a negative crop), as PowerPoint does.
        let (x, c) = drag_edge(&o, crop, 7, Vec2::new(-40.0, 0.0), 2.0).unwrap_or_default();
        assert!(c[0] < 0.0 && (x.w - 120.0).abs() < 1e-9, "{c:?} {x:?}");
    }

    #[test]
    fn a_crop_drag_never_flips_or_collapses_the_frame() {
        let o = Xfrm::new(0.0, 0.0, 100.0, 100.0);
        let (x, c) = drag_edge(&o, [0.0; 4], 3, Vec2::new(-500.0, 0.0), 0.0).unwrap_or_default();
        assert!(x.w >= 1.0 - 1e-9 && !x.flip_h && c[2] <= MAX_CROP + 1e-9, "{x:?} {c:?}");
        assert!(drag_edge(&o, [0.0; 4], 3, Vec2::new(f64::NAN, 0.0), 0.0).is_none());
        assert!(drag_edge(&Xfrm::new(0.0, 0.0, 0.0, 10.0), [0.0; 4], 3, Vec2::new(1.0, 0.0), 0.0).is_none());
    }

    #[test]
    fn a_rotated_picture_crops_along_its_own_edge() {
        // Rotated 90°: the box's right side points down on the slide.
        let o = Xfrm { x: 0.0, y: 0.0, w: 200.0, h: 100.0, rot: 90.0, ..Default::default() };
        let (x, c) = drag_edge(&o, [0.0; 4], 3, Vec2::new(-100.0, 0.0), 0.0).unwrap_or_default();
        assert!(close(c, [0.0, 0.0, 0.5, 0.0]), "{c:?}");
        // The left side (now the top on the slide) stays put.
        let keep = |x: &Xfrm| x.affine() * Point::new(0.0, 0.0);
        assert!((keep(&o) - keep(&x)).hypot() < 1e-6, "{:?} {:?}", keep(&o), keep(&x));
    }

    #[test]
    fn panning_moves_the_image_under_the_frame() {
        // A 100 pt frame showing the middle half of a 200 pt image.
        let crop = [0.25, 0.0, 0.25, 0.0];
        let c = pan(100.0, 100.0, crop, Vec2::new(20.0, 0.0), 0.0).unwrap_or_default();
        assert!(close(c, [0.15, 0.0, 0.35, 0.0]), "{c:?}");
        // Near the frame edge the image edge snaps onto it.
        let c = pan(100.0, 100.0, crop, Vec2::new(48.0, 0.0), 3.0).unwrap_or_default();
        assert!(close(c, [0.0, 0.0, 0.5, 0.0]), "{c:?}");
        // Far past it, the crop stays within what can be stored.
        let c = pan(100.0, 100.0, crop, Vec2::new(1e6, 0.0), 0.0).unwrap_or_default();
        assert!(c.iter().all(|v| (MIN_CROP - 1e-9..=MAX_CROP + 1e-9).contains(v)), "{c:?}");
        assert!(pan(100.0, 100.0, crop, Vec2::new(f64::INFINITY, 0.0), 0.0).is_none());
    }
}
