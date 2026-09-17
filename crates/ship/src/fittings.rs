//! The inside of the ship: pictures of the parts the room has none for.
//!
//! The room aboard draws its own fixtures — the galley, the heads, the
//! table, the bunks, the bay, the locker — with the pictures in
//! `crates/game/src/room.rs`, and `hull` draws the skin and everything that
//! fires. What is left is what a body walks past between them: the helm,
//! the shelves, the shower, the bulkheads and the doors in them, the conduit
//! under the deck. Those used to be a coloured block a tile, which is what
//! the design phase still shows and is fine at eight pixels a tile; at the
//! game's scale a block is a hole in the picture, and a station with rooms
//! in it is mostly bulkhead.
//!
//! Drawn in **design space** like `hull`, in each part's own frame through
//! [`hull::Local`] — `u` across the part, `v` along it towards whoever uses
//! it, which is grid down for a part at `R0` — so a helm turned to face
//! the stern is drawn turned with it and its seat is still on the side the
//! Bim stands. The palette is the room's, so the two halves of the picture
//! read as one deck.

use shipdesign::PlacedPart;
use shipdesign::parts::{PartKind, TILE, solid_corner};

use crate::draw::{Color, DrawList, KIND_ELLIPSE, KIND_RECT};
use crate::hull::{Corner, Local, corner, middle};

const T: f32 = TILE as f32;

// --- the room's palette, again ------------------------------------------------

/// Composite panelling in the room's three shades.
const PANEL: Color = Color::rgb(0.19, 0.22, 0.26);
const PANEL_LIT: Color = Color::rgb(0.28, 0.32, 0.37);
const PANEL_EDGE: Color = Color::rgb(0.40, 0.46, 0.53);
const GLOW: Color = Color::rgb(0.38, 0.86, 0.95);
const GOOD: Color = Color::rgb(0.50, 0.90, 0.60);
const WARN: Color = Color::rgb(0.98, 0.45, 0.32);
const STEEL: Color = Color::rgb(0.78, 0.83, 0.87);
const DECK: Color = Color::rgb(0.13, 0.15, 0.18);

/// A bulkhead: the room's wall shade, with a darker seam between panels.
const WALL: Color = Color::rgb(0.30, 0.34, 0.40);
const WALL_PANEL: Color = Color::rgb(0.25, 0.29, 0.34);
const WALL_TRIM: Color = Color::rgba(0.62, 0.70, 0.78, 0.35);

/// The shower's tray and what is in it.
const TRAY: Color = Color::rgb(0.60, 0.70, 0.76);
const TRAY_LINE: Color = Color::rgba(0.20, 0.26, 0.31, 0.35);
const DRAIN: Color = Color::rgb(0.16, 0.19, 0.22);
const CURTAIN: Color = Color::rgba(0.86, 0.92, 0.96, 0.55);

/// What is on the shelves.
const CRATES: [Color; 4] = [
    Color::rgb(0.55, 0.44, 0.31),
    Color::rgb(0.44, 0.52, 0.58),
    Color::rgb(0.62, 0.58, 0.36),
    Color::rgb(0.38, 0.46, 0.40),
];

/// A conduit, as the design phase draws it.
const CONDUIT: Color = Color::rgba(0.74, 0.66, 0.22, 0.75);

/// The machinery's own colours: the reactor's amber, the tank's blue-grey,
/// the battery's brass and life support's teal — the palette swatches, so
/// the deck and the design phase agree about what is what.
const REACTOR: Color = Color::rgb(0.86, 0.62, 0.24);
const REACTOR_CORE: Color = Color::rgb(1.0, 0.80, 0.42);
const TANK: Color = Color::rgb(0.40, 0.48, 0.58);
const TANK_LIT: Color = Color::rgb(0.52, 0.60, 0.70);
const BATTERY: Color = Color::rgb(0.62, 0.58, 0.30);
const LIFE: Color = Color::rgb(0.34, 0.62, 0.52);
const STRIPE: Color = Color::rgb(0.92, 0.72, 0.18);

