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

/// The sky streams past a ship under way and stands still otherwise. It is
/// a picture clock on the *world's* clock: a step with the ship at rest
/// moves nothing, a frame with no step in it moves nothing however fast
/// the ship is going — a pause holds the stars — and the near layer streams
/// further than the far one, opposite the way the ship is going.
#[test]
fn the_sky_streams_on_the_world_s_clock_and_only_under_way() {
    use crate::starfield::Starfield;
    use worldgen::math::dvec2;

    let mut game = game();
    // Docked and still: frames and steps go by and the sky does not move.
    game.stream_sky();
    for _ in 0..10 {
        game.world.step(&[]);
        game.stream_sky();
    }
    assert!(
        game.stars.slid.iter().all(|s| s.x == 0.0 && s.y == 0.0),
        "the sky moved with the ship at rest: {:?}",
        game.stars.slid.map(|s| (s.x, s.y))
    );

    // Under way, to the east, on a clock that moved a minute: the near layer
    // streams west, and further than the far one.
    let mut stars = Starfield::new(1);
    let east = dvec2(3_000.0, 0.0);
    stars.advance(east, 100.0);
    assert!(
        stars.slid.iter().all(|s| s.x == 0.0 && s.y == 0.0),
        "the first frame is a reading, not a stream"
    );
    stars.advance(east, 101.0);
    let near = stars.slid[0];
    let far = stars.slid[2];
    assert!(near.x > 0.0 && near.y == 0.0, "near layer {near:?}");
    // Wrapped to the tile, so "west" reads as a large positive `x`.
    let west_by = |s: worldgen::math::DVec2| crate::starfield::FIELD - s.x;
    assert!(
        west_by(near) > 0.0 && west_by(near) < 100.0,
        "{}",
        west_by(near)
    );
    assert!(
        west_by(far) < west_by(near),
        "the far layer should stream less: {far:?} against {near:?}"
    );
    // The same velocity on a clock that has not moved: a pause.
    let held = stars.slid;
    stars.advance(east, 101.0);
    assert!(
        stars
            .slid
            .iter()
            .zip(held.iter())
            .all(|(a, b)| a.x == b.x && a.y == b.y)
    );
    // Faster is further, and the mapping is monotonic even past the clamp.
    let mut slow = Starfield::new(1);
    slow.advance(dvec2(30.0, 0.0), 0.0);
    slow.advance(dvec2(30.0, 0.0), 1.0);
    assert!(west_by(slow.slid[0]) > 0.0 && west_by(slow.slid[0]) < west_by(near));
    // North on the system's axes is up the screen, so a ship going north
    // has the stars going down: positive `y`, unwrapped.
    let mut north = Starfield::new(1);
    north.advance(dvec2(0.0, 3_000.0), 0.0);
    north.advance(dvec2(0.0, 3_000.0), 1.0);
    assert!(
        north.slid[0].y > 0.0 && north.slid[0].y < 100.0,
        "{:?}",
        (north.slid[0].x, north.slid[0].y)
    );
}

