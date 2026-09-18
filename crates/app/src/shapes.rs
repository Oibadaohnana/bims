//! The shape buffer, turned into triangles.
//!
//! Every painter in the workspace — the room's, the designer's, the lobby's
//! — writes the same twelve floats a shape: kind, centre, size, rotation,
//! corner radius, line width and a colour. The browser used to replay that
//! onto a canvas; this replays it into one mesh, which egui draws with the
//! rest of the frame, clipped to the canvas it belongs to. The field order
//! is the one thing shared with the painters, and it is in
//! `crates/game/src/draw.rs`.
//!
//! Shapes come in **world units** under a view — a scale and an offset that
//! turn them into canvas pixels — and land in a canvas, which is a rectangle
//! of the window. Screen coordinates are the canvas's: points from the
//! window's top-left corner, y down, the way every painter and every
//! pointer reading thinks.
//!
//! Every edge is **feathered**: the way epaint draws its own shapes, and
//! the only anti-aliasing there is, because bevy_egui paints into the
//! window's unsampled target and so no multisampling reaches it. A filled
//! shape is drawn half a pixel small with a ramp round it from its colour
//! to nothing a pixel wide; a stroke is a band with a ramp down each side;
//! and a stroke thinner than a pixel is drawn a pixel wide and that much
//! fainter, so a seam at a low zoom fades rather than flickers.

use bevy::prelude::*;
use bevy_egui::egui;

/// Floats per shape. The painters' `draw::STRIDE`, and asserted equal to it
/// in the tests so the two cannot drift.
pub const STRIDE: usize = 12;

const KIND_ELLIPSE: f32 = 1.0;
const KIND_TRIANGLE: f32 = 2.0;

/// How a canvas maps world units to its own pixels: `px = offset + scale *
/// world`. The room fits a fixed deck to the window, the designer pans and
/// zooms a grid, the lobby projects a galaxy itself and hands over pixels
/// with a scale of one.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub scale: f32,
    pub offset: Vec2,
}

impl View {
    pub const PIXELS: View = View {
        scale: 1.0,
        offset: Vec2::ZERO,
    };

    pub fn to_canvas(self, world: Vec2) -> Vec2 {
        self.offset + world * self.scale
    }

    pub fn to_world(self, canvas: Vec2) -> Vec2 {
        (canvas - self.offset) / self.scale
    }
}

/// A rectangle of the window, in logical points from its top-left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect {
    pub fn new(min: Vec2, max: Vec2) -> Rect {
        Rect { min, max }
    }

    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }

    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.min.x && p.x < self.max.x && p.y >= self.min.y && p.y < self.max.y
    }
}

/// The far side of a feather ramp: nothing at all, premultiplied, so the
/// ramp is a fade of the shape's own colour rather than a fade to black.
const CLEAR: egui::Color32 = egui::Color32::TRANSPARENT;

/// Triangles with a colour at every corner: an egui mesh in the making.
/// Positions are window points; the canvas's origin is added on the way in.
pub struct ShapeBuf {
    mesh: egui::Mesh,
    origin: Vec2,
    clip: Rect,
    /// One physical pixel, in points: the width of the ramp along every
    /// edge from a shape's colour to nothing.
    feather: f32,
}

impl ShapeBuf {
    /// A buffer for a canvas at `clip`, on a display with `pixels_per_point`.
    pub fn new(clip: Rect, pixels_per_point: f32) -> ShapeBuf {
        ShapeBuf {
            mesh: egui::Mesh::default(),
            origin: clip.min,
            clip,
            feather: if pixels_per_point > 0.0 {
                1.0 / pixels_per_point
            } else {
                1.0
            },
        }
    }

    pub fn is_empty(&self) -> bool {
        self.mesh.indices.is_empty()
    }

    pub fn into_shape(self) -> egui::Shape {
        egui::Shape::mesh(self.mesh)
    }

