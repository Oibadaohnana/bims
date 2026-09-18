//! The inside of the ship: pictures of the parts the room has none for.
//!
//! The room aboard draws its own fixtures — the galley, the heads, the
//! table, the bunks, the bay, the locker — with the pictures in
//! `crates/game/src/room.rs`, and `hull` draws the skin and everything that
//! fires. What is left is what a body walks past between them: the helm,
//! the shelves, the shower, the bulkheads and the doors in them, the conduit
//! under the deck (which is [`conduit`], apart from [`part`], because it
//! needs the grid to know which way it runs). Those used to be a coloured block a tile, which is what
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

use shipdesign::parts::{Layer, PartKind, TILE, solid_corner};
use shipdesign::{Grid, PlacedPart, ShipDesign};

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

/// The workshop: the smelter's melt from dull to white, the firebrick
/// round it, the bench top, the lamp over it and the board under it, the
/// suit through the locker's window, and the armoury's gunmetal and the
/// rifles' stocks. The part colours are the palette swatches again.
const SMELT: Color = Color::rgb(0.80, 0.42, 0.20);
const MELT: Color = Color::rgb(1.0, 0.70, 0.28);
const MELT_CORE: Color = Color::rgb(1.0, 0.93, 0.66);
const BRICK: Color = Color::rgb(0.38, 0.27, 0.23);
const BENCH: Color = Color::rgb(0.56, 0.50, 0.38);
const BENCH_EDGE: Color = Color::rgba(0.30, 0.26, 0.18, 0.6);
const LAMP: Color = Color::rgb(1.0, 0.92, 0.70);
const CIRCUIT: Color = Color::rgb(0.16, 0.36, 0.28);
const SUIT: Color = Color::rgb(0.78, 0.80, 0.84);
const VISOR: Color = Color::rgb(0.38, 0.62, 0.78);
const GUNMETAL: Color = Color::rgb(0.42, 0.38, 0.44);
const STOCK: Color = Color::rgb(0.42, 0.30, 0.22);

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
        PartKind::Helm => helm(list, part),
        PartKind::Shelf => shelf(list, part),
        PartKind::Shower => shower(list, part),
        PartKind::Reactor => reactor(list, part),
        PartKind::FuelTank => tank(list, part),
        PartKind::Battery => battery(list, part),
        PartKind::LifeSupport => life_support(list, part),
        PartKind::Smelter => smelter(list, part),
        PartKind::Workbench => workbench(list, part),
        PartKind::SuitLocker => suit_locker(list, part),
        PartKind::Armoury => armoury(list, part),
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

/// Which sides of a conduit tile another run of conduit is on — north,
/// east, south, west — so the picture reaches only towards what it joins.
/// The same four-neighbour rule `shipdesign::power` builds a network by; a
/// part standing over the run is joined through the tile, not across a
/// side, and does not show as an arm.
pub fn conduit_links(design: &ShipDesign, grid: &Grid, (x, y): (u32, u32)) -> [bool; 4] {
    let run = |tile: (i32, i32)| {
        let id = grid.get(Layer::Utility, tile);
        id != 0
            && design
                .part(id)
                .is_some_and(|p| p.kind == PartKind::PowerConduit)
    };
    let (x, y) = (x as i32, y as i32);
    [
        run((x, y - 1)),
        run((x + 1, y)),
        run((x, y + 1)),
        run((x - 1, y)),
    ]
}