/// The `rot` field of every shape in the buffer.
fn shape_rotations(list: &crate::draw::DrawList) -> Vec<f32> {
    list.shapes()
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
    let mut moved: Vec<f32> = turned
        .iter()
        .copied()
        .filter(|a| !at_rest.iter().any(|r| (a - r).abs() < 1e-6))
        .collect();
    // The marker is a hull and two fins: three rectangles, at the heading
    // and leaning out either side of it. Nothing else moved.
    moved.sort_by(|a, b| a.total_cmp(b));
    let marker = [
        2.4 - crate::hull::FIN_LEAN,
        2.4,
        2.4 + crate::hull::FIN_LEAN,
    ];
    assert_eq!(
        moved.len(),
        3,
        "only the ship marker should have turned: {moved:?}"
    );
    for (a, b) in moved.iter().zip(marker) {
        assert!(
            (a - b).abs() < 1e-5,
            "the marker turned to {moved:?}, not {marker:?}"
        );
    }
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

    // And a pan cannot shove it off the edge, however hard it is shoved —
    // once the view is tethered; it opens free, and free goes anywhere.
    game.set_mode(ViewMode::Ship);
    game.set_follow(true);
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

/// What is lit follows the plan through every phase: nothing at the dock,
/// the thrusters through the turn, the forward engines through the burn,
/// nothing through the flip, the *same* forward engines through the brake —
/// the flyer has no backward engine, so it turns round to stop — and nothing
/// once it has arrived. And the picture says so: there is exhaust in the
/// buffer exactly when something is lit.
#[test]
fn the_exhaust_follows_the_plan() {
    use flight::{Phase, Target};
    use world::world::Command;

    let mut game = game();
    // Somewhere off to one side, so there is a real turn to make first —
    // and the crew member at the helm, since the ship is flown from there.
    let here = game.world.ship.position();
    game.world.man_the_helm_for_probe(0);
    game.send(Command::Confirm {
        slot: 0,
        target: Target::Point(dvec2(here.x + 30_000.0, here.y + 7_000.0)),
    });

    // The exhaust on its own, rather than the whole frame: the room and the
    // starfield both change from one frame to the next, so a count of
    // everything says nothing about the flame.
    let exhaust = |game: &Game| -> usize {
        let design = &game.world.ship.design;
        let centre = game.world.ship.dynamics.centre_of_mass;
        let mut list = crate::draw::DrawList::default();
        crate::hull::exhaust(
            &mut list,
            design,
            &design.grid(),
            game.firing(),
            (centre.x as f32, centre.y as f32),
            game.frame,
        );
        list.len() / crate::draw::STRIDE
    };
    assert_eq!(game.firing(), crate::hull::Firing::NONE);
    assert_eq!(exhaust(&game), 0, "nothing burns at the dock");

    let mut seen = std::collections::BTreeSet::new();
    let mut under_way = false;
    for _ in 0..400_000 {
        game.step();
        let Some(state) = game.world.trip_state() else {
            // Casting off and pushing off the berth come first, and the
            // engines are cold through both; the trip is over once there
            // has been one.
            assert_eq!(game.firing(), crate::hull::Firing::NONE);
            if under_way {
                break;
            }
            continue;
        };
        under_way = true;
        let firing = game.firing();
        match state.phase {
            Phase::Align => {
                assert!(
                    !firing.forward && !firing.backward,
                    "no engine while aligning"
                );
                assert!(firing.alpha != 0.0, "the thrusters push through a turn");
            }
            Phase::Burn => {
                assert!(
                    firing.forward && !firing.backward,
                    "the forward engines burn"
                );
                assert_eq!(firing.alpha, 0.0);
            }
            Phase::Flip => {
                assert!(!firing.forward && !firing.backward, "a flip is a coast");
            }
            Phase::Brake => {
                assert!(
                    firing.forward && !firing.backward,
                    "the brake after a flip is the forward engines again"
                );
                assert_eq!(firing.alpha, 0.0);
            }
            Phase::Arrived => {}
        }
        if seen.insert(state.phase.code()) {
            // The first frame of each phase: the picture has exhaust in it
            // exactly when something is lit. A flip coasts, so it draws
            // what the dock draws — the ship and nothing behind it.
            let lit = firing.forward || firing.backward || firing.alpha != 0.0;
            let drawn = exhaust(&game);
            assert_eq!(
                drawn > 0,
                lit,
                "phase {:?}: {drawn} shapes of exhaust, lit {lit}",
                state.phase,
            );
        }
    }
    assert!(
        seen.len() >= 4,
        "the trip should have aligned, burnt, flipped and braked: {seen:?}"
    );
    assert!(
        game.world.trip_state().is_none(),
        "the trip should have ended"
    );
    assert_eq!(game.firing(), crate::hull::Firing::NONE);
    assert_eq!(exhaust(&game), 0, "nothing burns once it is there");
}

/// The mated airlocks are a door: shut while nobody is near the passage,
/// open once somebody stands at it, and shut again when they have gone —
/// eased, so the picture never jumps. A picture clock only: the passage is
/// walkable whatever the door looks like.
#[test]
fn the_airlock_opens_for_whoever_comes_to_it_and_shuts_behind_them() {
    let mut game = game();
    assert!(game.mated_airlock().is_some(), "the fixture docks");
    for _ in 0..60 {
        game.tick_airlock();
    }
    assert_eq!(game.airlock_ajar, 0.0, "open with nobody at it");

    // Stand the crew member in the passage: the ship's door face, in the
    // room's units.
    let port = shipdesign::port(&game.world.ship.design).unwrap();
    let (fx, fy) = port.face();
    let offset = game.world.aboard.offset;
    let at = bims::math::vec2((fx + offset.x) as f32, (fy + offset.y) as f32);
    game.world.aboard.room.put_for_probe(0, at);
    let mut opening = Vec::new();
    for _ in 0..40 {
        game.tick_airlock();
        opening.push(game.airlock_ajar);
    }
    assert!(
        opening.windows(2).all(|w| w[1] >= w[0]),
        "it should open, not flicker"
    );
    assert!(
        opening[0] > 0.0 && opening[0] < 1.0,
        "it should ease, not jump"
    );
    assert_eq!(game.airlock_ajar, 1.0, "wide open with somebody at it");

    // And away again: the far end of the ship.
    let far = bims::math::vec2(
        (offset.x + 3.0 * 52.0) as f32,
        (offset.y + 3.0 * 52.0) as f32,
    );
    game.world.aboard.room.put_for_probe(0, far);
    for _ in 0..60 {
        game.tick_airlock();
    }
    assert_eq!(game.airlock_ajar, 0.0, "shut once they have gone");
}

/// The ship view is about the crew member the player steers: wherever they
/// stand — aboard, or across the airlock on the station — they are in the
/// middle of the window, the pan is measured from them and cannot take
/// them off the edge, and a zoom keeps what is under the pointer under it.
#[test]
fn the_camera_follows_the_crew_member_the_player_steers() {
    let mut game = game();
    // The view opens free; this is about it tethered.
    game.set_follow(true);
    let middle = (CANVAS.0 / 2.0, CANVAS.1 / 2.0);
    let pixel = |game: &Game| {
        let (x, y) = world_paint::crew_on_screen(game, 0);
        let cam = &game.ship_view;
        (
            cam.offset_x() + x * cam.scale(),
            cam.offset_y() + y * cam.scale(),
        )
    };

    game.follow_player();
    let at = pixel(&game);
    assert!(
        (at.0 - middle.0).abs() < 1e-3 && (at.1 - middle.1).abs() < 1e-3,
        "{at:?}"
    );

    // Across the airlock: far from the ship's centre, still in the middle.
    let offset = game.world.aboard.offset;
    let side = game.world.ship.design.build_area as f64 * 52.0;
    let far = bims::math::vec2(
        (offset.x + side + 20.0 * 52.0) as f32,
        (offset.y + side / 2.0) as f32,
    );
    let landed = game.world.aboard.room.put_for_probe(0, far);
    assert!(
        (landed - far).len() < 60.0,
        "the probe could not stand there: {landed:?}"
    );
    game.follow_player();
    let at = pixel(&game);
    assert!(
        (at.0 - middle.0).abs() < 1e-3 && (at.1 - middle.1).abs() < 1e-3,
        "{at:?}"
    );
    // And the ship's own centre is off to one side, a long way.
    assert!((game.ship_view.offset_x() - middle.0).abs() > 10.0 * 52.0 * game.ship_view.scale());

    // Zoomed in about a corner of the window, the point under the pointer
    // stays under it; the crew member stays where the pan left them.
    let corner = (100.0, 80.0);
    let before = game.ship_view.to_view(corner.0, corner.1);
    game.ship_view.zoom(corner.0, corner.1, 2.0);
    let after = game.ship_view.to_view(corner.0, corner.1);
    assert!((before.0 - after.0).abs() < 1e-3 && (before.1 - after.1).abs() < 1e-3);

    // Panned as far as it goes, they are still a sliver inside the edge.
    game.ship_view.pan(-100_000.0, -100_000.0);
    game.follow_player();
    let at = pixel(&game);
    assert!(at.0 > 0.0 && at.1 > 0.0, "panned off the edge: {at:?}");

    // Let go, the view stays exactly where it was — nothing on screen
    // moves for the flip of a switch.
    let held = (game.ship_view.offset_x(), game.ship_view.offset_y());
    game.set_follow(false);
    game.follow_player();
    let loose = (game.ship_view.offset_x(), game.ship_view.offset_y());
    assert!(
        (held.0 - loose.0).abs() < 1e-3 && (held.1 - loose.1).abs() < 1e-3,
        "letting go moved the view: {held:?} -> {loose:?}"
    );
    // And the crew member walking off does not drag it after them.
    let back = bims::math::vec2(offset.x as f32 + 100.0, offset.y as f32 + 100.0);
    game.world.aboard.room.put_for_probe(0, back);
    game.follow_player();
    let still = (game.ship_view.offset_x(), game.ship_view.offset_y());
    assert!((still.0 - loose.0).abs() < 1e-3 && (still.1 - loose.1).abs() < 1e-3);
    // A free camera goes wherever it is dragged: a pan that would have
    // been clamped carries the whole way, and the subject is off the edge.
    game.ship_view.pan(-100_000.0, 0.0);
    assert!(
        (game.ship_view.offset_x() - loose.0 + 100_000.0).abs() < 1.0,
        "a free pan was clamped: {} -> {}",
        loose.0,
        game.ship_view.offset_x()
    );
    assert!(
        pixel(&game).0 < 0.0,
        "the subject should be off the edge now"
    );
    // Zooming still keeps what is under the pointer under it.
    let before = game.ship_view.to_view(corner.0, corner.1);
    game.ship_view.zoom(corner.0, corner.1, 0.5);
    let after = game.ship_view.to_view(corner.0, corner.1);
    assert!((before.0 - after.0).abs() < 1e-2 && (before.1 - after.1).abs() < 1e-2);
    // Tethered again, the next frame snaps back to the crew member.
    game.set_follow(true);
    game.follow_player();
    let at = pixel(&game);
    assert!(
        (at.0 - middle.0).abs() < 1e-3 && (at.1 - middle.1).abs() < 1e-3,
        "following again did not bring them back: {at:?}"
    );
}

/// The blueprint in hand is the world's answer about the tile under the
/// pointer, asked once per tile and kept: the same tile again is the same
/// answer without the ship being validated again, a different tile or a
/// turn is asked afresh, and the tool put down forgets it. A site laid out
/// is found under its tiles.
#[test]
fn the_blueprint_asks_the_world_once_a_tile_and_finds_a_site_under_the_pointer() {
    use shipdesign::parts::{PartKind, Rotation};
    use world::world::Command;
    let mut game = game();
    assert!(game.ghost_check().is_none(), "no tool in hand");
    game.set_placing(Some((PartKind::Wall, Rotation::R0)));
    assert!(game.ghost_check().is_none(), "no tile under the pointer");

    // A deck tile the wall may stand on: the flyer's deck, away from the
    // fixtures. Found rather than named, so the fixture may move.
    let design = game.world.ship.design.clone();
    let grid = design.grid();
    let tile = (1..design.build_area as i32 - 1)
        .flat_map(|y| (1..design.build_area as i32 - 1).map(move |x| (x, y)))
        .find(|&t| {
            grid.has_floor(t)
                && game
                    .world
                    .can_place_site(PartKind::Wall, (t.0 as u32, t.1 as u32), Rotation::R0)
                    .is_ok()
        })
        .expect("somewhere on the deck a wall may go");
    game.hover = Some(tile);
    assert_eq!(game.ghost_check(), Some(Ok(())));
    assert!(game.ghost_ok());
    assert_eq!(game.ghost_answer(), Some(Ok(())));
    // Off the hull it is refused, and the refusal is the rules' own.
    game.hover = Some((-1, -1));
    assert!(matches!(
        game.ghost_check(),
        Some(Err(world::SiteRefusal::WontFit(_)))
    ));
    assert!(!game.ghost_ok());
    game.hover = Some(tile);
    assert_eq!(game.ghost_check(), Some(Ok(())));

    // Laid out, and found under the pointer.
    assert!(game.site_at(tile).is_none());
    game.send(Command::PlaceSite {
        slot: 0,
        kind: PartKind::Wall,
        origin: (tile.0 as u32, tile.1 as u32),
        rotation: Rotation::R0,
    });
    game.step();
    let site = game.site_at(tile).expect("the site under the pointer");
    assert_eq!(site.kind, PartKind::Wall);
    // And the same tile is now refused: the site is already there.
    assert!(matches!(
        game.ghost_check(),
        Some(Err(world::SiteRefusal::WontFit(_)))
    ));
    game.set_placing(None);
    assert!(game.ghost_check().is_none());
    assert!(!game.ghost_ok());
}