    fn vertex(&mut self, canvas: Vec2, color: egui::Color32) -> u32 {
        let i = self.mesh.vertices.len() as u32;
        let p = canvas + self.origin;
        self.mesh.vertices.push(egui::epaint::Vertex {
            pos: egui::pos2(p.x, p.y),
            uv: egui::epaint::WHITE_UV,
            color,
        });
        i
    }

    /// A convex polygon, as a fan.
    fn fan(&mut self, points: &[Vec2], color: egui::Color32) {
        if points.len() < 3 {
            return;
        }
        let first = self.vertex(points[0], color);
        let mut prev = self.vertex(points[1], color);
        for &p in &points[2..] {
            let here = self.vertex(p, color);
            self.mesh.indices.extend_from_slice(&[first, prev, here]);
            prev = here;
        }
    }

    /// The band between two rings of the same length, each ring its own
    /// colour: an outline, or one side of a feather ramp.
    fn band(&mut self, outer: &[Vec2], oc: egui::Color32, inner: &[Vec2], ic: egui::Color32) {
        let n = outer.len().min(inner.len());
        if n < 2 {
            return;
        }
        let base = self.mesh.vertices.len() as u32;
        for i in 0..n {
            self.vertex(outer[i], oc);
            self.vertex(inner[i], ic);
        }
        for i in 0..n as u32 {
            let j = (i + 1) % n as u32;
            let (o, in_) = (base + 2 * i, base + 2 * i + 1);
            let (oj, inj) = (base + 2 * j, base + 2 * j + 1);
            self.mesh
                .indices
                .extend_from_slice(&[o, oj, inj, o, inj, in_]);
        }
    }

    /// A convex polygon, filled, with its edge feathered. `extent` is the
    /// shape's narrowest width: a shape narrower than the feather gets a
    /// ramp that narrow, so a dot is never pulled inside out.
    fn fill(&mut self, points: &[Vec2], color: egui::Color32, extent: f32) {
        if points.len() < 3 {
            return;
        }
        let f = self.feather.min(extent.max(0.0));
        if f <= 0.0 {
            self.fan(points, color);
            return;
        }
        let normals = normals(points);
        let inner = offset(points, &normals, -f / 2.0);
        let outer = offset(points, &normals, f / 2.0);
        self.fan(&inner, color);
        self.band(&outer, CLEAR, &inner, color);
    }

    /// The band between two rings, feathered along both edges: a stroke.
    /// The rings are the stroke's own edges — the caller has already put
    /// them half a line either side of the path.
    fn stroke(&mut self, outer: &[Vec2], inner: &[Vec2], color: egui::Color32) {
        if outer.len().min(inner.len()) < 2 {
            return;
        }
        let f = self.feather / 2.0;
        let (no, ni) = (normals(outer), normals(inner));
        let outer_out = offset(outer, &no, f);
        let outer_in = offset(outer, &no, -f);
        // The inner ring's normals point out of the hole, into the stroke.
        let inner_out = offset(inner, &ni, f);
        let inner_in = offset(inner, &ni, -f);
        self.band(&outer_out, CLEAR, &outer_in, color);
        self.band(&outer_in, color, &inner_out, color);
        self.band(&inner_out, color, &inner_in, CLEAR);
    }

    /// A stroke's width and colour as drawn: one thinner than a pixel is
    /// drawn a pixel wide and fainter by the same ratio, so it fades
    /// rather than breaking up.
    fn thin(&self, line: f32, color: egui::Color32) -> (f32, egui::Color32) {
        if line >= self.feather {
            (line, color)
        } else {
            (self.feather, color.gamma_multiply(line / self.feather))
        }
    }