/// The picture for an interior part, if it has one. `false` means the
/// caller draws its block — the same contract as [`hull::part`], which is
/// asked first.
pub fn part(list: &mut DrawList, part: &PlacedPart) -> bool {
    match part.kind {
        PartKind::Wall => {
            for tile in part.tiles() {
                wall(list, tile);
            }
        }
        PartKind::DiagonalWall => diagonal_wall(list, part),
        PartKind::Door => door(list, part),
        PartKind::PowerConduit => {
            for tile in part.tiles() {
                conduit(list, tile);
            }
        }
        PartKind::Helm => helm(list, part),
        PartKind::Shelf => shelf(list, part),
        PartKind::Shower => shower(list, part),
        PartKind::Reactor => reactor(list, part),
        PartKind::FuelTank => tank(list, part),
        PartKind::Battery => battery(list, part),
        PartKind::LifeSupport => life_support(list, part),
        _ => return false,
    }
    true
}

// --- bulkheads ---------------------------------------------------------------------

/// One tile of bulkhead: a slab with a panel let into it and a seam round
/// the edge, so a run of them reads as panelling rather than as one grey
/// bar.
fn wall(list: &mut DrawList, tile: (u32, u32)) {
    let (cx, cy) = middle(tile.0, tile.1);
    list.rect(cx, cy, T - 1.0, T - 1.0, 0.0, WALL);
    list.rect(cx, cy, T - 12.0, T - 12.0, 2.0, WALL_PANEL);
    list.stroke_rect(cx, cy, T - 12.0, T - 12.0, 2.0, 1.0, WALL_TRIM);
}

/// A bulkhead cut across its tile, with the trim along the cut.
fn diagonal_wall(list: &mut DrawList, part: &PlacedPart) {
    let (cx, cy) = middle(part.origin.0, part.origin.1);
    let c: Corner = corner(part.rotation);
    let (sx, sy) = solid_corner(part.rotation);
    list.triangle(cx, cy, T - 1.0, T - 1.0, c.rot, WALL);
    let nudge = 1.0;
    list.triangle(
        cx + sx as f32 * nudge,
        cy + sy as f32 * nudge,
        T - 12.0,
        T - 12.0,
        c.rot,
        WALL_PANEL,
    );
    let trim = 2.0;
    list.push(
        KIND_RECT,
        cx - c.normal.0 * trim,
        cy - c.normal.1 * trim,
        (T - 1.0) * core::f32::consts::SQRT_2 - 3.0 * trim,
        1.5,
        c.along,
        0.0,
        0.0,
        WALL_TRIM,
    );
}

/// A door: a threshold in the deck, and the two leaves drawn back into the
/// bulkhead either side of it. Two tiles along the bulkhead and one deep,
/// drawn in the part's own frame so it turns with the part — `along` is
/// the run and `across` the bulkhead's depth, and which is which is the
/// rotation's business, read where the room reads it
/// (`door_slides_along_x`) rather than guessed off the neighbours.
fn door(list: &mut DrawList, part: &PlacedPart) {
    let (local, across, along) = Local::of(part);
    // The threshold: a lighter strip of deck the length of the opening.
    local.push(
        list,
        KIND_RECT,
        0.0,
        0.0,
        across * 0.5,
        along - 2.0,
        2.0,
        0.0,
        PANEL,
    );
    local.push(
        list,
        KIND_RECT,
        0.0,
        0.0,
        across * 0.34,
        along - 6.0,
        2.0,
        0.0,
        DECK,
    );
    // The two leaves, a tile each, drawn back so the way through is open.
    let leaf = along * 0.22;
    for side in [-1.0f32, 1.0] {
        let v = side * (along / 2.0 - leaf / 2.0 - 1.0);
        local.push(
            list,
            KIND_RECT,
            0.0,
            v,
            across * 0.28,
            leaf,
            1.5,
            0.0,
            PANEL_LIT,
        );
        local.push(
            list,
            KIND_RECT,
            0.0,
            v,
            across * 0.28,
            leaf,
            1.5,
            1.0,
            PANEL_EDGE,
        );
    }
    // And the lamp over it, lit.
    let (gx, gy) = local.at(across * 0.34, 0.0);
    list.ellipse(gx, gy, 5.0, 5.0, GOOD);
}

