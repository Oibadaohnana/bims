// The crate's modules, declared once. Every probe `include!`s this rather than
// carrying its own copy: when they each had one, adding a new src/*.rs meant
// every probe but the newest failed to *compile*, and rustc failing leaves the
// previous binary in place — so running one printed a confident pass from
// stale code. Three separate rounds of that happened.
#[path = "../src/bath.rs"]
mod bath;
#[path = "../src/bim.rs"]
mod bim;
#[path = "../src/character.rs"]
mod character;
#[path = "../src/clock.rs"]
mod clock;
#[path = "../src/dish.rs"]
mod dish;
#[path = "../src/draw.rs"]
mod draw;
#[path = "../src/filth.rs"]
mod filth;
#[path = "../src/game.rs"]
mod game;
#[path = "../src/health.rs"]
mod health;
#[path = "../src/hydro.rs"]
mod hydro;
#[path = "../src/manager.rs"]
mod manager;
#[path = "../src/memory.rs"]
mod memory;
#[path = "../src/math.rs"]
mod math;
#[path = "../src/nav.rs"]
mod nav;
#[path = "../src/needs.rs"]
mod needs;
#[path = "../src/rng.rs"]
mod rng;
#[path = "../src/room.rs"]
mod room;
#[path = "../src/schedule.rs"]
mod schedule;
#[path = "../src/task.rs"]
mod task;
#[path = "../src/work.rs"]
mod work;
#[path = "../src/social.rs"]
mod social;
// The shared `time` crate, stood over the same file as a plain module. The
// game reaches it as `crate::time` for exactly this reason: a probe links
// nothing, so an extern crate here would mean a build step per probe.
#[path = "../crates/time/src/lib.rs"]
mod time;
