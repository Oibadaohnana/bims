// The crate's modules, declared once. Every probe `include!`s this rather than
// carrying its own copy: when they each had one, adding a new crates/game/src/*.rs meant
// every probe but the newest failed to *compile*, and rustc failing leaves the
// previous binary in place — so running one printed a confident pass from
// stale code. Three separate rounds of that happened.
#[path = "../crates/game/src/bath.rs"]
mod bath;
#[path = "../crates/game/src/bim.rs"]
mod bim;
#[path = "../crates/game/src/character.rs"]
mod character;
#[path = "../crates/game/src/clock.rs"]
mod clock;
#[path = "../crates/game/src/combat.rs"]
mod combat;
#[path = "../crates/game/src/door.rs"]
mod door;
#[path = "../crates/game/src/dish.rs"]
mod dish;
#[path = "../crates/game/src/draw.rs"]
mod draw;
#[path = "../crates/game/src/filth.rs"]
mod filth;
#[path = "../crates/game/src/galley.rs"]
mod galley;
#[path = "../crates/game/src/game.rs"]
mod game;
#[path = "../crates/game/src/health.rs"]
mod health;
#[path = "../crates/game/src/hydro.rs"]
mod hydro;
#[path = "../crates/game/src/manager.rs"]
mod manager;
#[path = "../crates/game/src/memory.rs"]
mod memory;
#[path = "../crates/game/src/math.rs"]
mod math;
#[path = "../crates/game/src/nav.rs"]
mod nav;
#[path = "../crates/game/src/needs.rs"]
mod needs;
#[path = "../crates/game/src/rng.rs"]
mod rng;
#[path = "../crates/game/src/room.rs"]
mod room;
#[path = "../crates/game/src/schedule.rs"]
mod schedule;
#[path = "../crates/game/src/sight.rs"]
mod sight;
#[path = "../crates/game/src/task.rs"]
mod task;
#[path = "../crates/game/src/work.rs"]
mod work;
#[path = "../crates/game/src/social.rs"]
mod social;
// The shared `time` crate, stood over the same file as a plain module. The
// game reaches it as `crate::time` for exactly this reason: a probe links
// nothing, so an extern crate here would mean a build step per probe.
#[path = "../crates/time/src/lib.rs"]
mod time;