/// A run of conduit: a cross through the tile, thin, so what stands on the
/// same tile is still what the tile is about.
fn conduit(list: &mut DrawList, tile: (u32, u32)) {
    let (cx, cy) = middle(tile.0, tile.1);
    let thick = 4.0;
    list.rect(cx, cy, T, thick, 0.0, CONDUIT);
    list.rect(cx, cy, thick, T, 0.0, CONDUIT);
}

// --- systems -------------------------------------------------------------------------

/// The helm: a console the width of the part with a wide screen along its
/// far edge, instruments under it, and a yoke on the near side where the
/// pilot stands.
fn helm(list: &mut DrawList, part: &PlacedPart) {
    let (local, across, along) = Local::of(part);
    let (w, h) = (across - 6.0, along - 6.0);
    // The desk, and a lighter working surface let into it.
    local.push(list, KIND_RECT, 0.0, 0.0, w, h, 5.0, 0.0, PANEL);
    local.push(list, KIND_RECT, 0.0, 0.0, w, h, 5.0, 1.5, PANEL_EDGE);
    // The screen, dark with the glow of a chart on it.
    let screen_h = h * 0.42;
    let screen_v = -h / 2.0 + screen_h / 2.0 + 4.0;
    local.push(
        list,
        KIND_RECT,
        0.0,
        screen_v,
        w - 10.0,
        screen_h,
        3.0,
        0.0,
        DRAIN,
    );
    local.push(
        list,
        KIND_RECT,
        0.0,
        screen_v,
        w - 14.0,
        screen_h - 4.0,
        2.0,
        0.0,
        GLOW.alpha(0.16),
    );
    // A track across the chart, and the ship on it.
    local.push(
        list,
        KIND_RECT,
        0.0,
        screen_v + 1.0,
        w - 24.0,
        1.5,
        0.0,
        0.0,
        GLOW.alpha(0.55),
    );
    local.push(
        list,
        KIND_ELLIPSE,
        -w * 0.18,
        screen_v + 1.0,
        6.0,
        6.0,
        0.0,
        0.0,
        GLOW,
    );
    local.push(
        list,
        KIND_ELLIPSE,
        w * 0.3,
        screen_v + 1.0,
        4.0,
        4.0,
        0.0,
        1.0,
        GLOW.alpha(0.7),
    );
    // Instruments under the screen: a row of readouts in three colours.
    let row_v = screen_v + screen_h / 2.0 + 7.0;
    let lights = [
        (-0.36, GOOD),
        (-0.24, GOOD),
        (-0.12, GLOW),
        (0.12, GLOW),
        (0.24, WARN),
        (0.36, GOOD),
    ];
    for &(u, color) in &lights {
        local.push(
            list,
            KIND_RECT,
            u * w,
            row_v,
            w * 0.08,
            5.0,
            1.0,
            0.0,
            color.alpha(0.85),
        );
    }
    // The yoke, on the pilot's side, and the throttle beside it.
    let yoke_v = h / 2.0 - 9.0;
    local.push(list, KIND_RECT, 0.0, yoke_v, w * 0.3, 5.0, 2.0, 0.0, STEEL);
    local.push(
        list,
        KIND_ELLIPSE,
        0.0,
        yoke_v - 1.0,
        8.0,
        8.0,
        0.0,
        0.0,
        PANEL_EDGE,
    );
    local.push(
        list,
        KIND_RECT,
        w * 0.32,
        yoke_v - 2.0,
        4.0,
        12.0,
        1.0,
        0.0,
        STEEL,
    );
    local.push(
        list,
        KIND_ELLIPSE,
        w * 0.32,
        yoke_v - 7.0,
        6.0,
        6.0,
        0.0,
        0.0,
        WARN,
    );
}

// --- stores ----------------------------------------------------------------------------

