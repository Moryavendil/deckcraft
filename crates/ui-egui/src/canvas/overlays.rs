use super::{Color32, CornerRadius, Point, Pos2, Rect, SlideApp, Stroke, Tokens, ToolKind, Ui, Xf, Xfrm, box_handles, pos2, theme};
use deckcraft_engine::DocState;

fn poly(p: &egui::Painter, pts: [Pos2; 4], stroke: Stroke) {
    for i in 0..4 {
        p.line_segment([pts[i], pts[(i + 1) % 4]], stroke);
    }
}

fn selection_handles(app: &SlideApp, st: &DocState, painter: &egui::Painter, xf: Xf, t: &Tokens) {
    let sel_stroke = Stroke::new(1.0, t.selection);
    let editing = st.selection.text.clone();
    for id in &st.selection.shapes {
        let Some(sh) = st.shape(*id) else { continue };
        let x = deckcraft_engine::cmd::xfrm_of(&st.doc, &st.selection, sh);
        let a = x.affine();
        let corners = [Point::new(0.0, 0.0), Point::new(x.w, 0.0), Point::new(x.w, x.h), Point::new(0.0, x.h)].map(|q| xf.to_screen(a * q));
        if sh.is_line() {
            let (p0, p1) = (xf.to_screen(a * Point::new(0.0, 0.0)), xf.to_screen(a * Point::new(x.w, x.h)));
            let glued = match sh.kind {
                deckcraft_model::ShapeKind::Connector { start, end } => [start.is_some(), end.is_some()],
                _ => [false, false],
            };
            for (q, g) in [p0, p1].into_iter().zip(glued) {
                // Glued ends are filled with the accent, like attached connector ends.
                let fill = if g { t.accent } else { t.handle_fill };
                painter.circle(q, 4.5, fill, Stroke::new(1.0, if g { t.handle_fill } else { t.handle_stroke }));
            }
            continue;
        }
        // Crop mode draws the picture's crop handles instead ([`crop_overlay`]).
        if app.session.tool.cropping(st) == Some(sh.id) {
            continue;
        }
        let editing_this = editing.as_ref().is_some_and(|e| e.shape == *id && !e.notes);
        if app.session.tool.editing_points(st) == Some(sh.id) {
            // Edit Points: the outline's vertices are the handles.
            // No selection box: the outline and its vertices are what's edited.
            if let Some((path, index)) = app.session.tool.edit_vertex
                && let Some((hin, hout, v)) = deckcraft_engine::cmd::points::slide_handles(&st.doc, &st.selection, sh, path, index)
            {
                for h in [hin, hout].into_iter().flatten() {
                    painter.line_segment([xf.to_screen(v), xf.to_screen(h)], sel_stroke);
                    painter.rect(
                        egui::Rect::from_center_size(xf.to_screen(h), egui::vec2(6.0, 6.0)),
                        0.0,
                        Color32::WHITE,
                        Stroke::new(1.0, t.selection),
                        egui::StrokeKind::Middle,
                    );
                }
            }
            for q in deckcraft_engine::cmd::points::slide_vertices(&st.doc, &st.selection, sh).into_iter().flatten() {
                let c = xf.to_screen(q);
                painter.rect(
                    egui::Rect::from_center_size(c, egui::vec2(7.0, 7.0)),
                    0.0,
                    Color32::BLACK,
                    Stroke::new(1.0, Color32::WHITE),
                    egui::StrokeKind::Middle,
                );
            }
            continue;
        }
        if editing_this {
            // Dashed box while editing text.
            for i in 0..4 {
                let (s, e) = (corners[i], corners[(i + 1) % 4]);
                painter.add(egui::Shape::dashed_line(&[s, e], Stroke::new(1.0, t.selection), 4.0, 3.0));
            }
        } else {
            poly(painter, corners, sel_stroke);
        }
        if sh.locked && !editing_this {
            // Locked: greyed handles that don't respond, no rotate or adjust handles.
            for h in box_handles(&x).0 {
                painter.circle(xf.to_screen(h), 4.0, t.text_faint, Stroke::new(1.0, t.handle_fill));
            }
            continue;
        }
        if st.selection.shapes.len() == 1 || !editing_this {
            let (hs, rot) = box_handles(&x);
            let rot_s = xf.to_screen(rot);
            if !editing_this && sh.ph.as_ref().is_none_or(|_| true) {
                let top_mid = xf.to_screen(hs[1]);
                painter.line_segment([top_mid, rot_s], sel_stroke);
                rotate_handle(painter, rot_s, t);
            }
            for h in hs {
                painter.circle(xf.to_screen(h), 4.5, t.handle_fill, Stroke::new(1.0, t.handle_stroke));
            }
            let geo = deckcraft_render::shape_geometry(sh, x.w, x.h);
            for hd in &geo.handles {
                let q = xf.to_screen(a * hd.pos);
                painter.circle(q, 4.5, Color32::from_rgb(0xFF, 0xD2, 0x3F), Stroke::new(1.0, Color32::from_rgb(0x8A, 0x6A, 0x00)));
            }
            for hd in deckcraft_engine::tools::wordart_handles(&st.doc, &st.selection, sh, x.w, x.h) {
                let q = xf.to_screen(a * hd.pos);
                painter.circle(q, 4.5, Color32::from_rgb(0xFF, 0xD2, 0x3F), Stroke::new(1.0, Color32::from_rgb(0x8A, 0x6A, 0x00)));
            }
        }
    }
}