    /// Replay `shapes` — twelve floats each, in world units under `view`.
    pub fn replay(&mut self, shapes: &[f32], view: View) {
        let clip = Rect::new(Vec2::ZERO, self.clip.size());
        for s in shapes.as_chunks::<STRIDE>().0 {
            let kind = s[0];
            let centre = view.to_canvas(Vec2::new(s[1], s[2]));
            let size = Vec2::new(s[3].abs(), s[4].abs()) * view.scale;
            let rot = s[5];
            let radius = s[6] * view.scale;
            let line = s[7] * view.scale;
            let a = s[11];
            if a <= 0.0 {
                continue;
            }
            // Past the canvas by more than its own size, nothing of it can
            // show. egui clips the rest.
            let reach = size.max_element() + line + 1.0;
            if centre.x + reach < clip.min.x
                || centre.x - reach > clip.max.x
                || centre.y + reach < clip.min.y
                || centre.y - reach > clip.max.y
            {
                continue;
            }
            let color = egui::Color32::from_rgba_unmultiplied(
                (s[8] * 255.0).round() as u8,
                (s[9] * 255.0).round() as u8,
                (s[10] * 255.0).round() as u8,
                (a * 255.0).round() as u8,
            );
            if kind == KIND_ELLIPSE {
                self.ellipse(centre, size, rot, line, color);
            } else if kind == KIND_TRIANGLE {
                self.triangle(centre, size, rot, line, color);
            } else {
                self.rect(centre, size, rot, radius, line, color);
            }
        }
    }

    fn ellipse(&mut self, centre: Vec2, size: Vec2, rot: f32, line: f32, color: egui::Color32) {
        let r = size / 2.0;
        let n = segments(r.max_element() + line);
        if line > 0.0 {
            let (line, color) = self.thin(line, color);
            if r.min_element() <= line / 2.0 {
                // No hole left in the middle: a disc, as wide as the stroke.
                let r = r + line / 2.0;
                let pts = ring(centre, r, rot, n);
                self.fill(&pts, color, r.min_element() * 2.0);
                return;
            }
            let outer = ring(centre, r + line / 2.0, rot, n);
            let inner = ring(centre, r - line / 2.0, rot, n);
            self.stroke(&outer, &inner, color);
        } else {
            let pts = ring(centre, r, rot, n);
            self.fill(&pts, color, r.min_element() * 2.0);
        }
    }

    /// The bottom-left half of the box, as the painters define it.
    fn triangle(&mut self, centre: Vec2, size: Vec2, rot: f32, line: f32, color: egui::Color32) {
        let half = size / 2.0;
        let pts = [
            turned(Vec2::new(-half.x, -half.y), rot) + centre,
            turned(Vec2::new(-half.x, half.y), rot) + centre,
            turned(Vec2::new(half.x, half.y), rot) + centre,
        ];
        // The narrowest way across a right triangle is the height over its
        // hypotenuse.
        let extent = if size.x > 0.0 && size.y > 0.0 {
            size.x * size.y / size.length()
        } else {
            0.0
        };
        if line > 0.0 {
            let (line, color) = self.thin(line, color);
            if extent <= line {
                self.fill(&pts, color, extent);
                return;
            }
            let normals = normals(&pts);
            let outer = offset(&pts, &normals, line / 2.0);
            let inner = offset(&pts, &normals, -line / 2.0);
            self.stroke(&outer, &inner, color);
        } else {
            self.fill(&pts, color, extent);
        }
    }

    fn rect(
        &mut self,
        centre: Vec2,
        size: Vec2,
        rot: f32,
        radius: f32,
        line: f32,
        color: egui::Color32,
    ) {
        let half = size / 2.0;
        if line > 0.0 {
            let (line, color) = self.thin(line, color);
            let grow = Vec2::splat(line / 2.0);
            let outer_r = if radius > 0.0 {
                radius + line / 2.0
            } else {
                0.0
            };
            let corners = if radius > 0.0 {
                segments(radius + line) / 4 + 1
            } else {
                1
            };
            if half.min_element() <= line / 2.0 {
                // No hole left in the middle: a slab, as wide as the stroke.
                let pts = rounded(centre, half + grow, rot, outer_r, corners);
                self.fill(&pts, color, (half + grow).min_element() * 2.0);
                return;
            }
            // The inner ring keeps the outer's count of corner points even
            // where its radius has gone to nought, so the two pair up.
            let inner_r = if radius > 0.0 {
                (radius - line / 2.0).max(0.0)
            } else {
                0.0
            };
            let outer = rounded(centre, half + grow, rot, outer_r, corners);
            let inner = rounded(centre, half - grow, rot, inner_r, corners);
            self.stroke(&outer, &inner, color);
        } else {
            let corners = if radius > 0.0 {
                segments(radius) / 4 + 1
            } else {
                1
            };
            let pts = rounded(centre, half, rot, radius, corners);
            self.fill(&pts, color, half.min_element() * 2.0);
        }
    }
}