/// A shelf: a rack seen from above, three boards deep, with crates on them
/// and the open side towards whoever takes something down.
fn shelf(list: &mut DrawList, part: &PlacedPart) {
    let (local, across, along) = Local::of(part);
    let (w, h) = (across - 6.0, along - 6.0);
    local.push(list, KIND_RECT, 0.0, 0.0, w, h, 3.0, 0.0, PANEL);
    // The uprights down each side.
    for u in [-w / 2.0 + 2.5, w / 2.0 - 2.5] {
        local.push(list, KIND_RECT, u, 0.0, 4.0, h, 1.0, 0.0, PANEL_EDGE);
    }
    // Three boards, each with a couple of crates on it.
    for (row, share) in [-0.3f32, 0.0, 0.3].iter().enumerate() {
        let v = share * h;
        local.push(
            list,
            KIND_RECT,
            0.0,
            v + 5.0,
            w - 8.0,
            2.0,
            0.0,
            0.0,
            PANEL_EDGE,
        );
        for (i, u) in [-w * 0.22, w * 0.1, w * 0.3].iter().enumerate() {
            // Not every place is taken, or the rack reads as a solid block.
            if (row + i) % 3 == 2 {
                continue;
            }
            let crate_w = if i == 1 { w * 0.3 } else { w * 0.2 };
            local.push(
                list,
                KIND_RECT,
                *u,
                v,
                crate_w,
                9.0,
                1.5,
                0.0,
                CRATES[(row * 2 + i) % CRATES.len()],
            );
        }
    }
}

// --- the shower ----------------------------------------------------------------------

/// A shower: a tiled tray with the drain in the middle, the head on an arm
/// out of the far wall, and a curtain drawn back along one side.
fn shower(list: &mut DrawList, part: &PlacedPart) {
    let (local, across, along) = Local::of(part);
    let (w, h) = (across - 6.0, along - 6.0);
    local.push(list, KIND_RECT, 0.0, 0.0, w, h, 4.0, 0.0, TRAY);
    local.push(list, KIND_RECT, 0.0, 0.0, w, h, 4.0, 1.5, PANEL_EDGE);
    // Tile lines across the tray, both ways.
    for share in [-0.25f32, 0.25] {
        local.push(
            list,
            KIND_RECT,
            share * w,
            0.0,
            1.0,
            h - 6.0,
            0.0,
            0.0,
            TRAY_LINE,
        );
        local.push(
            list,
            KIND_RECT,
            0.0,
            share * h,
            w - 6.0,
            1.0,
            0.0,
            0.0,
            TRAY_LINE,
        );
    }
    // The drain.
    local.push(list, KIND_ELLIPSE, 0.0, 0.0, 10.0, 10.0, 0.0, 0.0, DRAIN);
    local.push(
        list,
        KIND_ELLIPSE,
        0.0,
        0.0,
        5.0,
        5.0,
        0.0,
        0.0,
        TRAY_LINE.alpha(0.8),
    );
    // The head, on its arm out of the far wall.
    let arm_v = -h / 2.0 + 6.0;
    local.push(
        list,
        KIND_RECT,
        0.0,
        arm_v + 2.0,
        3.0,
        12.0,
        1.0,
        0.0,
        STEEL,
    );
    local.push(
        list,
        KIND_ELLIPSE,
        0.0,
        arm_v + 9.0,
        11.0,
        11.0,
        0.0,
        0.0,
        STEEL,
    );
    local.push(
        list,
        KIND_ELLIPSE,
        0.0,
        arm_v + 9.0,
        6.0,
        6.0,
        0.0,
        0.0,
        PANEL_EDGE,
    );
    // The curtain rail down one side, with the curtain bunched at the far
    // end of it.
    let rail_u = w / 2.0 - 4.0;
    local.push(list, KIND_RECT, rail_u, 0.0, 2.0, h - 4.0, 0.0, 0.0, STEEL);
    for i in 0..4 {
        let v = -h / 2.0 + 6.0 + i as f32 * 6.0;
        local.push(
            list,
            KIND_ELLIPSE,
            rail_u - 1.0,
            v,
            9.0,
            7.0,
            0.0,
            0.0,
            CURTAIN,
        );
    }
}

