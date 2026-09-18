//! The outside of the ship, and what it is doing to itself.
//!
//! Pictures of the hull's working parts — the plating, the engines, the
//! thrusters, the airlock, the array — and the exhaust behind them when they
//! fire. Everything in here is drawn in **design space**: world units, `y`
//! down, about the design's origin, which is the frame every tile was laid
//! out in. `world_paint` turns the whole picture with the ship afterwards, so
//! nothing here has heard of a heading.
//!
//! # What fires, and which way
//!
//! [`Firing`] is [`flight::Effort`] read against the design, and it is the
//! only thing the exhaust is drawn from — the same closed-form plan the
//! ship's position comes from, so the flame is behind the ship at 24x and
//! after an hour's catch-up exactly as it is at 1x.
//!
//! An engine burns when it faces the way the ship is being pushed: a
//! forward engine through the burn and through a flip brake, a backward
//! engine through a brake without one, and one bolted sideways never — the
//! autopilot does not fly it, so it is dead weight and drawn as such.
//!
//! A thruster has a nozzle on every side of it that faces open space, and
//! **which nozzle fires is worked out from where it is**: exhaust out of a
//! nozzle pushes the ship the other way, and that push turns the ship about
//! its centre of mass one way or the other. The nozzle whose turn matches the
//! plan's is the one lit. The dynamics never look at where a thruster is —
//! four of them turn the ship the same however they are placed — but the
//! picture does, because a corner thruster puffing *into* the hull is a
//! picture that says the ship is broken.

use shipdesign::parts::{Layer, PartKind, Rotation, TILE, is_diagonal, solid_corner};
use shipdesign::{Grid, PlacedPart, ShipDesign};

use crate::draw::{Color, DrawList, KIND_ELLIPSE, KIND_RECT};
use crate::paint::PART_COLORS;

const T: f32 = TILE as f32;
const SQRT_2: f32 = core::f32::consts::SQRT_2;

// --- the palette --------------------------------------------------------------

const HULL: Color = Color::rgb(0.47, 0.52, 0.59);
const HULL_PANEL: Color = Color::rgb(0.39, 0.44, 0.51);
const HULL_RIM: Color = Color::rgba(0.90, 0.95, 1.0, 0.45);
/// A station too far off to draw tile by tile: one plate the size of its
/// hull, in the hull's own colour, with its icon on it.
pub const HULL_FAR: Color = Color::rgb(0.36, 0.41, 0.48);
/// A stranger's station, near or far: under the black fog of what the
/// crew have not looked into, its plate is the fog's own colour.
pub const HULL_UNKNOWN: Color = Color::rgb(0.05, 0.05, 0.06);
const STEEL: Color = Color::rgb(0.20, 0.22, 0.26);
const STEEL_LIGHT: Color = Color::rgb(0.30, 0.33, 0.38);
const STEEL_DARK: Color = Color::rgb(0.13, 0.14, 0.17);
const RIM: Color = Color::rgba(0.85, 0.92, 1.0, 0.25);
/// The deck seen through an open collar: the way through, when two airlocks
/// are mated.
const DECK_THROUGH: Color = Color::rgb(0.13, 0.15, 0.18);
const DISH: Color = Color::rgb(0.80, 0.86, 0.92);
const SHADOW: Color = Color::rgba(0.0, 0.01, 0.03, 0.72);

const FLAME_CORE: Color = Color::rgb(1.0, 0.93, 0.72);
const FLAME: Color = Color::rgb(1.0, 0.62, 0.22);
const FLAME_TAIL: Color = Color::rgb(0.86, 0.30, 0.08);
const FLAME_GLOW: Color = Color::rgba(1.0, 0.52, 0.20, 0.10);
const PUFF: Color = Color::rgb(0.82, 0.91, 1.0);

const PORT: Color = Color::rgb(1.0, 0.28, 0.22);
const STARBOARD: Color = Color::rgb(0.30, 1.0, 0.45);
const STROBE: Color = Color::rgb(1.0, 1.0, 1.0);

// --- what is firing ------------------------------------------------------------

/// The exhaust to draw, worked out once a frame.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Firing {
    /// The engines facing the ship's nose are lit.
    pub forward: bool,
    /// The engines facing its stern are lit.
    pub backward: bool,
    /// The thrusters are pushing, and which way: the sign of the angular
    /// acceleration, positive with the heading climbing. Nothing when they
    /// are not.
    pub alpha: f64,
}

