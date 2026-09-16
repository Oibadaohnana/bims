//! The geometry of the game view.
//!
//! **An exception to "the cdylibs' tests are the probes and the harnesses",
//! and a narrow one.** Almost everything in this crate is browser-shaped —
//! a pointer, a ghost, a shape buffer — and is checked by
//! `scratchpad/ship-check.mjs` running the real page against the real wasm.
//! What is below is not: it is the arithmetic that turns a design tile into a
//! place on screen and back, it is pure, and getting it wrong looks like a
//! ship you cannot click on rather than like an error. That is worth a unit
//! test, so the crate is an `rlib` as well as a `cdylib` and
//! `nix flake check` runs this with the rest.

use flight::angle;
use shipdesign::fixture::flyer;
use shipdesign::parts::TILE;
use worldgen::GalaxyType;
use worldgen::math::dvec2;

use crate::game::{Game, ViewMode};
use crate::world_paint;

const CANVAS: (f32, f32) = (960.0, 640.0);

fn game() -> Game {
    let galaxy = worldgen::Galaxy::new(world::data::DEFAULT_SEED, GalaxyType::SpiralTwoArm);
    let (star, station) = world::spawn(&galaxy).expect("the default seed has a dock");
    Game::start(
        flyer(2),
        40_000,
        2,
        0,
        world::data::DEFAULT_SEED,
        GalaxyType::SpiralTwoArm,
        star,
        station,
        CANVAS.0,
        CANVAS.1,
    )
    .expect("the fixture should open a world")
}

/// Where a design tile's middle lands on the canvas, the way the painter puts
/// it there. Written out here rather than called, so the test is comparing the
/// painter's arithmetic against a statement of it rather than against itself.
fn on_canvas(game: &Game, tile: (u32, u32)) -> (f32, f32) {
    let centre = game.world.ship.dynamics.centre_of_mass;
    let middle = dvec2(
        (tile.0 as f64 + 0.5) * TILE as f64,
        (tile.1 as f64 + 0.5) * TILE as f64,
    );
    let d = middle.sub(centre);
    let h = game.ship_turn();
    let (s, c) = (h.sin(), h.cos());
    let (vx, vy) = ((d.x * c - d.y * s) as f32, (d.x * s + d.y * c) as f32);
    let camera = &game.ship_view;
    (
        camera.offset_x() + vx * camera.scale(),
        camera.offset_y() + vy * camera.scale(),
    )
}

/// At rest the ship is drawn exactly as it was laid out: the grid's up is up,
/// and its right is right.
#[test]
fn at_a_heading_of_nothing_the_design_is_the_way_it_was_built() {
    let game = game();
    assert_eq!(game.world.ship.heading, 0.0);

    let middle = game.world.ship.design.build_area / 2;
    let here = on_canvas(&game, (middle, middle));
    let above = on_canvas(&game, (middle, middle - 1));
    let right = on_canvas(&game, (middle + 1, middle));

    assert!(above.1 < here.1, "the grid's up should be up the screen");
    assert!((above.0 - here.0).abs() < 1e-3, "and straight up");
    assert!(right.0 > here.0, "the grid's right should be right");
    assert!((right.1 - here.1).abs() < 1e-3, "and straight across");
}

/// Turned a quarter, the nose points at the right-hand edge. That is the whole
/// of what "Forward is the grid's up" means once the ship is moving.
#[test]
fn at_a_quarter_turn_the_nose_points_to_the_right() {
    let mut game = game();
    game.world.ship.heading = std::f64::consts::FRAC_PI_2;

    let middle = game.world.ship.design.build_area / 2;
    let here = on_canvas(&game, (middle, middle));
    let forward = on_canvas(&game, (middle, middle - 1));

    assert!(forward.0 > here.0, "Forward should be to the right");
    assert!(
        (forward.1 - here.1).abs() < 1e-3,
        "and level with where it started",
    );

    // And the other three quarters, so the sense of the turn is pinned as
    // clockwise rather than merely as "a turn". `here` is recomputed each
    // time: the tile being measured from is not the centre of mass, so it
    // swings round with everything else.
    game.world.ship.heading = std::f64::consts::PI;
    let here = on_canvas(&game, (middle, middle));
    let forward = on_canvas(&game, (middle, middle - 1));
    assert!(
        forward.1 > here.1,
        "half a turn puts the nose down the screen"
    );

    game.world.ship.heading = 3.0 * std::f64::consts::FRAC_PI_2;
    let here = on_canvas(&game, (middle, middle));
    let forward = on_canvas(&game, (middle, middle - 1));
    assert!(forward.0 < here.0, "three quarters puts it to the left");
}

