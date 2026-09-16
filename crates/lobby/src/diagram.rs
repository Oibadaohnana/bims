//! The system diagram: one star's system, fitted to the side panel.
//!
//! Normalised to the panel rather than drawn to a scale. Systems differ in
//! span by an order of magnitude and the panel is a fixed few hundred
//! pixels, so the star goes in the middle, the furthest thing goes near the
//! edge, and everything else lands in proportion. **No distances are shown
//! and none can be read off it** — that is deliberate. The lobby is where a
//! start is chosen, not where a system is explored, and a diagram a player
//! could measure would be a diagram that did the exploring for them.
//!
//! The same twelve-float format as the preview, into the same buffer, drawn
//! one after the other by the host. Positions come out in panel pixels with
//! north up; where each body and station landed is handed back so the host
//! can label them — the words are its, as always.

use worldgen::{BodyKind, StarSystem, StationKind};

use crate::draw::{Color, DrawList, KIND_ELLIPSE, KIND_RECT};

/// Pixels kept clear round the edge, so a marker on the furthest body is
/// whole rather than clipped.
const MARGIN: f32 = 22.0;

const STAR: Color = Color::rgb(0.98, 0.88, 0.55);
const ORBIT: Color = Color::rgba(0.55, 0.85, 0.75, 0.16);
const SPAWN: Color = Color::rgb(1.0, 0.86, 0.45);
const OUTLINE: Color = Color::rgb(0.03, 0.05, 0.05);

fn body_color(kind: BodyKind) -> Color {
    match kind {
        BodyKind::RockyPlanet => Color::rgb(0.72, 0.56, 0.44),
        BodyKind::GasGiant => Color::rgb(0.82, 0.70, 0.44),
        BodyKind::IceWorld => Color::rgb(0.66, 0.84, 0.92),
        BodyKind::AsteroidBelt => Color::rgb(0.55, 0.55, 0.58),
    }
}

fn body_radius(kind: BodyKind) -> f32 {
    match kind {
        BodyKind::RockyPlanet => 4.5,
        BodyKind::GasGiant => 7.5,
        BodyKind::IceWorld => 4.5,
        BodyKind::AsteroidBelt => 2.0,
    }
}

pub fn station_color(kind: StationKind) -> Color {
    match kind {
        StationKind::Orbital => Color::rgb(0.58, 0.82, 0.90),
        StationKind::Refinery => Color::rgb(0.86, 0.62, 0.30),
        StationKind::MiningOutpost => Color::rgb(0.70, 0.66, 0.46),
        StationKind::Derelict => Color::rgb(0.52, 0.48, 0.50),
        StationKind::Relay => Color::rgb(0.62, 0.74, 0.92),
    }
}

/// Where everything landed, in panel pixels, indexed as the system's
/// `bodies` and `stations` are.
#[derive(Default)]
pub struct Placed {
    pub bodies: Vec<(f32, f32)>,
    pub stations: Vec<(f32, f32)>,
}

/// Paint the system into `list`, and say where everything went.
///
/// `spawn` is the id of the station that is the pending start, if it is in
/// this system; it is ringed.
pub fn paint(
    system: &StarSystem,
    spawn: Option<u32>,
    width: f32,
    height: f32,
    list: &mut DrawList,
) -> Placed {
    list.clear();
    let (cx, cy) = (width / 2.0, height / 2.0);

    // The furthest thing from the star decides the scale. Stations are
    // included because a relay sits out past everything.
    let mut furthest = 0.0f64;
    for body in &system.bodies {
        furthest = furthest.max(body.position.length());
    }
    for (i, _) in system.stations.iter().enumerate() {
        if let Some(p) = system.absolute_position(worldgen::Node::Station(i as u32)) {
            furthest = furthest.max(p.length());
        }
    }
    let room = (width.min(height) / 2.0 - MARGIN).max(1.0);
    let scale = if furthest > 0.0 {
        room / furthest as f32
    } else {
        1.0
    };
    let place = |x: f64, y: f64| (cx + x as f32 * scale, cy - y as f32 * scale);

    let mut placed = Placed::default();

    // Orbits first, under everything: a faint ring through each body, so the
    // diagram reads as a system rather than as dots.
    for body in &system.bodies {
        let d = body.position.length() as f32 * scale * 2.0;
        list.push(KIND_ELLIPSE, cx, cy, d, d, 0.0, 0.0, 1.0, ORBIT);
    }

    list.ellipse(cx, cy, 28.0, 28.0, STAR.alpha(0.18));
    list.ellipse(cx, cy, 14.0, 14.0, STAR);

    for body in &system.bodies {
        let (x, y) = place(body.position.x, body.position.y);
        placed.bodies.push((x, y));
        let color = body_color(body.kind);
        let r = body_radius(body.kind);
        if body.kind == BodyKind::AsteroidBelt {
            // A belt is measured as a point and drawn as a handful of rocks
            // about it, at fixed angles so it holds still between frames.
            for i in 0..6 {
                let a = i as f32 * core::f32::consts::TAU / 6.0 + 0.4;
                let (dx, dy) = (a.cos() * 7.0, a.sin() * 7.0);
                list.ellipse(x + dx, y + dy, r * 2.0, r * 2.0, color);
            }
        } else {
            list.ellipse(x, y, r * 2.0, r * 2.0, color);
        }
    }

    for station in &system.stations {
        let Some(p) = system.absolute_position(worldgen::Node::Station(station.id)) else {
            placed.stations.push((cx, cy));
            continue;
        };
        let (x, y) = place(p.x, p.y);
        placed.stations.push((x, y));
        // A square with a dark edge: it sits in orbit, which at this scale is
        // on top of its planet, and the edge is what keeps it legible there.
        list.rect(x, y, 11.0, 11.0, 1.5, OUTLINE);
        list.rect(x, y, 8.0, 8.0, 1.0, station_color(station.kind));
        if spawn == Some(station.id) {
            list.push(KIND_RECT, x, y, 18.0, 18.0, 0.0, 2.0, 2.0, SPAWN);
        }
    }

    placed
}