fn turned(p: Vec2, rot: f32) -> Vec2 {
    if rot == 0.0 {
        return p;
    }
    let (s, c) = rot.sin_cos();
    Vec2::new(p.x * c - p.y * s, p.x * s + p.y * c)
}

/// How many straight pieces a curve of this radius wants on screen: enough
/// that a big ring is round, few enough that a dot is three triangles.
fn segments(radius: f32) -> usize {
    ((radius * 1.2) as usize).clamp(8, 64)
}

fn ring(centre: Vec2, r: Vec2, rot: f32, n: usize) -> Vec<Vec2> {
    (0..n)
        .map(|i| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            turned(Vec2::new(a.cos() * r.x, a.sin() * r.y), rot) + centre
        })
        .collect()
}

/// A rectangle's outline, corners rounded by `radius` with `corners` points
/// each — one point when square. Clockwise on a y-down screen, starting at
/// the top-left.
fn rounded(centre: Vec2, half: Vec2, rot: f32, radius: f32, corners: usize) -> Vec<Vec2> {
    let r = radius.min(half.x).min(half.y).max(0.0);
    let mut pts = Vec::with_capacity(corners * 4);
    // Corner centres, and the quarter of the circle each one sweeps.
    let arcs = [
        (Vec2::new(-half.x + r, -half.y + r), std::f32::consts::PI),
        (
            Vec2::new(half.x - r, -half.y + r),
            1.5 * std::f32::consts::PI,
        ),
        (Vec2::new(half.x - r, half.y - r), 0.0),
        (
            Vec2::new(-half.x + r, half.y - r),
            0.5 * std::f32::consts::PI,
        ),
    ];
    for (at, start) in arcs {
        if corners <= 1 {
            let corner = Vec2::new(
                if at.x < 0.0 { -half.x } else { half.x },
                if at.y < 0.0 { -half.y } else { half.y },
            );
            pts.push(turned(corner, rot) + centre);
            continue;
        }
        // With no radius every point of the arc is the corner itself; the
        // repeats are kept so the ring pairs up with one that has a radius.
        for i in 0..corners {
            let a = start + i as f32 / (corners - 1) as f32 * 0.5 * std::f32::consts::PI;
            let p = at + Vec2::new(a.cos(), a.sin()) * r;
            pts.push(turned(p, rot) + centre);
        }
    }
    pts
}

/// Every point moved along its normal by `by`.
fn offset(points: &[Vec2], normals: &[Vec2], by: f32) -> Vec<Vec2> {
    points
        .iter()
        .zip(normals)
        .map(|(p, n)| *p + *n * by)
        .collect()
}

/// The outward normal at every corner of a convex polygon, scaled so that
/// moving each corner along it by `d` moves every edge out by `d` — the
/// mitre, as epaint does it. Repeated corners (a square corner of a
/// "rounded" ring) all get the mitre of the corner they make together, so a
/// ring with them in it offsets the same as one without. A sharp mitre is
/// capped, so an acute corner's ramp is a little short rather than a spike.
fn normals(points: &[Vec2]) -> Vec<Vec2> {
    let n = points.len();
    let area: f32 = (0..n)
        .map(|i| points[i].perp_dot(points[(i + 1) % n]))
        .sum();
    let sign = if area >= 0.0 { 1.0 } else { -1.0 };
    let distinct = |from: usize, step: usize| -> Option<Vec2> {
        (1..n)
            .map(|k| points[(from + k * step) % n])
            .find(|p| p.distance_squared(points[from]) > 1e-8)
    };
    (0..n)
        .map(|i| {
            let (Some(prev), Some(next)) = (distinct(i, n - 1), distinct(i, 1)) else {
                return Vec2::ZERO;
            };
            let n0 = edge_normal(points[i] - prev) * sign;
            let n1 = edge_normal(next - points[i]) * sign;
            let m = (n0 + n1) / 2.0;
            let len_sq = m.length_squared();
            if len_sq < 1e-6 {
                return n0;
            }
            let m = m / len_sq;
            if m.length_squared() > 4.0 {
                m.normalize() * 2.0
            } else {
                m
            }
        })
        .collect()
}