/// A point on the canvas over a known tile has to come back as that tile, at
/// every heading, north up and head up. This is the one that a wrong sign in
/// the inverse turn breaks — and it breaks it silently, as a ship you cannot
/// click on once it has turned.
#[test]
fn a_screen_point_maps_back_to_the_tile_it_is_over() {
    let mut game = game();
    let quarter = std::f64::consts::FRAC_PI_2;
    for head_up in [false, true] {
        game.head_up = head_up;
        for &heading in &[0.0, quarter, std::f64::consts::PI, 3.0 * quarter, 0.7] {
            game.world.ship.heading = heading;
            for tile in [(2u32, 2u32), (9, 3), (17, 17), (10, 10)] {
                let (x, y) = on_canvas(&game, tile);
                let back = game.tile_at(x, y);
                assert_eq!(
                    back,
                    (tile.0 as i32, tile.1 as i32),
                    "head up {head_up}, heading {heading}, tile {tile:?} came back as {back:?}",
                );
            }
        }
    }
}

/// Head up, the ship is drawn the way it was laid out whatever its heading,
/// and it is the sky that turns — by the heading undone — so a tile is where
/// it was at rest and a speck has moved.
#[test]
fn head_up_holds_the_ship_square_and_turns_the_sky() {
    let mut game = game();
    game.head_up = true;
    game.world.ship.heading = 1.1;
    let mut list = crate::draw::DrawList::new();
    world_paint::paint(&game, &mut list);
    let angles = shape_rotations(&list);

    assert!(
        !angles.iter().any(|a| (a - 1.1).abs() < 1e-6),
        "nothing should be turned to the heading when the view is head up",
    );
    assert!(
        angles.iter().any(|a| (a + 1.1).abs() < 1e-6),
        "the sky should be turned by the heading undone",
    );
    // The tiles land exactly where they do at rest — the same picture the
    // designer drew, in the middle of the window.
    for tile in [(2u32, 2u32), (17, 17)] {
        let turned = on_canvas(&game, tile);
        game.world.ship.heading = 0.0;
        let at_rest = on_canvas(&game, tile);
        game.world.ship.heading = 1.1;
        assert!((turned.0 - at_rest.0).abs() < 1e-3 && (turned.1 - at_rest.1).abs() < 1e-3);
    }
    // And a name over a Bim's head follows the same arithmetic, so it stays
    // over the Bim rather than turning off to where the sky went.
    let (cx, cy) = world_paint::crew_on_screen(&game, 0);
    game.world.ship.heading = 0.0;
    let (rx, ry) = world_paint::crew_on_screen(&game, 0);
    assert!((cx - rx).abs() < 1e-3 && (cy - ry).abs() < 1e-3);
}

/// Head up, the map turns round the ship by the heading undone and the marker
/// for the ship stands straight up — and a click still lands on what it looks
/// like it landed on, which is the half that would fail quietly.
#[test]
fn head_up_turns_the_map_and_the_pointer_follows() {
    let mut game = game();
    game.set_mode(ViewMode::Map);
    game.head_up = true;
    game.world.ship.heading = 2.4;

    let mut list = crate::draw::DrawList::new();
    world_paint::paint(&game, &mut list);
    let angles: Vec<f32> = shape_rotations(&list)
        .into_iter()
        .filter(|a| a.abs() > 1e-6)
        .collect();
    assert!(
        !angles.iter().any(|a| (a - 2.4).abs() < 1e-6),
        "the marker should stand straight up rather than to the heading",
    );
    assert!(
        angles.iter().any(|a| (a + 2.4).abs() < 1e-6),
        "the system should be turned by the heading undone",
    );

    // Somewhere out on the map. Where the painter puts it is the offset from
    // the ship with the y flipped and then the camera's turn applied, and a
    // click there has to pick it and a point read there has to be it.
    let here = game.world.ship.position();
    let camera = &game.map_view;
    let (node, at) = game
        .world
        .discovered
        .iter()
        .filter_map(|&n| game.world.system.absolute_position(n).map(|p| (n, p)))
        .find(|(_, p)| p.distance(here) > 0.0)
        .expect("the spawn system is charted, so there is something out there");
    let offset = at.sub(here);
    let (ox, oy) = crate::game::turned(offset.x as f32, -offset.y as f32, -2.4);
    let (sx, sy) = (
        camera.offset_x() + ox * camera.scale(),
        camera.offset_y() + oy * camera.scale(),
    );
    assert_eq!(game.pick(sx, sy, 4.0), Some(node));
    let read = game.point_at(sx, sy);
    let tolerance = 1.0 / camera.scale() as f64;
    assert!(
        read.distance(at) < tolerance,
        "the point under the pointer came back {} away from where it was drawn",
        read.distance(at),
    );
}

/// The inverse is the forward turn read backwards and nothing else, which is
/// what stops the two drifting apart.
#[test]
fn the_pointer_arithmetic_is_the_painters_arithmetic_backwards() {
    for &heading in &[0.3, 1.9, -2.6] {
        let offset = dvec2(120.0, -75.0);
        let there = angle::rotate_design(offset, heading);
        let back = angle::unrotate_design(there, heading);
        assert!((back.x - offset.x).abs() < 1e-9);
        assert!((back.y - offset.y).abs() < 1e-9);
    }
}