impl Firing {
    /// Nothing lit — the ship docked, holding, or coasting.
    pub const NONE: Firing = Firing {
        forward: false,
        backward: false,
        alpha: 0.0,
    };

    /// Which engines an effort lights, at this heading along this line.
    ///
    /// The plan pushes along its own `direction`, `accel` signed; the engines
    /// facing the nose push along the heading. The forward engines are lit
    /// when those two agree and the backward ones when they do not — which is
    /// exactly what makes a flip brake light the same engines as the burn.
    pub fn of(effort: flight::Effort, heading: f64, direction: worldgen::math::DVec2) -> Firing {
        let mut firing = Firing {
            forward: false,
            backward: false,
            alpha: effort.alpha,
        };
        if effort.engines > 0 && effort.accel != 0.0 {
            let nose = flight::angle::facing(heading);
            let push = (nose.x * direction.x + nose.y * direction.y) * effort.accel;
            firing.forward = push > 0.0;
            firing.backward = push < 0.0;
        }
        firing
    }

    /// Whether this engine is lit.
    fn lights(self, rotation: Rotation) -> bool {
        match rotation {
            Rotation::R0 => self.forward,
            Rotation::R180 => self.backward,
            Rotation::R90 | Rotation::R270 => false,
        }
    }
}

// --- the frame a part is drawn in ---------------------------------------------

/// The four sides of a tile, as unit offsets in the grid.
const SIDES: [(f32, f32); 4] = [(0.0, -1.0), (1.0, 0.0), (0.0, 1.0), (-1.0, 0.0)];

/// A part's own axes, turned with it: `right` and `aft` as unit vectors in
/// the grid, and the angle a shape laid out unturned has to be emitted at.
/// `aft` is where an engine's exhaust goes — grid down for a part at
/// [`Rotation::R0`], which faces the nose.
fn axes(rotation: Rotation) -> ((f32, f32), (f32, f32), f32) {
    let quarter = core::f32::consts::FRAC_PI_2;
    match rotation {
        Rotation::R0 => ((1.0, 0.0), (0.0, 1.0), 0.0),
        Rotation::R90 => ((0.0, 1.0), (-1.0, 0.0), quarter),
        Rotation::R180 => ((-1.0, 0.0), (0.0, -1.0), 2.0 * quarter),
        Rotation::R270 => ((0.0, -1.0), (1.0, 0.0), 3.0 * quarter),
    }
}

/// Where a part's box is: its centre, and its extent across and along
/// `aft`.
fn part_box(part: &PlacedPart) -> ((f32, f32), f32, f32) {
    let (w, h) = shipdesign::parts::footprint(part.kind, part.rotation);
    let centre = (
        (part.origin.0 as f32 + w as f32 / 2.0) * T,
        (part.origin.1 as f32 + h as f32 / 2.0) * T,
    );
    let (across, along) = match part.rotation {
        Rotation::R0 | Rotation::R180 => (w as f32 * T, h as f32 * T),
        Rotation::R90 | Rotation::R270 => (h as f32 * T, w as f32 * T),
    };
    (centre, across, along)
}

/// A shape laid out in a part's own frame — `u` across, `v` along `aft`,
/// both from the part's centre — emitted turned with the part. `fittings`
/// draws the inside of the ship in the same frame.
pub(crate) struct Local {
    centre: (f32, f32),
    right: (f32, f32),
    aft: (f32, f32),
    rot: f32,
}

impl Local {
    pub(crate) fn of(part: &PlacedPart) -> (Local, f32, f32) {
        let (centre, across, along) = part_box(part);
        let (right, aft, rot) = axes(part.rotation);
        (
            Local {
                centre,
                right,
                aft,
                rot,
            },
            across,
            along,
        )
    }