/// A run of conduit. Not drawn by [`part`], because it needs the grid: a
/// straight run through the tile that reaches only the sides another run
/// is on (`links`, from [`conduit_links`]), so a line of conduit is a line,
/// a corner is a corner, and a tile with none beside it is a pad in the
/// middle. Thin, so what stands on the same tile is still what the tile is
/// about.
pub fn conduit(list: &mut DrawList, tile: (u32, u32), links: [bool; 4]) {
    let (cx, cy) = middle(tile.0, tile.1);
    let thick = 4.0;
    let half = T / 2.0;
    let [north, east, south, west] = links;
    // The pad every tile has, a little wider than the run so a join reads
    // as a join; a lone tile is nothing but this.
    list.rect(cx, cy, thick + 2.0, thick + 2.0, 1.0, CONDUIT);
    if north {
        list.rect(cx, cy - half / 2.0, thick, half, 0.0, CONDUIT);
    }
    if south {
        list.rect(cx, cy + half / 2.0, thick, half, 0.0, CONDUIT);
    }
    if west {
        list.rect(cx - half / 2.0, cy, half, thick, 0.0, CONDUIT);
    }
    if east {
        list.rect(cx + half / 2.0, cy, half, thick, 0.0, CONDUIT);
    }
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

// --- the workshop ------------------------------------------------------------------

/// The smelter: a furnace housing with the hearth set into its far half —
/// firebrick round a well of melt, glowing brighter towards the middle —
/// the flue in one corner, and along the near side, where the Bim stands,
/// the pour spout over a row of ingot moulds and the controls beside them.
fn smelter(list: &mut DrawList, part: &PlacedPart) {
    let (local, across, along) = Local::of(part);
    let (w, h) = (across - 6.0, along - 6.0);
    local.push(list, KIND_RECT, 0.0, 0.0, w, h, 5.0, 0.0, PANEL);
    local.push(list, KIND_RECT, 0.0, 0.0, w, h, 5.0, 1.5, PANEL_EDGE);
    // Hazard stripes along the two sides.
    for u in [-w / 2.0 + 5.0, w / 2.0 - 5.0] {
        for i in 0..6 {
            let v = -h / 2.0 + 8.0 + i as f32 * (h - 16.0) / 5.0;
            local.push(list, KIND_RECT, u, v, 6.0, 6.0, 0.0, 0.0, STRIPE);
        }
    }
    // The hearth, in the far half: brick, then the well, then the melt in
    // rings from its dull edge to the white of its middle.
    let d = w.min(h) * 0.52;
    let hv = -h * 0.2;
    local.push(
        list,
        KIND_ELLIPSE,
        0.0,
        hv,
        d + 12.0,
        d + 12.0,
        0.0,
        0.0,
        BRICK,
    );
    local.push(
        list,
        KIND_ELLIPSE,
        0.0,
        hv,
        d + 12.0,
        d + 12.0,
        0.0,
        2.0,
        PANEL_EDGE,
    );
    local.push(list, KIND_ELLIPSE, 0.0, hv, d, d, 0.0, 0.0, DRAIN);
    for (share, colour) in [(0.86, SMELT.alpha(0.75)), (0.62, MELT), (0.34, MELT_CORE)] {
        local.push(
            list,
            KIND_ELLIPSE,
            0.0,
            hv,
            d * share,
            d * share,
            0.0,
            0.0,
            colour,
        );
    }
    // The flue, in the far corner away from the controls.
    let (fu, fv) = (w * 0.34, -h * 0.34);
    local.push(list, KIND_ELLIPSE, fu, fv, 16.0, 16.0, 0.0, 0.0, STEEL);
    local.push(list, KIND_ELLIPSE, fu, fv, 16.0, 16.0, 0.0, 2.0, PANEL_EDGE);
    local.push(list, KIND_ELLIPSE, fu, fv, 7.0, 7.0, 0.0, 0.0, DRAIN);
    // The spout out of the hearth towards the moulds, with melt in it.
    let sv = hv + d / 2.0 + 4.0;
    local.push(list, KIND_RECT, 0.0, sv, 14.0, 12.0, 2.0, 0.0, BRICK);
    local.push(
        list,
        KIND_RECT,
        0.0,
        sv,
        6.0,
        10.0,
        1.0,
        0.0,
        MELT.alpha(0.85),
    );
    // A shelf of moulds along the near side, the first two poured.
    let mv = h / 2.0 - 12.0;
    local.push(
        list,
        KIND_RECT,
        -w * 0.12,
        mv,
        w * 0.6,
        16.0,
        2.0,
        0.0,
        PANEL_LIT,
    );
    for i in 0..4 {
        let u = -w * 0.34 + i as f32 * w * 0.147;
        local.push(list, KIND_RECT, u, mv, 11.0, 10.0, 1.5, 0.0, DRAIN);
        if i < 2 {
            local.push(list, KIND_RECT, u, mv, 8.0, 7.0, 1.0, 0.0, STEEL);
        }
    }
    // The controls beside them: a dial and two lamps, the hot one lit.
    let cu = w * 0.34;
    local.push(list, KIND_RECT, cu, mv, 22.0, 18.0, 2.0, 0.0, DRAIN);
    local.push(list, KIND_ELLIPSE, cu - 5.0, mv, 8.0, 8.0, 0.0, 0.0, STEEL);
    local.push(
        list,
        KIND_RECT,
        cu - 5.0,
        mv - 2.0,
        1.5,
        4.0,
        0.0,
        0.0,
        DRAIN,
    );
    local.push(
        list,
        KIND_ELLIPSE,
        cu + 5.0,
        mv - 4.0,
        4.0,
        4.0,
        0.0,
        0.0,
        WARN,
    );
    local.push(
        list,
        KIND_ELLIPSE,
        cu + 5.0,
        mv + 4.0,
        4.0,
        4.0,
        0.0,
        0.0,
        GOOD,
    );
}

/// The workbench: a bench top with a tool rail along its far edge and the
/// tools hung on it, a vice at one end, a lamp over the other, and the
/// job in the middle — a circuit board with its parts on it. The drawer is
/// on the near side, where the Bim stands.
fn workbench(list: &mut DrawList, part: &PlacedPart) {
    let (local, across, along) = Local::of(part);
    let (w, h) = (across - 6.0, along - 6.0);
    // The frame under the top, then the top, a shade lighter on the far
    // two thirds where the light falls.
    local.push(list, KIND_RECT, 0.0, 0.0, w, h, 3.0, 0.0, PANEL);
    local.push(
        list,
        KIND_RECT,
        0.0,
        -2.0,
        w - 4.0,
        h - 10.0,
        2.0,
        0.0,
        BENCH,
    );
    local.push(
        list,
        KIND_RECT,
        0.0,
        -2.0,
        w - 4.0,
        h - 10.0,
        2.0,
        1.0,
        BENCH_EDGE,
    );
    // The drawer face along the near edge, and its pull.
    local.push(
        list,
        KIND_RECT,
        0.0,
        h / 2.0 - 4.0,
        w - 8.0,
        6.0,
        1.5,
        0.0,
        PANEL_LIT,
    );
    local.push(
        list,
        KIND_RECT,
        0.0,
        h / 2.0 - 4.0,
        w * 0.2,
        2.0,
        1.0,
        0.0,
        STEEL,
    );
    // The rail along the far edge, with a spanner, a driver and a file on it.
    let rv = -h / 2.0 + 5.0;
    local.push(
        list,
        KIND_RECT,
        0.0,
        rv,
        w - 12.0,
        2.0,
        0.0,
        0.0,
        PANEL_EDGE,
    );
    local.push(
        list,
        KIND_RECT,
        -w * 0.3,
        rv + 4.0,
        3.0,
        11.0,
        1.0,
        0.0,
        STEEL,
    );
    local.push(
        list,
        KIND_ELLIPSE,
        -w * 0.3,
        rv + 1.5,
        6.0,
        5.0,
        0.0,
        0.0,
        STEEL,
    );
    local.push(
        list,
        KIND_RECT,
        -w * 0.2,
        rv + 5.0,
        2.5,
        8.0,
        1.0,
        0.0,
        STEEL,
    );
    local.push(
        list,
        KIND_RECT,
        -w * 0.2,
        rv + 10.0,
        4.0,
        5.0,
        1.0,
        0.0,
        WARN,
    );
    local.push(
        list,
        KIND_RECT,
        -w * 0.1,
        rv + 5.0,
        3.5,
        12.0,
        0.5,
        0.0,
        PANEL_EDGE,
    );
    // The vice at the right-hand end: its body, the two jaws, the handle.
    let vu = w * 0.36;
    local.push(list, KIND_RECT, vu, 2.0, 16.0, 14.0, 2.0, 0.0, PANEL_LIT);
    local.push(list, KIND_RECT, vu - 3.0, 2.0, 4.0, 12.0, 1.0, 0.0, STEEL);
    local.push(list, KIND_RECT, vu + 4.0, 2.0, 4.0, 12.0, 1.0, 0.0, STEEL);
    local.push(list, KIND_RECT, vu + 10.0, 2.0, 2.0, 12.0, 1.0, 0.0, STEEL);
    // The job: a board with its parts on it, and the lamp's pool of light.
    local.push(
        list,
        KIND_ELLIPSE,
        -w * 0.05,
        0.0,
        34.0,
        20.0,
        0.0,
        0.0,
        LAMP.alpha(0.16),
    );
    local.push(
        list,
        KIND_RECT,
        -w * 0.05,
        1.0,
        22.0,
        14.0,
        1.5,
        0.0,
        CIRCUIT,
    );
    for (du, dv, c) in [
        (-6.0, -3.0, GOOD),
        (0.0, -3.0, STEEL),
        (6.0, -3.0, GOOD),
        (-4.0, 3.0, GLOW),
        (5.0, 3.0, STEEL),
    ] {
        local.push(
            list,
            KIND_RECT,
            -w * 0.05 + du,
            1.0 + dv,
            4.0,
            3.0,
            0.5,
            0.0,
            c,
        );
    }
    // The lamp on its arm at the left-hand end, lit.
    local.push(list, KIND_RECT, -w * 0.38, -2.0, 3.0, 16.0, 1.0, 0.0, STEEL);
    local.push(
        list,
        KIND_ELLIPSE,
        -w * 0.38,
        -8.0,
        10.0,
        10.0,
        0.0,
        0.0,
        PANEL_LIT,
    );
    local.push(
        list,
        KIND_ELLIPSE,
        -w * 0.38,
        -8.0,
        5.0,
        5.0,
        0.0,
        0.0,
        LAMP,
    );
}

/// The suit locker: a cabinet with a window in the door, and the suit
/// through it — helmet, visor and shoulders, white against the dark of
/// the inside — with the air line coiled beside it and the ready lamp lit.
fn suit_locker(list: &mut DrawList, part: &PlacedPart) {
    let (local, across, along) = Local::of(part);
    let (w, h) = (across - 6.0, along - 6.0);
    local.push(list, KIND_RECT, 0.0, 0.0, w, h, 3.0, 0.0, PANEL);
    local.push(list, KIND_RECT, 0.0, 0.0, w, h, 3.0, 1.5, PANEL_EDGE);
    // The door, with its window and the dark inside showing through.
    local.push(
        list,
        KIND_RECT,
        0.0,
        0.0,
        w - 8.0,
        h - 8.0,
        2.0,
        0.0,
        PANEL_LIT,
    );
    local.push(
        list,
        KIND_RECT,
        -3.0,
        -1.0,
        w - 20.0,
        h - 16.0,
        2.0,
        0.0,
        DRAIN,
    );
    // The suit: shoulders, then the helmet over them, then the visor.
    local.push(list, KIND_RECT, -3.0, 7.0, 18.0, 12.0, 4.0, 0.0, SUIT);
    local.push(list, KIND_ELLIPSE, -3.0, -3.0, 14.0, 14.0, 0.0, 0.0, SUIT);
    local.push(list, KIND_RECT, -3.0, -3.0, 10.0, 5.0, 2.0, 0.0, VISOR);
    // The air line hung in a coil beside it, and the lamp on the frame.
    local.push(
        list,
        KIND_ELLIPSE,
        w / 2.0 - 8.0,
        -6.0,
        8.0,
        8.0,
        0.0,
        1.5,
        STEEL,
    );
    local.push(
        list,
        KIND_ELLIPSE,
        w / 2.0 - 8.0,
        4.0,
        8.0,
        8.0,
        0.0,
        1.5,
        STEEL,
    );
    local.push(
        list,
        KIND_ELLIPSE,
        w / 2.0 - 8.0,
        h / 2.0 - 7.0,
        4.0,
        4.0,
        0.0,
        0.0,
        GOOD,
    );
    // The handle on the near side, where the Bim reaches for it.
    local.push(
        list,
        KIND_RECT,
        -3.0,
        h / 2.0 - 5.0,
        12.0,
        2.5,
        1.0,
        0.0,
        STEEL,
    );
}

/// The armoury: a gunmetal cabinet with a barred window down its front and
/// the rack behind it, three rifles stood in it, a strongbox at one end
/// and the lock's lamp glowing red — shut, and staying so.
fn armoury(list: &mut DrawList, part: &PlacedPart) {
    let (local, across, along) = Local::of(part);
    let (w, h) = (across - 6.0, along - 6.0);
    local.push(list, KIND_RECT, 0.0, 0.0, w, h, 3.0, 0.0, GUNMETAL);
    local.push(list, KIND_RECT, 0.0, 0.0, w, h, 3.0, 1.5, PANEL_EDGE);
    // The rack: a dark well with the rifles stood in it, seen end on as
    // stocks, barrels running away from the Bim.
    let rw = w * 0.6;
    local.push(
        list,
        KIND_RECT,
        -w * 0.14,
        0.0,
        rw,
        h - 10.0,
        2.0,
        0.0,
        DRAIN,
    );
    for i in 0..3 {
        let u = -w * 0.14 + (i as f32 - 1.0) * rw * 0.3;
        local.push(list, KIND_RECT, u, -3.0, 4.0, h - 20.0, 1.0, 0.0, STEEL);
        local.push(
            list,
            KIND_RECT,
            u,
            h / 2.0 - 12.0,
            7.0,
            9.0,
            1.5,
            0.0,
            STOCK,
        );
        local.push(
            list,
            KIND_RECT,
            u,
            -h / 2.0 + 10.0,
            5.0,
            3.0,
            0.5,
            0.0,
            PANEL_EDGE,
        );
    }
    // The bars across the window.
    for i in 0..2 {
        let v = (i as f32 - 0.5) * (h - 10.0) * 0.4;
        local.push(list, KIND_RECT, -w * 0.14, v, rw, 2.0, 0.0, 0.0, PANEL_EDGE);
    }
    // The strongbox at the other end, with its dial, and the lock's lamp.
    let bu = w * 0.32;
    local.push(
        list,
        KIND_RECT,
        bu,
        -4.0,
        w * 0.24,
        h - 22.0,
        2.0,
        0.0,
        PANEL,
    );
    local.push(
        list,
        KIND_RECT,
        bu,
        -4.0,
        w * 0.24,
        h - 22.0,
        2.0,
        1.0,
        PANEL_EDGE,
    );
    local.push(list, KIND_ELLIPSE, bu, -4.0, 9.0, 9.0, 0.0, 0.0, STEEL);
    local.push(list, KIND_RECT, bu, -6.0, 1.5, 4.0, 0.0, 0.0, DRAIN);
    local.push(
        list,
        KIND_ELLIPSE,
        bu,
        h / 2.0 - 7.0,
        5.0,
        5.0,
        0.0,
        0.0,
        WARN,
    );
    // Hazard chevrons along the near edge, either side of the lamp.
    for i in 0..3 {
        let u = -w * 0.38 + i as f32 * 9.0;
        local.push(
            list,
            KIND_RECT,
            u,
            h / 2.0 - 5.0,
            5.0,
            4.0,
            0.0,
            0.0,
            STRIPE,
        );
    }
}