/// The unit normal to an edge, on the outside of a polygon whose corners
/// run anticlockwise in the axes' own sense.
fn edge_normal(edge: Vec2) -> Vec2 {
    Vec2::new(edge.y, -edge.x).normalize_or_zero()
}

#[cfg(test)]
mod tests {
    use super::*;

    const KIND_RECT: f32 = 0.0;

    #[test]
    fn the_stride_is_the_painters() {
        assert_eq!(STRIDE, bims::draw::STRIDE);
        assert_eq!(STRIDE, ship::draw::STRIDE);
        assert_eq!(STRIDE, lobby::draw::STRIDE);
    }

    #[test]
    fn a_whole_rect_is_two_triangles_in_a_ramp_and_one_past_the_canvas_is_none() {
        let clip = Rect::new(Vec2::new(10.0, 10.0), Vec2::new(110.0, 110.0));
        let mut buf = ShapeBuf::new(clip, 1.0);
        let inside = [
            KIND_RECT, 50.0, 50.0, 10.0, 10.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0,
        ];
        buf.replay(&inside, View::PIXELS);
        // The two triangles of the rect, and two along each of its four
        // edges for the ramp.
        assert_eq!(buf.mesh.indices.len(), 6 + 4 * 6);
        // The canvas's origin goes onto every corner, and the solid corner
        // is half a pixel in from the rect's own.
        assert_eq!(buf.mesh.vertices[0].pos, egui::pos2(55.5, 55.5));
        assert_eq!(buf.mesh.vertices[0].color.a(), 255);
        // The ramp's far edge is half a pixel out, and nothing at all.
        let out = buf
            .mesh
            .vertices
            .iter()
            .find(|v| v.pos == egui::pos2(54.5, 54.5))
            .expect("the ramp's outer corner");
        assert_eq!(out.color, egui::Color32::TRANSPARENT);

        let mut gone = ShapeBuf::new(clip, 1.0);
        let outside = [
            KIND_ELLIPSE,
            300.0,
            300.0,
            20.0,
            20.0,
            0.0,
            0.0,
            0.0,
            1.0,
            1.0,
            1.0,
            1.0,
        ];
        gone.replay(&outside, View::PIXELS);
        assert!(gone.is_empty());
    }

    #[test]
    fn a_stroke_is_a_band_that_leaves_the_middle_empty() {
        let clip = Rect::new(Vec2::ZERO, Vec2::new(100.0, 100.0));
        let mut buf = ShapeBuf::new(clip, 1.0);
        let ring = [
            KIND_ELLIPSE,
            50.0,
            50.0,
            40.0,
            40.0,
            0.0,
            0.0,
            2.0,
            1.0,
            1.0,
            1.0,
            1.0,
        ];
        buf.replay(&ring, View::PIXELS);
        // Every vertex sits on the stroke's two edges or half a pixel
        // either side of them, never at the centre; the ones furthest in
        // and out are the clear ends of the ramps.
        for v in &buf.mesh.vertices {
            let d = ((v.pos.x - 50.0).powi(2) + (v.pos.y - 50.0).powi(2)).sqrt();
            let r = [18.5f32, 19.5, 20.5, 21.5]
                .into_iter()
                .find(|r| (d - r).abs() < 0.1)
                .unwrap_or_else(|| panic!("{d} is on no ring"));
            let clear = r == 18.5 || r == 21.5;
            assert_eq!(v.color == egui::Color32::TRANSPARENT, clear, "{d}");
        }
    }