/// The starfield and anything drawn because it is out there are **never**
/// turned. Nothing in the shape buffer for them carries a rotation, however
/// the ship is pointing.
#[test]
fn the_sky_and_what_is_alongside_do_not_turn_with_the_ship() {
    let mut game = game();
    let mut list = crate::draw::DrawList::new();

    // Every shape is either square to the window or turned with the ship —
    // to the heading, or, for one of the crew, to the heading plus the way
    // they are facing on the deck. There is no other thing: the sky, the
    // station alongside and the void behind them are never turned.
    game.world.ship.heading = 1.1;
    world_paint::paint(&game, &mut list);
    let angles = shape_rotations(&list);
    // What the ship is alongside is drawn square to the window, and its
    // picture has turns of its own that do not move with the ship: whatever
    // was turned at rest is still turned by exactly that much.
    game.world.ship.heading = 0.0;
    let mut at_rest = crate::draw::DrawList::new();
    world_paint::paint(&game, &mut at_rest);
    let fixed = shape_rotations(&at_rest);
    // The room aboard draws turned things of its own — a Bim's body, a
    // pot's handle — and every one of them is turned by the heading on top.
    let room: Vec<f32> = game
        .world
        .aboard
        .room
        .shapes()
        .chunks_exact(bims::draw::STRIDE)
        .map(|shape| shape[5] + 1.1)
        .collect();
    for angle in &angles {
        assert!(
            angle.abs() < 1e-6
                || (angle - 1.1).abs() < 1e-6
                || room.iter().any(|f| (angle - f).abs() < 1e-5)
                || fixed.iter().any(|f| (angle - f).abs() < 1e-6),
            "a shape was drawn at {angle}, which is neither square nor turned with the ship",
        );
    }
    assert!(
        !room.is_empty(),
        "the room aboard should have drawn something"
    );
    assert!(
        angles.iter().any(|a| a.abs() < 1e-6),
        "the starfield should be square to the window",
    );
    assert!(
        angles.iter().any(|a| (a - 1.1).abs() < 1e-6),
        "the ship should be turned to its heading",
    );
}

/// The `rot` field of every shape in the buffer.
fn shape_rotations(list: &crate::draw::DrawList) -> Vec<f32> {
    let slice = unsafe { std::slice::from_raw_parts(list.as_ptr(), list.len()) };
    slice
        .chunks(crate::draw::STRIDE)
        .map(|shape| shape[5])
        .collect()
}

/// The map never turns at all: north is up on it whatever the ship is doing,
/// and the only thing on it that carries a rotation is the marker saying which
/// way the ship is pointing.
#[test]
fn the_map_is_north_up_whatever_the_ship_is_doing() {
    let mut game = game();
    game.set_mode(ViewMode::Map);

    // The pictures of planets and stations carry turns of their own — a gas
    // giant's ring, a belt's rocks — and those are fixed: exactly the same
    // whichever way the ship points. What turns with the ship is the marker
    // for the ship, and nothing else.
    let rotations_at = |game: &mut Game, heading: f64| -> Vec<f32> {
        game.world.ship.heading = heading;
        let mut list = crate::draw::DrawList::new();
        world_paint::paint(game, &mut list);
        shape_rotations(&list)
            .into_iter()
            .filter(|a| a.abs() > 1e-6)
            .collect()
    };
    let at_rest = rotations_at(&mut game, 0.0);
    let turned = rotations_at(&mut game, 2.4);
    let moved: Vec<f32> = turned
        .iter()
        .copied()
        .filter(|a| !at_rest.iter().any(|r| (a - r).abs() < 1e-6))
        .collect();
    assert_eq!(
        moved,
        vec![2.4],
        "only the ship marker should have turned with the ship"
    );
}

/// The camera is centred on the ship, in both views, with or without a pan.
#[test]
fn the_ship_is_in_the_middle_of_both_views() {
    let mut game = game();
    for mode in [ViewMode::Ship, ViewMode::Map] {
        game.set_mode(mode);
        assert!((game.camera().offset_x() - CANVAS.0 / 2.0).abs() < 1e-3);
        assert!((game.camera().offset_y() - CANVAS.1 / 2.0).abs() < 1e-3);
    }

    // And a pan cannot shove it off the edge, however hard it is shoved.
    game.set_mode(ViewMode::Ship);
    game.camera_mut().pan(100_000.0, -100_000.0);
    assert!(game.camera().offset_x() < CANVAS.0);
    assert!(game.camera().offset_y() > 0.0);
}

/// A map click on empty space is a place to fly to; a map click on something
/// is that thing. Both, because the panel offers both and they are different
/// commands.
#[test]
fn a_click_on_the_map_is_a_place_or_a_thing() {
    let mut game = game();
    game.set_mode(ViewMode::Map);

    // The dock the ship is tied to is at the ship's own position, which is the
    // middle of the canvas.
    let picked = game.pick(CANVAS.0 / 2.0, CANVAS.1 / 2.0, 20.0);
    assert!(picked.is_some(), "the station under the ship should pick");

    // Somewhere out in the corner is nothing, and is still somewhere.
    assert!(game.pick(4.0, 4.0, 20.0).is_none());
    let point = game.point_at(4.0, 4.0);
    assert!(point.distance(game.world.ship.position()) > 0.0);
}
