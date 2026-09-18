//! The arithmetic of the preview, and the one promise the cache makes.
//!
//! **An exception to "the cdylibs' tests are the probes and the harnesses",
//! of the same narrow kind `crates/ship` makes.** The page is checked by
//! `scratchpad/builder-check.mjs` against the real wasm. What is below is
//! pure: whether "has a station" agrees with what generating the system
//! says, whether a point on the canvas maps back to the star it is over, and
//! whether the nearest star is the one picked. Wrong in any of those looks
//! like a map you cannot click on rather than like an error.

use worldgen::GalaxyType;
use worldgen::fixture::{REFERENCE_SEED, reference};

use crate::lobby::Lobby;
use crate::preview::PICK_RADIUS;

const CANVAS: (f32, f32) = (960.0, 640.0);

fn lobby(t: GalaxyType) -> Lobby {
    Lobby::new(REFERENCE_SEED, t, CANVAS.0, CANVAS.1)
}

/// The acceptance criterion, natively: a star reported as having a station
/// has one when its system is generated, and one reported without has none.
#[test]
fn has_station_is_what_generating_the_system_says() {
    for &t in &GalaxyType::ALL {
        let lobby = lobby(t);
        let galaxy = reference(t);
        assert_eq!(lobby.has_station.len(), galaxy.stars.len());
        let mut with = 0;
        for star in &galaxy.stars {
            let generated = !galaxy.system(star.id).unwrap().stations.is_empty();
            assert_eq!(
                lobby.has_station[star.id as usize], generated,
                "{t:?} star {}",
                star.id
            );
            with += usize::from(generated);
        }
        assert!(with > 100, "{t:?}: only {with} stars with a station");
    }
}

/// `can_start` is `has_station` less the stars whose every station is
/// hostile: a subset, a real one, and what generating the system says.
#[test]
fn can_start_is_has_station_less_the_hostile_ones() {
    for &t in &GalaxyType::ALL {
        let lobby = lobby(t);
        let galaxy = reference(t);
        assert_eq!(lobby.can_start.len(), galaxy.stars.len());
        let mut only_hostile = 0;
        for star in &galaxy.stars {
            let system = galaxy.system(star.id).unwrap();
            let open = system.stations.iter().any(|s| !s.hostile);
            assert_eq!(
                lobby.can_start[star.id as usize], open,
                "{t:?} star {}",
                star.id
            );
            if lobby.can_start[star.id as usize] {
                assert!(lobby.has_station[star.id as usize]);
            }
            only_hostile += usize::from(!open && !system.stations.is_empty());
        }
        assert!(
            only_hostile > 0,
            "{t:?}: no star held entirely by the enemy"
        );
    }
}

/// The two ways a start is picked both refuse a hostile station, and
/// `station_hostile` says of the inspected system what the blueprint says.
#[test]
fn a_start_is_never_at_a_hostile_station() {
    let mut lobby = lobby(GalaxyType::Spiral);
    let mut hostile_seen = 0;
    for roll in (0..2_000u64).map(|i| i.wrapping_mul(0x9e37_79b9_7f4a_7c15)) {
        let (star, station) = lobby.random_start(roll).expect("somewhere to start");
        assert!(lobby.can_start_at(star, station), "{star}/{station}");
        lobby.inspect(star);
        assert!(!lobby.station_hostile(station));
        let (_, system) = lobby.inspected.as_ref().unwrap();
        for st in &system.stations {
            assert_eq!(lobby.station_hostile(st.id), st.hostile);
            if st.hostile {
                assert!(!lobby.can_start_at(star, st.id));
                hostile_seen += 1;
            }
        }
    }
    assert!(
        hostile_seen > 0,
        "no hostile station in two thousand systems"
    );
    // Nothing inspected, or a station that is not there, is not hostile.
    lobby.inspect(crate::NONE);
    assert!(!lobby.station_hostile(0));
    assert!(!lobby.can_start_at(crate::NONE, 0));
    assert!(!lobby.can_start_at(0, crate::NONE));
}

/// The diagram rings every hostile station of the inspected system in the
/// enemy red, and nothing else in it.
#[test]
fn the_diagram_rings_the_hostile_stations() {
    use crate::draw::{ENEMY, STRIDE};
    let mut lobby = lobby(GalaxyType::Round);
    let mut list = crate::draw::DrawList::new();
    let mut ringed = 0;
    for star in 0..lobby.galaxy.stars.len() as u32 {
        lobby.inspect(star);
        lobby.paint_system(280.0, 240.0, &mut list);
        let (_, system) = lobby.inspected.as_ref().unwrap();
        let hostile = system.stations.iter().filter(|s| s.hostile).count();
        let red = list
            .shapes()
            .chunks(STRIDE)
            .filter(|s| s[8] == ENEMY.r && s[9] == ENEMY.g && s[10] == ENEMY.b)
            .count();
        assert_eq!(red, hostile, "star {star}");
        ringed += hostile;
    }
    assert!(ringed > 0);
}

/// The designations alone would get this wrong, which is why the cache
/// exists: some of the stars with a station were never promised one.
#[test]
fn the_designations_are_not_the_whole_answer() {
    let lobby = lobby(GalaxyType::Spiral);
    let unpromised = lobby
        .galaxy
        .stars
        .iter()
        .filter(|s| lobby.has_station[s.id as usize])
        .filter(|s| lobby.galaxy.designation_for(s.id).is_none())
        .count();
    assert!(
        unpromised > 50,
        "{unpromised} unpromised stars with a station"
    );
}

