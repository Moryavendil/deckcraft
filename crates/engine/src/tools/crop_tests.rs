//! Crop mode through `picture.cropMode`, pointer gestures and keys, as the UI, CLI, control
//! channel and MCP drive it.

use deckcraft_model::{ShapeId, ShapeKind};
use serde_json::json;

use super::{Hit, Mods, PointerEvent, PointerKind, Xfrm};
use crate::Session;

/// A slide with a 200 × 100 pt picture at (100, 100).
fn picture() -> (Session, ShapeId) {
    let mut s = Session::with_new();
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    let png = deckcraft_render::Image { width: 4, height: 2, pixels: [255, 0, 0, 255].repeat(8) }.to_png();
    let id = s.execute("insert.picture", &json!({"data": crate::cmd::base64_encode(&png), "name": "red.png", "rect": [100, 100, 200, 100]})).unwrap()
        ["id"]
        .as_u64()
        .unwrap();
    (s, ShapeId(id as u32))
}

fn ev(kind: PointerKind, x: f64, y: f64) -> PointerEvent {
    PointerEvent { kind, x, y, mods: Mods::default(), tol: 3.0, pressure: None }
}

fn drag(s: &mut Session, from: (f64, f64), to: (f64, f64)) {
    s.pointer(ev(PointerKind::Down, from.0, from.1)).unwrap();
    s.pointer(ev(PointerKind::Drag, (from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, to.0, to.1)).unwrap();
    s.pointer(ev(PointerKind::Up, to.0, to.1)).unwrap();
}

fn state(s: &Session, id: ShapeId) -> (Xfrm, [f64; 4]) {
    let sh = s.doc().unwrap().shape(id).unwrap();
    let ShapeKind::Picture { fill } = &sh.kind else { panic!("not a picture") };
    (sh.xfrm.unwrap(), fill.crop)
}

fn cropping(s: &Session) -> Option<ShapeId> {
    s.tool.cropping(s.doc().unwrap())
}

fn close(a: [f64; 4], b: [f64; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6)
}

#[test]
fn crop_mode_toggles_and_its_handles_replace_the_box_handles() {
    let (mut s, id) = picture();
    assert_eq!(s.execute("picture.cropMode", &json!({})).unwrap()["cropping"], json!(id.0));
    assert_eq!(cropping(&s), Some(id));
    // The right edge's middle is a crop handle, not a resize handle.
    assert_eq!(s.hit_test(super::Point::new(300.0, 150.0), 3.0), Some(Hit::CropHandle { id, handle: 3 }));
    assert_eq!(s.hit_test(super::Point::new(150.0, 150.0), 3.0), Some(Hit::CropImage { id }));
    // Clicking Crop again leaves crop mode.
    s.execute("picture.cropMode", &json!({})).unwrap();
    assert_eq!(cropping(&s), None);
    assert_eq!(s.hit_test(super::Point::new(300.0, 150.0), 3.0), Some(Hit::Resize { id, handle: 3 }));
}

#[test]
fn dragging_a_crop_handle_crops_that_side_in_one_undo_step() {
    let (mut s, id) = picture();
    s.execute("picture.cropMode", &json!({})).unwrap();
    drag(&mut s, (300.0, 150.0), (250.0, 150.0));
    let (x, c) = state(&s, id);
    assert!(close(c, [0.0, 0.0, 0.25, 0.0]), "{c:?}");
    assert!((x.x - 100.0).abs() < 1e-6 && (x.w - 150.0).abs() < 1e-6, "{x:?}");
    // Still cropping after the drag: more sides can be cropped.
    assert_eq!(cropping(&s), Some(id));
    drag(&mut s, (100.0, 100.0), (110.0, 120.0));
    let (_, c) = state(&s, id);
    assert!(close(c, [0.05, 0.2, 0.25, 0.0]), "{c:?}");
    // Each drag is its own undo step.
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(close(state(&s, id).1, [0.0, 0.0, 0.25, 0.0]));
    s.execute("edit.undo", &json!({})).unwrap();
    let (x, c) = state(&s, id);
    assert!(close(c, [0.0; 4]) && (x.w - 200.0).abs() < 1e-6);
}

#[test]
fn dragging_the_picture_in_crop_mode_moves_the_image_not_the_frame() {
    let (mut s, id) = picture();
    s.execute("picture.crop", &json!({"left": 0.25, "right": 0.25})).unwrap();
    s.execute("picture.cropMode", &json!({})).unwrap();
    let before = state(&s, id).0;
    drag(&mut s, (200.0, 150.0), (220.0, 150.0));
    let (x, c) = state(&s, id);
    assert_eq!(x, before, "the frame stays put");
    // The image (400 pt wide on the slide) moved 20 pt right: 5% less cropped on the left.
    assert!(close(c, [0.2, 0.0, 0.3, 0.0]), "{c:?}");
    // The cropped-away part around the frame grabs the image too.
    assert_eq!(s.hit_test(super::Point::new(30.0, 150.0), 3.0), Some(Hit::CropImage { id }));
}

#[test]
fn escape_enter_a_click_elsewhere_or_a_double_click_end_crop_mode_and_keep_the_crop() {
    let (mut s, id) = picture();
    for end in ["escape", "enter", "click", "double"] {
        s.execute("edit.select", &json!({"ids": [id.0]})).unwrap();
        s.execute("picture.cropMode", &json!({"on": true})).unwrap();
        // Each case crops another 10 pt off the right, from where that edge now is.
        let right = state(&s, id).0.x + state(&s, id).0.w;
        drag(&mut s, (right, 150.0), (right - 10.0, 150.0));
        let crop = state(&s, id).1;
        assert!(crop[2] > 0.0, "{end}: {crop:?}");
        match end {
            "click" => {
                s.pointer(ev(PointerKind::Down, 600.0, 400.0)).unwrap();
                s.pointer(ev(PointerKind::Up, 600.0, 400.0)).unwrap();
            }
            "double" => {
                s.pointer(ev(PointerKind::Down, 150.0, 150.0)).unwrap();
                s.pointer(ev(PointerKind::Up, 150.0, 150.0)).unwrap();
                s.pointer(ev(PointerKind::Down, 150.0, 150.0)).unwrap();
                s.pointer(ev(PointerKind::DoubleClick, 150.0, 150.0)).unwrap();
                s.pointer(ev(PointerKind::Up, 150.0, 150.0)).unwrap();
            }
            k => {
                assert_eq!(s.key(k, Mods::default()).unwrap()["handled"], json!(true));
            }
        }
        assert_eq!(cropping(&s), None, "{end}");
        assert!(close(state(&s, id).1, crop), "{end}");
        assert!(s.tool.crop.is_none(), "{end}: crop mode doesn't come back on reselecting");
    }
}

#[test]
fn crop_mode_refuses_what_it_cannot_crop() {
    let (mut s, id) = picture();
    let rect = s.execute("shape.insert", &json!({"preset": "rect", "rect": [400, 300, 50, 50]})).unwrap()["id"].as_u64().unwrap();
    s.execute("edit.select", &json!({"ids": [rect]})).unwrap();
    assert!(s.execute("picture.cropMode", &json!({})).unwrap_err().to_string().contains("only a picture"));
    s.execute("edit.select", &json!({"ids": [id.0, rect]})).unwrap();
    assert!(s.execute("picture.cropMode", &json!({})).unwrap_err().to_string().contains("select one picture"));
    s.execute("edit.select", &json!({"ids": [id.0]})).unwrap();
    s.execute("shape.lock", &json!({"locked": true})).unwrap();
    assert!(s.execute("picture.cropMode", &json!({})).unwrap_err().to_string().contains("locked"));
    // Naming the picture selects it alone.
    s.execute("shape.lock", &json!({"locked": false})).unwrap();
    s.execute("edit.select", &json!({"ids": [rect]})).unwrap();
    s.execute("picture.cropMode", &json!({"id": id.0})).unwrap();
    assert_eq!(s.doc().unwrap().selection.shapes, vec![id]);
    assert_eq!(cropping(&s), Some(id));
}

#[test]
fn a_rotated_picture_crops_along_its_own_edges() {
    let (mut s, id) = picture();
    s.edit(|doc, sel| {
        if let Some(sh) = crate::shapes_mut(doc, sel).and_then(|l| deckcraft_model::find_shape_mut(l, id))
            && let Some(x) = sh.xfrm.as_mut()
        {
            x.rot = 90.0;
        }
        Ok(())
    })
    .unwrap();
    s.execute("picture.cropMode", &json!({})).unwrap();
    // Turned 90° about its centre (200, 150), the box's right edge middle is at (200, 250).
    assert_eq!(s.hit_test(super::Point::new(200.0, 250.0), 3.0), Some(Hit::CropHandle { id, handle: 3 }));
    drag(&mut s, (200.0, 250.0), (200.0, 200.0));
    assert!(close(state(&s, id).1, [0.0, 0.0, 0.25, 0.0]), "{:?}", state(&s, id).1);
}