    #[test]
    fn a_hairline_is_drawn_a_pixel_wide_and_fainter() {
        let clip = Rect::new(Vec2::ZERO, Vec2::new(100.0, 100.0));
        let mut buf = ShapeBuf::new(clip, 1.0);
        let hair = [
            KIND_RECT, 50.0, 50.0, 40.0, 40.0, 0.0, 0.0, 0.25, 1.0, 1.0, 1.0, 1.0,
        ];
        buf.replay(&hair, View::PIXELS);
        let solid = buf.mesh.vertices.iter().map(|v| v.color.a()).max().unwrap();
        assert!(solid > 0 && solid < 128, "{solid}");
        // A pixel wide about the path at 30: the ramps meet on it, solid,
        // and reach nothing half a pixel either side.
        let at = |x: f32| {
            buf.mesh
                .vertices
                .iter()
                .find(|v| (v.pos.x - x).abs() < 0.01)
                .map(|v| v.color.a())
        };
        assert_eq!(at(30.0), Some(solid));
        assert_eq!(at(29.0), Some(0));
        assert_eq!(at(31.0), Some(0));
    }

    #[test]
    fn a_stroked_triangle_is_hollow() {
        let clip = Rect::new(Vec2::ZERO, Vec2::new(100.0, 100.0));
        let mut buf = ShapeBuf::new(clip, 1.0);
        let outline = [
            KIND_TRIANGLE,
            50.0,
            50.0,
            40.0,
            40.0,
            0.0,
            0.0,
            2.0,
            1.0,
            1.0,
            1.0,
            1.0,
        ];
        buf.replay(&outline, View::PIXELS);
        // Nothing lands deep inside the triangle: its middle, the centroid
        // of (30,30) (30,70) (70,70), is well off every edge.
        let (cx, cy) = (130.0 / 3.0, 170.0 / 3.0);
        for v in &buf.mesh.vertices {
            let d = ((v.pos.x - cx).powi(2) + (v.pos.y - cy).powi(2)).sqrt();
            assert!(d > 6.0, "{:?}", v.pos);
        }
        assert!(!buf.is_empty());
    }

    #[test]
    fn the_mitre_moves_every_edge_the_same_distance() {
        let square = [
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(10.0, 10.0),
            Vec2::new(0.0, 10.0),
        ];
        for n in normals(&square) {
            // Out along the diagonal, by root two: one unit along each axis.
            assert!(
                (n.x.abs() - 1.0).abs() < 1e-5 && (n.y.abs() - 1.0).abs() < 1e-5,
                "{n}"
            );
        }
        // The first corner is pushed towards negative x and y, and the
        // same square the other way round comes out the same way out.
        let first = normals(&square)[0];
        assert!(first.x < 0.0 && first.y < 0.0);
        let reversed: Vec<Vec2> = square.iter().rev().copied().collect();
        let last = normals(&reversed)[3];
        assert!(last.x < 0.0 && last.y < 0.0);
        // A corner given three times over is still that corner's mitre.
        let repeated = [
            Vec2::new(0.0, 0.0),
            Vec2::new(0.0, 0.0),
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(10.0, 10.0),
            Vec2::new(0.0, 10.0),
        ];
        let ns = normals(&repeated);
        assert_eq!(ns[0], ns[1]);
        assert_eq!(ns[1], ns[2]);
        assert!(ns[0].x < 0.0 && ns[0].y < 0.0);
    }

    #[test]
    fn the_view_goes_both_ways() {
        let view = View {
            scale: 2.0,
            offset: Vec2::new(10.0, 20.0),
        };
        let world = Vec2::new(3.0, 4.0);
        assert_eq!(view.to_world(view.to_canvas(world)), world);
    }
}