/// How opaque the cropped-away part of the image is in crop mode. It is see-through rather than
/// greyed, so it reads the same over a light or a dark slide and the slide shows through it.
const CROP_GHOST_ALPHA: u8 = 110;

/// The whole image of the picture or video in crop mode: a picture's image; a video's current frame
/// while it plays, else its poster frame. `None` when there is none to show.
fn crop_ghost(
    textures: &mut crate::textures::Textures,
    media: &crate::media::MediaHost,
    st: &DocState,
    id: deckcraft_model::ShapeId,
    ctx: &egui::Context,
) -> Option<egui::TextureHandle> {
    let mut decoded = |m: deckcraft_model::MediaId| st.doc.media(m).and_then(|item| textures.crop_ghost(ctx, &item.data));
    match &st.shape(id)?.kind {
        deckcraft_model::ShapeKind::Picture { fill } => decoded(fill.media),
        deckcraft_model::ShapeKind::Media(m) => media.texture(id).cloned().or_else(|| m.poster.and_then(decoded)),
        _ => None,
    }
}

/// Crop mode: the cropped-away part of the image (`ghost`), see-through, around the frame (the
/// frame itself shows the cropped picture or video as rendered), the image's outline, and the crop
/// handles: black bars at the frame's corners and edge middles, outlined in white so they show on
/// any picture.
fn crop_overlay(st: &DocState, id: deckcraft_model::ShapeId, ghost: Option<&egui::TextureHandle>, painter: &egui::Painter, xf: Xf, t: &Tokens) {
    let Some(sh) = st.shape(id) else { return };
    let Some(crop) = deckcraft_engine::tools::shape_crop(sh) else { return };
    let x = deckcraft_engine::cmd::xfrm_of(&st.doc, &st.selection, sh);
    let a = x.affine();
    let img = deckcraft_engine::tools::crop::image_rect(x.w, x.h, crop);
    let to_screen = |px: f64, py: f64| xf.to_screen(a * Point::new(px, py));
    // The cropped-away bands: the image outside the frame, each piece a quad in the picture's
    // own (possibly rotated, flipped) coordinates.
    if let Some(tex) = ghost
        && img.width() > 1e-9
        && img.height() > 1e-9
    {
        let frame = deckcraft_geom::Rect::new(0.0, 0.0, x.w, x.h);
        let (fy0, fy1) = (frame.y0.clamp(img.y0, img.y1), frame.y1.clamp(img.y0, img.y1));
        let pieces = [
            (img.x0, img.y0, img.x1, fy0),
            (img.x0, fy1, img.x1, img.y1),
            (img.x0, fy0, frame.x0.clamp(img.x0, img.x1), fy1),
            (frame.x1.clamp(img.x0, img.x1), fy0, img.x1, fy1),
        ];
        let mut mesh = egui::Mesh::with_texture(tex.id());
        let tint = Color32::from_white_alpha(CROP_GHOST_ALPHA);
        for (x0, y0, x1, y1) in pieces {
            if x1 - x0 <= 1e-6 || y1 - y0 <= 1e-6 {
                continue;
            }
            let uv = |px: f64, py: f64| pos2(((px - img.x0) / img.width()) as f32, ((py - img.y0) / img.height()) as f32);
            let base = mesh.vertices.len() as u32;
            for (px, py) in [(x0, y0), (x1, y0), (x1, y1), (x0, y1)] {
                mesh.vertices.push(egui::epaint::Vertex { pos: to_screen(px, py), uv: uv(px, py), color: tint });
            }
            mesh.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        painter.add(egui::Shape::mesh(mesh));
    }
    // The whole image's outline, dashed, and the frame's.
    let outline =
        |x0: f64, y0: f64, x1: f64, y1: f64| [to_screen(x0, y0), to_screen(x1, y0), to_screen(x1, y1), to_screen(x0, y1), to_screen(x0, y0)];
    painter.add(egui::Shape::dashed_line(&outline(img.x0, img.y0, img.x1, img.y1), Stroke::new(1.0, t.selection), 4.0, 3.0));
    poly(painter, [to_screen(0.0, 0.0), to_screen(x.w, 0.0), to_screen(x.w, x.h), to_screen(0.0, x.h)], Stroke::new(1.0, t.selection));
    // The crop handles. Along the picture's own axes on screen, so they turn and flip with it.
    let o = to_screen(0.0, 0.0);
    let ex = (to_screen(1.0, 0.0) - o).normalized();
    let ey = (to_screen(0.0, 1.0) - o).normalized();
    let (arm, half) = (11.0, 7.0);
    let marks: [(f64, f64); 8] = [(0.0, 0.0), (0.5, 0.0), (1.0, 0.0), (1.0, 0.5), (1.0, 1.0), (0.5, 1.0), (0.0, 1.0), (0.0, 0.5)];
    for (fx, fy) in marks {
        let c = to_screen(fx * x.w, fy * x.h);
        // Corner marks run inward along both edges; edge marks lie along their edge.
        let inward = |f: f64| if f == 0.0 { 1.0 } else { -1.0 };
        let pts = match (fx == 0.5, fy == 0.5) {
            (true, _) => vec![c - ex * half, c + ex * half],
            (_, true) => vec![c - ey * half, c + ey * half],
            _ => vec![c + ex * arm * inward(fx), c, c + ey * arm * inward(fy)],
        };
        painter.add(egui::Shape::line(pts.clone(), Stroke::new(6.0, Color32::WHITE)));
        painter.add(egui::Shape::line(pts, Stroke::new(4.0, Color32::BLACK)));
    }
}

fn text_overlays(st: &DocState, ui: &Ui, painter: &egui::Painter, xf: Xf, t: &Tokens) {
    let editing = st.selection.text.clone();
    // Caret / text selection, in a shape or a table cell.
    if let Some(ts) = &editing {
        for quad in text_marks(st, ts).iter().flat_map(|m| &m.selection) {
            painter.add(egui::Shape::convex_polygon(quad.iter().map(|q| xf.to_screen(*q)).collect(), t.text_selection, Stroke::NONE));
        }
        // An input method's composition: underlined in place, thicker under the part being converted.
        for (a, b, thick) in deckcraft_engine::cmd::text::composition_underlines(st) {
            painter.line_segment([xf.to_screen(a), xf.to_screen(b)], Stroke::new(if thick { 2.0 } else { 1.0 }, t.caret));
        }
        if !ts.is_range() {
            let blink = (ui.input(|i| i.time) * 1.6).fract() < 0.6;
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
            // An empty placeholder shows the caret where typing begins too.
            if blink && let Some((top, bot)) = deckcraft_engine::cmd::text::caret_line(st, ts) {
                // In the colour of the text being typed: a black caret is lost on a dark slide.
                let color = deckcraft_engine::cmd::text::caret_color(st, ts).map_or(t.caret, theme::to_color32);
                painter.line_segment([xf.to_screen(top), xf.to_screen(bot)], Stroke::new(1.5, color));
            }
        }
    }
    // Table cell editing: outline the cell (merged cells as drawn).
    if let Some(ts) = &editing
        && let (Some((r, c)), Some(sh)) = (ts.cell, st.shape(ts.shape))
        && let Some(f) = deckcraft_engine::cmd::table::table_frame(st, sh)
        && let Some(cr) = f.cell_rect(r, c)
    {
        let pts = [Point::new(cr.x0, cr.y0), Point::new(cr.x1, cr.y0), Point::new(cr.x1, cr.y1), Point::new(cr.x0, cr.y1)]
            .map(|q| xf.to_screen(f.affine * q));
        poly(painter, pts, Stroke::new(2.0, t.accent));
    }
    // Shade the selected table cells in the text-selection color, each merged cell as it is drawn.
    if let Some((id, ..)) = st.selection.cells
        && st.selection.text.is_none()
        && let Some(sh) = st.shape(id)
        && let deckcraft_model::ShapeKind::Table(table) = &sh.kind
        && let Some(f) = deckcraft_engine::cmd::table::table_frame(st, sh)
    {
        use deckcraft_engine::cmd::table::{in_range, selected_range};
        let range = selected_range(st.selection.cells, id, table);
        for (_, _, cr) in f.cells.iter().filter(|(r, c, _)| range.is_some() && in_range(range, *r, *c)) {
            let pts = [Point::new(cr.x0, cr.y0), Point::new(cr.x1, cr.y0), Point::new(cr.x1, cr.y1), Point::new(cr.x0, cr.y1)];
            painter.add(egui::Shape::convex_polygon(pts.iter().map(|q| xf.to_screen(f.affine * *q)).collect(), t.text_selection, Stroke::NONE));
        }
    }
}

pub(super) fn overlays(app: &mut SlideApp, ui: &Ui, painter: &egui::Painter, xf: Xf, t: &Tokens) {
    // Spelling squiggles under misspelt words (checked in the background).
    if let Some(st) = app.session.active() {
        let lines = app.squiggles.update(st, Some(ui.ctx()));
        let scale = (xf.to_screen(Point::new(1.0, 0.0)) - xf.to_screen(Point::new(0.0, 0.0))).length();
        for (a, b) in lines {
            crate::squiggles::paint(painter, xf.to_screen(a), xf.to_screen(b), scale);
        }
    }
    let Some(st) = app.session.active() else { return };
    if let Some(id) = app.session.tool.cropping(st) {
        let ghost = crop_ghost(&mut app.textures, &app.media, st, id, ui.ctx());
        crop_overlay(st, id, ghost.as_ref(), painter, xf, t);
    }
    selection_handles(app, st, painter, xf, t);
    text_overlays(st, ui, painter, xf, t);
    // Smart guides, marquee, previews.
    for g in &app.session.tool.guides {
        let (a, b) =
            if g.vertical { (Point::new(g.pos, g.from), Point::new(g.pos, g.to)) } else { (Point::new(g.from, g.pos), Point::new(g.to, g.pos)) };
        painter.add(egui::Shape::dashed_line(&[xf.to_screen(a), xf.to_screen(b)], Stroke::new(1.0, t.guide), 5.0, 3.0));
    }
    if let Some(m) = app.session.tool.marquee {
        let r = Rect::from_two_pos(xf.to_screen(Point::new(m.x0, m.y0)), xf.to_screen(Point::new(m.x1, m.y1)));
        painter.rect(
            r,
            CornerRadius::ZERO,
            Color32::from_rgba_unmultiplied(80, 130, 230, 30),
            Stroke::new(1.0, Color32::from_rgb(80, 130, 230)),
            egui::StrokeKind::Inside,
        );
    }
    if let Some(pv) = app.session.tool.preview {
        draw_preview(app, painter, xf, pv, t);
    }
    // Freeform being drawn.
    let path = &app.session.tool.path_preview;
    if path.len() > 1 {
        let pts: Vec<Point> = if app.session.tool.path_smooth && path.len() > 2 {
            let d = deckcraft_engine::cmd::shape::freeform_path(path, false, true);
            let mut v = vec![];
            let bp = deckcraft_render::parse_path(&d);
            deckcraft_geom::preset_flatten(&bp, &mut |el| match el {
                deckcraft_geom::PathEl::MoveTo(q) | deckcraft_geom::PathEl::LineTo(q) => v.push(q),
                _ => {}
            });
            v
        } else {
            path.clone()
        };
        painter.add(egui::Shape::line(pts.iter().map(|q| xf.to_screen(*q)).collect(), Stroke::new(1.5, t.accent)));
        if app.session.tool.path_closing
            && let Some(s0) = path.first()
        {
            painter.circle(xf.to_screen(*s0), 5.0, t.accent, Stroke::new(1.5, t.handle_fill));
        }
    }
    // Connection sites of the shape under a connector end, and the site it would glue to.
    for q in &app.session.tool.sites {
        let c = xf.to_screen(*q);
        painter.circle(c, 3.5, t.handle_fill, Stroke::new(1.0, t.handle_stroke));
    }
    if let Some(g) = app.session.tool.glue {
        painter.circle(xf.to_screen(g), 5.0, t.accent, Stroke::new(1.5, t.handle_fill));
    }
    if app.session.tool.ink_preview.len() > 1
        && let ToolKind::Ink { color, width, mode } = &app.session.tool.kind
    {
        let pts = &app.session.tool.ink_preview;
        let legacy = deckcraft_engine::model::ink_pressure_is_legacy(pts);
        let c = theme::to_color32(*color);
        let c = if mode == "highlighter" { c.gamma_multiply(0.45) } else { c };
        if legacy || mode == "highlighter" {
            // One line, so see-through highlighter ink doesn't darken where segments meet.
            let line: Vec<Pos2> = pts.iter().map(|(x, y, _)| xf.to_screen(Point::new(*x, *y))).collect();
            painter.add(egui::Shape::line(line, Stroke::new((*width as f32 * xf.scale).max(1.0), c)));
        } else {
            for pair in pts.windows(2) {
                let [q0, q1] = pair else { continue };
                let a = deckcraft_engine::model::ink_sample_width(*width, q0.2, legacy);
                let b = deckcraft_engine::model::ink_sample_width(*width, q1.2, legacy);
                let sw = ((a + b) * 0.5 * f64::from(xf.scale)).max(1.0) as f32;
                let p0 = xf.to_screen(Point::new(q0.0, q0.1));
                let p1 = xf.to_screen(Point::new(q1.0, q1.1));
                painter.add(egui::Shape::line(vec![p0, p1], Stroke::new(sw, c)));
            }
        }
    }
}

/// The text selection (quads) of the text being edited, in slide space: in a shape or a table
/// cell, wherever its text is drawn. The caret is `cmd::text::caret_line`.
pub(crate) struct TextMarks {
    pub selection: Vec<[Point; 4]>,
}

pub(crate) fn text_marks(st: &deckcraft_engine::DocState, ts: &deckcraft_engine::TextSel) -> Option<TextMarks> {
    let (l, m) = deckcraft_engine::cmd::text::text_frame(st, ts)?;
    let pos = |p: (usize, usize)| deckcraft_text::Pos { para: p.0, ch: p.1 };
    let (s0, s1) = ts.ordered();
    let selection = l
        .selection_rects(pos(s0), pos(s1))
        .into_iter()
        .map(|r| [Point::new(r.x0, r.y0), Point::new(r.x1, r.y0), Point::new(r.x1, r.y1), Point::new(r.x0, r.y1)].map(|q| m * q))
        .collect();
    Some(TextMarks { selection })
}

fn rotate_handle(p: &egui::Painter, c: Pos2, t: &Tokens) {
    p.circle_filled(c, 6.0, t.handle_fill);
    p.add(egui::Shape::Path(egui::epaint::PathShape {
        points: (0..=20)
            .map(|i| {
                let a = -0.4 + i as f32 / 20.0 * 5.2;
                pos2(c.x + 4.0 * a.cos(), c.y + 4.0 * a.sin())
            })
            .collect(),
        closed: false,
        fill: Color32::TRANSPARENT,
        stroke: Stroke::new(1.3, t.handle_stroke).into(),
    }));
}

fn draw_preview(app: &SlideApp, p: &egui::Painter, xf: Xf, x: Xfrm, t: &Tokens) {
    let preset = match &app.session.tool.kind {
        ToolKind::Shape { preset } => preset.clone(),
        _ => "rect".into(),
    };
    if preset == deckcraft_engine::tools::DRAW_TABLE {
        // A border inside a table (flat), or the outline of a new table.
        let (a, b) = (xf.to_screen(Point::new(x.x, x.y)), xf.to_screen(Point::new(x.x + x.w, x.y + x.h)));
        if x.w < 0.5 || x.h < 0.5 {
            p.line_segment([a, b], Stroke::new(1.5, t.accent));
        } else {
            p.rect_stroke(Rect::from_two_pos(a, b), 0.0, Stroke::new(1.0, t.accent), egui::StrokeKind::Inside);
        }
        return;
    }
    if deckcraft_geom::preset::is_line_like(&preset) {
        let a = x.affine();
        p.line_segment([xf.to_screen(a * Point::new(0.0, 0.0)), xf.to_screen(a * Point::new(x.w, x.h))], Stroke::new(1.5, t.accent));
        return;
    }
    let r = Rect::from_two_pos(xf.to_screen(Point::new(x.x, x.y)), xf.to_screen(Point::new(x.x + x.w, x.y + x.h)));
    if r.width() < 1.0 || r.height() < 1.0 {
        return;
    }
    crate::ribbon::paint_preset(p, r, &preset, t.accent.gamma_multiply(0.25), t.accent);
}