// --- machinery ----------------------------------------------------------------------

/// The reactor: a housing with the containment vessel set into it, rings
/// round a core that glows, and the hazard stripes along its edges that
/// say what it is from across the deck.
fn reactor(list: &mut DrawList, part: &PlacedPart) {
    let (local, across, along) = Local::of(part);
    let (w, h) = (across - 6.0, along - 6.0);
    local.push(list, KIND_RECT, 0.0, 0.0, w, h, 5.0, 0.0, PANEL);
    local.push(list, KIND_RECT, 0.0, 0.0, w, h, 5.0, 1.5, PANEL_EDGE);
    // Hazard stripes down the two long edges.
    for u in [-w / 2.0 + 5.0, w / 2.0 - 5.0] {
        for i in 0..5 {
            let v = -h / 2.0 + 8.0 + i as f32 * (h - 16.0) / 4.0;
            local.push(list, KIND_RECT, u, v, 6.0, 6.0, 0.0, 0.0, STRIPE);
        }
    }
    // The vessel: a ring, a darker well, and the core in it.
    let d = w.min(h) * 0.7;
    local.push(list, KIND_ELLIPSE, 0.0, 0.0, d, d, 0.0, 0.0, PANEL_LIT);
    local.push(list, KIND_ELLIPSE, 0.0, 0.0, d, d, 0.0, 2.0, PANEL_EDGE);
    local.push(
        list,
        KIND_ELLIPSE,
        0.0,
        0.0,
        d * 0.72,
        d * 0.72,
        0.0,
        0.0,
        DRAIN,
    );
    local.push(
        list,
        KIND_ELLIPSE,
        0.0,
        0.0,
        d * 0.5,
        d * 0.5,
        0.0,
        0.0,
        REACTOR.alpha(0.55),
    );
    local.push(
        list,
        KIND_ELLIPSE,
        0.0,
        0.0,
        d * 0.3,
        d * 0.3,
        0.0,
        0.0,
        REACTOR_CORE,
    );
    // Four bolts round the vessel.
    for (u, v) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        local.push(
            list,
            KIND_ELLIPSE,
            u * d * 0.42,
            v * d * 0.42,
            6.0,
            6.0,
            0.0,
            0.0,
            STEEL,
        );
    }
}

/// The fuel tank: two cylinders side by side in a cradle, capped at both
/// ends, with the filler on the side it is worked from and a gauge down
/// one of them.
fn tank(list: &mut DrawList, part: &PlacedPart) {
    let (local, across, along) = Local::of(part);
    let (w, h) = (across - 6.0, along - 6.0);
    local.push(list, KIND_RECT, 0.0, 0.0, w, h, 4.0, 0.0, PANEL);
    let cyl = w * 0.42;
    for (i, u) in [-w * 0.24, w * 0.24].iter().enumerate() {
        // The body, a lighter strip down its crown, and the caps.
        local.push(list, KIND_RECT, *u, 0.0, cyl, h - 8.0, cyl * 0.5, 0.0, TANK);
        local.push(
            list,
            KIND_RECT,
            *u - cyl * 0.18,
            0.0,
            cyl * 0.2,
            h - 22.0,
            3.0,
            0.0,
            TANK_LIT.alpha(0.7),
        );
        for v in [-h / 2.0 + 8.0, h / 2.0 - 8.0] {
            local.push(list, KIND_RECT, *u, v, cyl - 4.0, 5.0, 2.0, 0.0, STEEL);
        }
        // The gauge on the first, a filler cap on the second.
        if i == 0 {
            local.push(
                list,
                KIND_RECT,
                *u + cyl * 0.3,
                0.0,
                3.0,
                h * 0.5,
                0.0,
                0.0,
                DRAIN,
            );
            local.push(
                list,
                KIND_RECT,
                *u + cyl * 0.3,
                h * 0.08,
                3.0,
                h * 0.34,
                0.0,
                0.0,
                GOOD,
            );
        } else {
            local.push(
                list,
                KIND_ELLIPSE,
                *u,
                h / 2.0 - 8.0,
                9.0,
                9.0,
                0.0,
                0.0,
                STEEL,
            );
            local.push(
                list,
                KIND_ELLIPSE,
                *u,
                h / 2.0 - 8.0,
                4.0,
                4.0,
                0.0,
                0.0,
                DRAIN,
            );
        }
    }
}