    pub(crate) fn at(&self, u: f32, v: f32) -> (f32, f32) {
        (
            self.centre.0 + u * self.right.0 + v * self.aft.0,
            self.centre.1 + u * self.right.1 + v * self.aft.1,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn push(
        &self,
        list: &mut DrawList,
        kind: f32,
        u: f32,
        v: f32,
        w: f32,
        h: f32,
        radius: f32,
        line: f32,
        color: Color,
    ) {
        let (x, y) = self.at(u, v);
        list.push(kind, x, y, w, h, self.rot, radius, line, color);
    }
}

/// The middle of a tile.
pub(crate) fn middle(x: u32, y: u32) -> (f32, f32) {
    ((x as f32 + 0.5) * T, (y as f32 + 0.5) * T)
}

// --- a corner piece ---------------------------------------------------------------

/// A diagonal wall's tile, as the numbers every picture of one is drawn
/// from: the angle the format's triangle is emitted at, the unit normal
/// out of the hypotenuse — towards the corner the wall leaves open — and
/// the angle the hypotenuse itself runs at.
///
/// The format's triangle has its right angle at the bottom-left, which is
/// [`Rotation::R0`]'s corner ([`solid_corner`]), so the triangle's turn is
/// the part's turn and nothing more; the rest is read off the same
/// function, so a chamfer cannot be filled on one side by the painter and
/// bevelled on the other.
pub struct Corner {
    pub rot: f32,
    pub normal: (f32, f32),
    pub along: f32,
}

pub fn corner(rotation: Rotation) -> Corner {
    let (sx, sy) = solid_corner(rotation);
    Corner {
        rot: rotation.code() as f32 * core::f32::consts::FRAC_PI_2,
        normal: (-(sx as f32) / SQRT_2, -(sy as f32) / SQRT_2),
        // The hypotenuse falls left to right when the solid corner is on the
        // left or right of the *bottom* row or the *top* row — R0 and R180 —
        // and rises for the other two.
        along: match rotation {
            Rotation::R0 | Rotation::R180 => core::f32::consts::FRAC_PI_4,
            Rotation::R90 | Rotation::R270 => 3.0 * core::f32::consts::FRAC_PI_4,
        },
    }
}

/// The diagonal wall standing in a tile, if one is.
pub fn diagonal_at(design: &ShipDesign, grid: &Grid, tile: (u32, u32)) -> Option<Rotation> {
    let id = grid.get(Layer::Object, (tile.0 as i32, tile.1 as i32));
    design
        .part(id)
        .filter(|p| is_diagonal(p.kind))
        .map(|p| p.rotation)
}

/// A strip of `thick` lying along a corner piece's hypotenuse, `offset`
/// out of it along the normal — negative is into the solid half. `length`
/// is along the hypotenuse, which is `T * SQRT_2` corner to corner.
fn along_hypotenuse(
    list: &mut DrawList,
    (cx, cy): (f32, f32),
    corner: &Corner,
    offset: f32,
    length: f32,
    thick: f32,
    color: Color,
) {
    list.push(
        KIND_RECT,
        cx + corner.normal.0 * offset,
        cy + corner.normal.1 * offset,
        length,
        thick,
        corner.along,
        0.0,
        0.0,
        color,
    );
}

/// Whether there is nothing of the ship beyond this tile on that side.
/// Everything stands on the frame, so the frame is what is asked.
fn open(grid: &Grid, (x, y): (u32, u32), side: (f32, f32)) -> bool {
    grid.get(
        Layer::Structure,
        (x as i32 + side.0 as i32, y as i32 + side.1 as i32),
    ) == 0
}

/// A number between nought and one that jumps about from frame to frame
/// and from one thing to the next, for a flame that is never quite still.
/// A hash rather than the RNG: the picture must not draw from the stream
/// the simulation is on.
fn flicker(frame: u32, salt: u32) -> f32 {
    let mut h = frame.wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xFFFF) as f32 / 65535.0
}

// --- under the hull ------------------------------------------------------------

/// A dark rim round everything the ship holds, so the hull reads as a solid
/// body against the stars rather than as tiles laid on them. One strip per
/// open side, and a corner where two open sides meet, so no two overlap
/// along an edge and the rim comes out one shade.
pub fn shadow(list: &mut DrawList, design: &ShipDesign, grid: &Grid) {
    let edge = 8.0;
    for part in &design.parts {
        if part.layer() != Layer::Structure {
            continue;
        }
        for (x, y) in part.tiles() {
            let (cx, cy) = middle(x, y);
            let mut open_sides: Vec<bool> = SIDES.iter().map(|&s| open(grid, (x, y), s)).collect();
            // A corner piece: the rim runs along the hypotenuse, and the two
            // sides of the tile the wall does not reach have no rim of their
            // own — the hypotenuse is the edge of the ship there.
            if let Some(rotation) = diagonal_at(design, grid, (x, y)) {
                let c = corner(rotation);
                along_hypotenuse(list, (cx, cy), &c, edge / 2.0, T * SQRT_2, edge, SHADOW);
                let (sx, sy) = solid_corner(rotation);
                for (i, &(dx, dy)) in SIDES.iter().enumerate() {
                    if (dx as i32, dy as i32) != (sx, 0) && (dx as i32, dy as i32) != (0, sy) {
                        open_sides[i] = false;
                    }
                }
            }
            for (i, &(sx, sy)) in SIDES.iter().enumerate() {
                if !open_sides[i] {
                    continue;
                }
                // The strip along this side, just outside the tile.
                let (w, h) = if sx == 0.0 { (T, edge) } else { (edge, T) };
                list.rect(
                    cx + sx * (T + edge) / 2.0,
                    cy + sy * (T + edge) / 2.0,
                    w,
                    h,
                    0.0,
                    SHADOW,
                );
                // And the corner beyond it, when the next side round is open
                // too — an outer corner of the hull.
                let next = (i + 1) % 4;
                if open_sides[next] {
                    let (nx, ny) = SIDES[next];
                    list.rect(
                        cx + (sx + nx) * (T + edge) / 2.0,
                        cy + (sy + ny) * (T + edge) / 2.0,
                        edge,
                        edge,
                        0.0,
                        SHADOW,
                    );
                }
            }
        }
    }
}

/// The exhaust: a plume behind every engine that is lit, and a puff out of
/// every thruster nozzle that is pushing the right way. Drawn **under** the
/// hull, so an engine set inside the ship shows its flame only where it
/// clears the stern, and never over the deck.
pub fn exhaust(
    list: &mut DrawList,
    design: &ShipDesign,
    grid: &Grid,
    firing: Firing,
    centre_of_mass: (f32, f32),
    frame: u32,
) {
    for part in &design.parts {
        match part.kind {
            kind if kind.def().pushes() && firing.lights(part.rotation) => {
                plume(list, part, grid, frame)
            }
            PartKind::Thruster if firing.alpha != 0.0 => {
                let tile = part.origin;
                let (cx, cy) = middle(tile.0, tile.1);
                let (rx, ry) = (cx - centre_of_mass.0, cy - centre_of_mass.1);
                for (i, &side) in SIDES.iter().enumerate() {
                    if !open(grid, tile, side) {
                        continue;
                    }
                    // Exhaust out of this side pushes the ship the other way,
                    // and that push turns it about the centre of mass.
                    let (fx, fy) = (-side.0, -side.1);
                    let torque = rx * fy - ry * fx;
                    if (torque as f64) * firing.alpha > 0.0 {
                        puff(list, (cx, cy), side, frame, part.id * 4 + i as u32);
                    }
                }
            }
            _ => {}
        }
    }
}

/// The flame out of one engine.
///
/// It starts where the exhaust clears the ship: at the bell for an engine
/// flush with the stern, and at the skin for one set inside the hull — the
/// tiles aft of it are walked until one holds nothing, and the flame begins
/// there. Drawn under the hull, so what does not clear it is not seen; but
/// a flame whose bright end is under the deck is a dim smudge at the stern,
/// and this is what puts the bright end where it can be seen.
fn plume(list: &mut DrawList, part: &PlacedPart, grid: &Grid, frame: u32) {
    let (local, across, along) = Local::of(part);
    let f = flicker(frame, part.id);
    let g = flicker(frame.wrapping_add(7), part.id);
    let length = (3.4 + 1.4 * f) * T;
    let mut from = along / 2.0 - 0.1 * T;
    // Out to the skin, a tile at a time, and no further than a hull could
    // plausibly be deep.
    for _ in 0..16 {
        let (x, y) = local.at(0.0, from + 0.6 * T);
        let tile = ((x / T).floor() as i32, (y / T).floor() as i32);
        if grid.get(Layer::Structure, tile) == 0 {
            break;
        }
        from += T;
    }
    // A haze round the whole thing first, then four tongues from the bell
    // out, each narrower and dimmer than the last.
    local.push(
        list,
        KIND_ELLIPSE,
        0.0,
        from + length * 0.38,
        across * 1.7,
        length * 0.95,
        0.0,
        0.0,
        FLAME_GLOW,
    );
    let tongues = [
        (FLAME_CORE, 0.85),
        (FLAME, 0.65),
        (FLAME_TAIL, 0.42),
        (FLAME_TAIL, 0.18),
    ];
    for (i, &(color, alpha)) in tongues.iter().enumerate() {
        let share = i as f32 / tongues.len() as f32;
        let v = from + length * (share + 0.125);
        let w = across * (0.95 - 0.55 * share) * (0.88 + 0.24 * g);
        local.push(
            list,
            KIND_ELLIPSE,
            0.0,
            v,
            w,
            length * 0.4,
            0.0,
            0.0,
            color.alpha(alpha),
        );
    }
}

/// The puff out of one thruster nozzle, in the direction of `side`.
fn puff(list: &mut DrawList, (cx, cy): (f32, f32), side: (f32, f32), frame: u32, salt: u32) {
    let f = flicker(frame, salt);
    let rot = if side.0 == 0.0 {
        0.0
    } else {
        core::f32::consts::FRAC_PI_2
    };
    for (i, &(away, long, wide, alpha)) in [
        (0.48, 0.34, 0.26, 0.78),
        (0.88, 0.50, 0.40, 0.46),
        (1.34, 0.62, 0.56, 0.22),
    ]
    .iter()
    .enumerate()
    {
        let grow = 0.85 + 0.3 * flicker(frame.wrapping_add(i as u32 * 3), salt);
        let d = away * T * (0.9 + 0.2 * f);
        list.push(
            KIND_ELLIPSE,
            cx + side.0 * d,
            cy + side.1 * d,
            wide * T * grow,
            long * T * grow,
            rot,
            0.0,
            0.0,
            PUFF.alpha(alpha),
        );
    }
}

// --- the parts themselves ------------------------------------------------------

/// The picture for an exterior part, if it has one. `false` means the caller
/// draws its block. `mated` names the airlock that is mated to another one,
/// and how far its door stands open, if there is one.
pub fn part(
    list: &mut DrawList,
    part: &PlacedPart,
    grid: &Grid,
    firing: Firing,
    mated: Option<(u32, f32)>,
) -> bool {
    match part.kind {
        PartKind::OutsideWall => {
            for tile in part.tiles() {
                plate(list, tile, grid);
            }
        }
        PartKind::DiagonalOutsideWall => diagonal_plate(list, part.origin, part.rotation),
        PartKind::Thruster => {
            let tile = part.origin;
            plate(list, tile, grid);
            let (cx, cy) = middle(tile.0, tile.1);
            list.rect(cx, cy, T - 24.0, T - 24.0, 3.0, STEEL);
            for &side in &SIDES {
                if !open(grid, tile, side) {
                    continue;
                }
                // A nozzle poking a little way out of the skin.
                let (w, h) = if side.0 == 0.0 {
                    (0.34 * T, 0.22 * T)
                } else {
                    (0.22 * T, 0.34 * T)
                };
                let reach = T / 2.0 - 0.11 * T + 3.0;
                let (x, y) = (cx + side.0 * reach, cy + side.1 * reach);
                list.rect(x, y, w, h, 2.0, STEEL_DARK);
                list.stroke_rect(x, y, w, h, 2.0, 1.5, RIM);
            }
        }
        kind if kind.def().pushes() => engine(list, part, firing.lights(part.rotation)),
        PartKind::Airlock => match mated {
            Some((id, ajar)) if id == part.id => airlock(list, part, grid, true, ajar),
            _ => airlock(list, part, grid, false, 0.0),
        },
        PartKind::SensorArray => {
            let tile = part.origin;
            plate(list, tile, grid);
            let (cx, cy) = middle(tile.0, tile.1);
            list.push(
                KIND_ELLIPSE,
                cx,
                cy,
                0.68 * T,
                0.68 * T,
                0.0,
                0.0,
                3.0,
                DISH,
            );
            list.ellipse(cx, cy, 0.5 * T, 0.5 * T, DISH.alpha(0.45));
            list.ellipse(cx, cy, 0.16 * T, 0.16 * T, DISH);
        }
        _ => return false,
    }
    true
}

/// An airlock: a door in the skin, and a collar standing half a tile out of
/// it — `shipdesign::dock::PROTRUSION`, the same number the berth is worked
/// out from, so two docked airlocks meet collar to collar in the picture
/// exactly where they do in the arithmetic. Which way the collar points is
/// the side with no frame beyond it (`dock::port` asks the same question);
/// an airlock with hull all round is drawn as a door and nothing else.
///
/// Mated, the door is drawn parted and the collar's end open, so the two
/// collars read as one passage between the hulls.
fn airlock(list: &mut DrawList, part: &PlacedPart, grid: &Grid, mated: bool, ajar: f32) {
    let tiles = part.tiles();
    for &tile in &tiles {
        plate(list, tile, grid);
    }
    let n = tiles.len() as f32;
    let (cx, cy) = tiles.iter().fold((0.0, 0.0), |(x, y), &(tx, ty)| {
        let (mx, my) = middle(tx, ty);
        (x + mx / n, y + my / n)
    });
    let out = SIDES
        .iter()
        .copied()
        .find(|&side| tiles.iter().all(|&tile| open(grid, tile, side)));
    // How long the door is along the skin, and how deep it is through it.
    let (along, deep) = (n * T, T);
    // A rectangle `a` long the way the collar points and `b` wide across it.
    let oriented = |list: &mut DrawList,
                    x: f32,
                    y: f32,
                    a: f32,
                    b: f32,
                    radius: f32,
                    color: Color| {
        match out {
            Some((sx, _)) if sx != 0.0 => list.rect(x, y, a, b, radius, color),
            _ => list.rect(x, y, b, a, radius, color),
        }
    };
    let door = PART_COLORS[PartKind::Airlock as usize];
    let inset = 9.0;

    // The door, in the skin: two halves that part along the seam, `ajar`
    // of the way — a door, opening as somebody comes to it and shutting
    // behind them, not a switch. The deck shows between them.
    let (door_w, door_l) = (deep - 2.0 * inset, along - 2.0 * inset);
    let (tx, ty) = match out {
        Some((sx, _)) if sx != 0.0 => (0.0, 1.0),
        _ => (1.0, 0.0),
    };
    let slide = 0.32 * door_l * ajar.clamp(0.0, 1.0);
    for side in [-1.0f32, 1.0] {
        let d = side * (door_l * 0.25 + slide);
        oriented(
            list,
            cx + tx * d,
            cy + ty * d,
            door_w,
            door_l * 0.5 - 1.0,
            5.0,
            door,
        );
        // A bolt on each half, which goes with it.
        let b = side * (0.3 * along + slide);
        list.ellipse(cx + tx * b, cy + ty * b, 7.0, 7.0, STEEL_DARK);
    }
    if slide < 1.0 {
        // Shut: the seam the door parts along.
        oriented(list, cx, cy, 3.0, door_l - 6.0, 0.0, STEEL_DARK);
    }

    // The collar, out of the skin. Nothing to draw for a buried airlock.
    let Some((ox, oy)) = out else {
        return;
    };
    let reach = shipdesign::dock::PROTRUSION as f32;
    let skin = deep / 2.0;
    let (kx, ky) = (
        cx + ox * (skin + reach / 2.0),
        cy + oy * (skin + reach / 2.0),
    );
    oriented(list, kx, ky, reach, along - 14.0, 2.0, STEEL);
    oriented(
        list,
        kx,
        ky,
        reach - 4.0,
        along - 22.0,
        2.0,
        if mated { DECK_THROUGH } else { STEEL_DARK },
    );
    // The hatch at the end: shut, or open onto the other collar.
    let (hx, hy) = (
        cx + ox * (skin + reach - 2.0),
        cy + oy * (skin + reach - 2.0),
    );
    if !mated {
        oriented(list, hx, hy, 4.0, along - 18.0, 1.0, RIM);
    }
    // A rim down each side of the collar, so it reads as a tube.
    let (tx, ty) = (oy.abs(), ox.abs());
    for side in [-1.0f32, 1.0] {
        let off = side * (along / 2.0 - 8.0);
        oriented(list, kx + tx * off, ky + ty * off, reach, 3.0, 1.0, RIM);
    }
}

/// One plate of hull: a panel with a seam round it, and a bright bevel along
/// every edge that faces open space.
fn plate(list: &mut DrawList, tile: (u32, u32), grid: &Grid) {
    let (cx, cy) = middle(tile.0, tile.1);
    list.rect(cx, cy, T - 3.0, T - 3.0, 2.0, HULL);
    list.rect(cx, cy, T - 18.0, T - 18.0, 3.0, HULL_PANEL);
    let bevel = 7.0;
    for &side in &SIDES {
        if !open(grid, tile, side) {
            continue;
        }
        let (w, h) = if side.0 == 0.0 {
            (T - 3.0, bevel)
        } else {
            (bevel, T - 3.0)
        };
        list.rect(
            cx + side.0 * (T - 3.0 - bevel) / 2.0,
            cy + side.1 * (T - 3.0 - bevel) / 2.0,
            w,
            h,
            0.0,
            HULL_RIM,
        );
    }
}

/// Half a plate of hull, cut across the tile: the same panel and seam as
/// [`plate`], as triangles, with the bevel along the hypotenuse — which is
/// the edge that faces open space, whatever is beyond the two straight
/// sides.
fn diagonal_plate(list: &mut DrawList, tile: (u32, u32), rotation: Rotation) {
    let (cx, cy) = middle(tile.0, tile.1);
    let c = corner(rotation);
    let (sx, sy) = solid_corner(rotation);
    list.triangle(cx, cy, T - 3.0, T - 3.0, c.rot, HULL);
    // The inner panel is shrunk about the box's centre, which moves its
    // legs in further than its hypotenuse; nudging it into the corner evens
    // the seam up.
    let nudge = 1.3;
    list.triangle(
        cx + sx as f32 * nudge,
        cy + sy as f32 * nudge,
        T - 18.0,
        T - 18.0,
        c.rot,
        HULL_PANEL,
    );
    let bevel = 7.0;
    along_hypotenuse(
        list,
        (cx, cy),
        &c,
        -bevel / 2.0,
        (T - 3.0) * SQRT_2 - 2.0 * bevel,
        bevel,
        HULL_RIM,
    );
}

/// An engine: a housing with a bell at its aft end, and a core in the bell
/// that glows when it is lit.
fn engine(list: &mut DrawList, part: &PlacedPart, lit: bool) {
    let (local, across, along) = Local::of(part);
    let half = along / 2.0;
    let bell_at = half - 0.4 * T;
    // The housing runs from the nose end to the bell.
    let housing = (bell_at - 0.2 * T) - (-half + 4.0);
    local.push(
        list,
        KIND_RECT,
        0.0,
        (-half + 4.0 + bell_at - 0.2 * T) / 2.0,
        across - 8.0,
        housing,
        6.0,
        0.0,
        STEEL,
    );
    // A plate over the nose end, and a line down each side.
    local.push(
        list,
        KIND_RECT,
        0.0,
        -half + 0.45 * T,
        across - 0.5 * T,
        0.6 * T,
        4.0,
        0.0,
        STEEL_LIGHT,
    );
    let accent = PART_COLORS[part.kind as usize];
    for u in [-0.28 * across, 0.28 * across] {
        local.push(
            list,
            KIND_RECT,
            u,
            -0.1 * T,
            0.14 * T,
            housing * 0.55,
            3.0,
            0.0,
            accent.alpha(0.9),
        );
    }
    // The bell, and what is in it.
    local.push(
        list,
        KIND_ELLIPSE,
        0.0,
        bell_at,
        across - 0.3 * T,
        0.8 * T,
        0.0,
        0.0,
        STEEL_DARK,
    );
    local.push(
        list,
        KIND_ELLIPSE,
        0.0,
        bell_at,
        across - 0.3 * T,
        0.8 * T,
        0.0,
        2.0,
        RIM,
    );
    let (core, alpha) = if lit {
        (FLAME_CORE, 1.0)
    } else {
        (accent, 0.35)
    };
    local.push(
        list,
        KIND_ELLIPSE,
        0.0,
        bell_at + 0.05 * T,
        (across - 0.3 * T) * 0.55,
        0.4 * T,
        0.0,
        0.0,
        core.alpha(alpha),
    );
}

// --- the lights ----------------------------------------------------------------

/// Running lights: red to port, green to starboard, and a strobe at the bow.
/// Where the ship has no tile on the row or column that would carry one,
/// the nearest row or column that does is used, so an odd hull still shows
/// which way round it is.
pub fn lights(list: &mut DrawList, design: &ShipDesign, grid: &Grid, frame: u32) {
    let side = design.build_area;
    let held = |x: i32, y: i32| grid.get(Layer::Structure, (x, y)) != 0;
    let (mut x0, mut y0, mut x1, mut y1) = (side, side, 0, 0);
    for y in 0..side {
        for x in 0..side {
            if held(x as i32, y as i32) {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if x1 < x0 {
        return;
    }
    let beam = (y0 + y1) / 2;
    let keel = (x0 + x1) / 2;

    // The first held tile in from each edge along the middle row, trying
    // the rows either side of it in turn if the middle one is empty there.
    let along_row = |from_left: bool| -> Option<(u32, u32)> {
        for step in 0..=(y1 - y0) {
            for y in [beam.saturating_sub(step), beam + step] {
                if y < y0 || y > y1 {
                    continue;
                }
                let xs: Vec<u32> = if from_left {
                    (x0..=x1).collect()
                } else {
                    (x0..=x1).rev().collect()
                };
                if let Some(&x) = xs.iter().find(|&&x| held(x as i32, y as i32)) {
                    return Some((x, y));
                }
            }
        }
        None
    };
    let bow = || -> Option<(u32, u32)> {
        for step in 0..=(x1 - x0) {
            for x in [keel.saturating_sub(step), keel + step] {
                if x < x0 || x > x1 {
                    continue;
                }
                if let Some(y) = (y0..=y1).find(|&y| held(x as i32, y as i32)) {
                    return Some((x, y));
                }
            }
        }
        None
    };

    let steady = frame % 90 < 45;
    let strobe = frame % 120 < 5;
    let mut lamp = |at: Option<(u32, u32)>, side: (f32, f32), color: Color, on: bool| {
        let Some((x, y)) = at else { return };
        let (cx, cy) = middle(x, y);
        let (lx, ly) = (cx + side.0 * (T / 2.0 - 4.0), cy + side.1 * (T / 2.0 - 4.0));
        if on {
            list.ellipse(lx, ly, 22.0, 22.0, color.alpha(0.22));
            list.ellipse(lx, ly, 8.0, 8.0, color);
        } else {
            list.ellipse(lx, ly, 6.0, 6.0, color.alpha(0.35));
        }
    };
    lamp(along_row(true), (-1.0, 0.0), PORT, steady);
    lamp(along_row(false), (1.0, 0.0), STARBOARD, steady);
    lamp(bow(), (0.0, -1.0), STROBE, strobe);
}

// --- the map --------------------------------------------------------------------

/// How far each fin leans out from the marker's own line, in radians. Pinned
/// by `the_map_is_north_up_whatever_the_ship_is_doing`, which knows the
/// marker is three rectangles at three angles and nothing else on the map
/// turns.
pub const FIN_LEAN: f32 = 0.55;

/// The ship on the map: a hull with a nose and two fins, pointing along
/// `rot`, `long` from tail to nose. Three rectangles, because the format has
/// no triangle — and a marker that looks like a ship is a marker nobody has
/// to be told is the ship.
pub fn marker(list: &mut DrawList, rot: f32, long: f32, color: Color) {
    let (s, c) = (rot.sin(), rot.cos());
    // A point `v` along the nose and `u` across, turned to `rot`, about
    // the ship at the origin. The nose is grid up: `-v`.
    let at = |u: f32, v: f32| (u * c + v * s, u * s - v * c);
    let fin = long * 0.42;
    let lean = FIN_LEAN;
    for side in [-1.0f32, 1.0] {
        let (x, y) = at(side * long * 0.22, -long * 0.22);
        list.push(
            KIND_RECT,
            x,
            y,
            long * 0.16,
            fin,
            rot - side * lean,
            long * 0.05,
            0.0,
            color.alpha(0.85),
        );
    }
    let (x, y) = at(0.0, 0.0);
    list.push(
        KIND_RECT,
        x,
        y,
        long * 0.3,
        long,
        rot,
        long * 0.12,
        0.0,
        color,
    );
}