#[test]
fn the_checksum_is_the_fixture_s() {
    for &t in &GalaxyType::ALL {
        assert_eq!(
            lobby(t).checksum,
            worldgen::fixture::REFERENCE_CHECKSUMS[t as usize],
            "{t:?}"
        );
    }
}

/// Every star lands on the canvas at the fit, and comes back to itself.
#[test]
fn a_point_on_the_canvas_maps_back_to_the_galaxy_position_it_is_over() {
    let lobby = lobby(GalaxyType::Round);
    for star in &lobby.galaxy.stars {
        let (sx, sy) = lobby.preview.to_screen(star.position.x, star.position.y);
        assert!(
            sx >= 0.0 && sx <= CANVAS.0 && sy >= 0.0 && sy <= CANVAS.1,
            "star {} at ({sx}, {sy})",
            star.id
        );
        let (gx, gy) = lobby.preview.to_galaxy(sx, sy);
        // A pixel is a good many light years at the fit; the round trip is
        // exact to well under one.
        let tolerance = 1.0 / lobby.preview.scale() as f64;
        assert!((gx - star.position.x).abs() < tolerance, "star {}", star.id);
        assert!((gy - star.position.y).abs() < tolerance, "star {}", star.id);
    }
}

/// North is up: a star with a bigger `y` is higher on the canvas.
#[test]
fn north_is_up() {
    let lobby = lobby(GalaxyType::Round);
    let (_, top) = lobby.preview.to_screen(0.0, 1000.0);
    let (_, bottom) = lobby.preview.to_screen(0.0, -1000.0);
    assert!(top < bottom);
}

/// Hovering exactly on a star picks it, and hovering just beside it still
/// does — within the radius, the nearest wins.
#[test]
fn the_pick_is_the_nearest_star_within_reach() {
    let lobby = lobby(GalaxyType::SpiralTwoArm);
    let stars = &lobby.galaxy.stars;
    let mut picked = 0;
    for star in stars.iter().step_by(37) {
        let (sx, sy) = lobby.preview.to_screen(star.position.x, star.position.y);
        let Some(id) = lobby.preview.pick(stars, sx, sy) else {
            panic!("nothing under star {}", star.id);
        };
        // In a dense core another star can be closer to the pixel the
        // centre rounded to; what is picked is then the nearer one, and
        // that is right.
        let (px, py) = lobby
            .preview
            .to_screen(stars[id as usize].position.x, stars[id as usize].position.y);
        let d_picked = (px - sx).hypot(py - sy);
        assert!(
            d_picked <= 1e-3 || id != star.id,
            "star {} picked {id}",
            star.id
        );
        picked += 1;
    }
    assert!(picked > 20);

    // Nothing within reach is nothing.
    let far = lobby
        .preview
        .pick(stars, -PICK_RADIUS * 4.0, -PICK_RADIUS * 4.0);
    assert_eq!(far, None);
}

/// Zooming about a point keeps that point where it was.
#[test]
fn zooming_holds_the_point_under_the_pointer_still() {
    let mut lobby = lobby(GalaxyType::Elliptical);
    let at = (300.0, 200.0);
    let before = lobby.preview.to_galaxy(at.0, at.1);
    lobby.preview.zoom(at.0, at.1, 4.0);
    let after = lobby.preview.to_galaxy(at.0, at.1);
    assert!((before.0 - after.0).abs() < 1.0, "{before:?} vs {after:?}");
    assert!((before.1 - after.1).abs() < 1.0, "{before:?} vs {after:?}");
    assert!(lobby.preview.zoom_level() > 0.0);
}

/// Changing the seed forgets the spawn and the inspection; leaving it alone
/// forgets nothing.
#[test]
fn a_new_galaxy_forgets_what_was_chosen_in_the_old_one() {
    let mut lobby = lobby(GalaxyType::Round);
    lobby.spawn = Some((3, 0));
    lobby.inspect(3);
    assert!(!lobby.set_world(REFERENCE_SEED, GalaxyType::Round));
    assert_eq!(lobby.spawn, Some((3, 0)));
    assert!(lobby.set_world(REFERENCE_SEED + 1, GalaxyType::Round));
    assert_eq!(lobby.spawn, None);
    assert!(lobby.inspected.is_none());
    assert_ne!(lobby.checksum, worldgen::fixture::REFERENCE_CHECKSUMS[3]);
}

/// The diagram fits the panel: everything drawn is inside it.
#[test]
fn the_diagram_stays_inside_the_panel() {
    let mut lobby = lobby(GalaxyType::Spiral);
    let mut list = crate::draw::DrawList::new();
    let (w, h) = (280.0, 240.0);
    let mut looked = 0;
    for star in lobby
        .galaxy
        .stars
        .iter()
        .step_by(23)
        .map(|s| s.id)
        .collect::<Vec<_>>()
    {
        lobby.inspect(star);
        lobby.paint_system(w, h, &mut list);
        for &(x, y) in lobby.placed.bodies.iter().chain(&lobby.placed.stations) {
            assert!(
                x >= 0.0 && x <= w && y >= 0.0 && y <= h,
                "star {star}: ({x}, {y})"
            );
            looked += 1;
        }
    }
    assert!(looked > 40);
}