/// A battery: a cell in a casing, terminals at one end and a charge bar
/// of lit segments down the middle.
fn battery(list: &mut DrawList, part: &PlacedPart) {
    let (local, across, along) = Local::of(part);
    let (w, h) = (across - 8.0, along - 8.0);
    local.push(list, KIND_RECT, 0.0, 0.0, w, h, 4.0, 0.0, DRAIN);
    local.push(
        list,
        KIND_RECT,
        0.0,
        0.0,
        w - 6.0,
        h - 6.0,
        3.0,
        0.0,
        BATTERY,
    );
    for u in [-w * 0.22, w * 0.22] {
        local.push(
            list,
            KIND_RECT,
            u,
            -h / 2.0 + 2.0,
            7.0,
            6.0,
            1.0,
            0.0,
            STEEL,
        );
    }
    for i in 0..4 {
        let v = h * 0.3 - i as f32 * h * 0.17;
        let colour = if i < 3 { GOOD } else { GOOD.alpha(0.3) };
        local.push(list, KIND_RECT, 0.0, v, w * 0.5, h * 0.11, 1.0, 0.0, colour);
    }
}

/// Life support: a cabinet with the big fan behind its grille, the pipes
/// out of one end, and a lamp that says the air is good.
fn life_support(list: &mut DrawList, part: &PlacedPart) {
    let (local, across, along) = Local::of(part);
    let (w, h) = (across - 6.0, along - 6.0);
    local.push(list, KIND_RECT, 0.0, 0.0, w, h, 5.0, 0.0, PANEL);
    local.push(list, KIND_RECT, 0.0, 0.0, w, h, 5.0, 1.5, PANEL_EDGE);
    local.push(
        list,
        KIND_RECT,
        0.0,
        0.0,
        w - 12.0,
        h - 12.0,
        4.0,
        0.0,
        LIFE.alpha(0.35),
    );
    // The fan: a well, four blades, and the hub.
    let d = w.min(h) * 0.62;
    let (fu, fv) = (-w * 0.12, 0.0);
    local.push(list, KIND_ELLIPSE, fu, fv, d, d, 0.0, 0.0, DRAIN);
    for i in 0..4 {
        let angle = i as f32 * core::f32::consts::FRAC_PI_4;
        let (x, y) = local.at(fu, fv);
        list.push(KIND_RECT, x, y, d * 0.86, d * 0.16, angle, 3.0, 0.0, LIFE);
    }
    local.push(
        list,
        KIND_ELLIPSE,
        fu,
        fv,
        d * 0.26,
        d * 0.26,
        0.0,
        0.0,
        STEEL,
    );
    local.push(list, KIND_ELLIPSE, fu, fv, d, d, 0.0, 2.0, PANEL_EDGE);
    // The grille: three bars across the fan.
    for i in 0..3 {
        let v = fv - d * 0.3 + i as f32 * d * 0.3;
        local.push(
            list,
            KIND_RECT,
            fu,
            v,
            d,
            2.0,
            0.0,
            0.0,
            PANEL_EDGE.alpha(0.8),
        );
    }
    // Pipes down the other side, and the lamp.
    for i in 0..3 {
        let u = w * 0.3 + (i as f32 - 1.0) * 7.0;
        local.push(list, KIND_RECT, u, 0.0, 4.0, h - 16.0, 2.0, 0.0, STEEL);
    }
    local.push(
        list,
        KIND_ELLIPSE,
        w * 0.3,
        h / 2.0 - 7.0,
        6.0,
        6.0,
        0.0,
        0.0,
        GOOD,
    );
}
