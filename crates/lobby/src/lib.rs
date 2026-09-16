//! The lobby's World tab, as the browser sees it.
//!
//! `web/builder.html` hands this module a seed, a galaxy type and a canvas
//! size; it builds the galaxy — and every system in it, see
//! [`lobby::Lobby`] — and then answers questions about it and draws it. The
//! draw format is the room's twelve floats a shape, so the builder's replay
//! loop is the same as the other two pages'. It is a **separate cdylib**,
//! the third, and imports nothing from `crates/game` or `crates/ship`: the
//! builder is the page in front of the game, and has no ship and no room.
//!
//! # The boundary
//!
//! **No strings cross it.** A star's name is three numbers out of
//! `worldgen::name` and `STAR_WORDS` in `web/builder.js` is where the words
//! are; the kinds of body and station are their discriminants and
//! `BODY_KIND_NAMES` and `STATION_KIND_NAMES` are the host's. The seed and
//! the checksum are `u64`s and cross in the two `u32` halves everything
//! 64-bit does.
//!
//! **Nothing here decides anything.** Which star has a station, where a body
//! is, what a station is called — every answer is `worldgen`'s, and the
//! native server will give the same one. What is here is a camera, a pick
//! under the pointer, and a buffer of shapes.

pub mod diagram;
pub mod draw;
pub mod lobby;
pub mod preview;

use lobby::Lobby;
use worldgen::{GalaxyType, Name, StarSystem};

/// wasm is single-threaded and the host drives every call, so one global is
/// both sufficient and safe in practice. Same arrangement as the other two.
static mut LOBBY: Option<Lobby> = None;
static mut LIST: Option<draw::DrawList> = None;

/// "There is no such thing." What an id export answers when the thing asked
/// about is not there — no star under the pointer, no parent body, no
/// system open. `u32::MAX` rather than zero, because star 0, body 0 and
/// station 0 all exist.
pub const NONE: u32 = u32::MAX;

/// Which of a [`Name`]'s three numbers an export is asked for.
pub const NAME_WORD: u32 = 0;
pub const NAME_NUMBER: u32 = 1;
pub const NAME_PART: u32 = 2;

fn lobby() -> &'static mut Lobby {
    unsafe {
        (*(&raw mut LOBBY))
            .as_mut()
            .expect("lobby_init was not called")
    }
}

fn list() -> &'static mut draw::DrawList {
    unsafe {
        let slot = &mut *(&raw mut LIST);
        if slot.is_none() {
            *slot = Some(draw::DrawList::new());
        }
        slot.as_mut().unwrap()
    }
}

fn galaxy_type(code: u32) -> GalaxyType {
    GalaxyType::ALL
        .get(code as usize)
        .copied()
        .unwrap_or(GalaxyType::SpiralTwoArm)
}

fn seed(hi: u32, lo: u32) -> u64 {
    ((hi as u64) << 32) | lo as u64
}

fn name_field(name: Name, field: u32) -> u32 {
    match field {
        NAME_WORD => name.word as u32,
        NAME_NUMBER => name.number as u32,
        _ => name.part as u32,
    }
}

fn inspected() -> Option<&'static StarSystem> {
    lobby().inspected.as_ref().map(|(_, s)| s)
}

// --- starting up ----------------------------------------------------------

/// Floats per shape in the draw buffer. The host walks the slice with this
/// rather than a constant of its own.
#[unsafe(no_mangle)]
pub extern "C" fn lobby_stride() -> usize {
    draw::STRIDE
}

/// The value every id export uses for "nothing". Exported rather than written
/// down in the host, so there is one copy of it.
#[unsafe(no_mangle)]
pub extern "C" fn lobby_none() -> u32 {
    NONE
}

/// `worldgen::name::NO_NUMBER`: the value a name field carries when the
/// thing has no such number. Same reason as [`lobby_none`].
#[unsafe(no_mangle)]
pub extern "C" fn lobby_no_number() -> u32 {
    worldgen::name::NO_NUMBER as u32
}

/// Open the World tab on a galaxy. `width` and `height` are the preview
/// canvas in CSS pixels.
#[unsafe(no_mangle)]
pub extern "C" fn lobby_init(seed_hi: u32, seed_lo: u32, galaxy: u32, width: f32, height: f32) {
    unsafe {
        LOBBY = Some(Lobby::new(
            seed(seed_hi, seed_lo),
            galaxy_type(galaxy),
            width,
            height,
        ));
    }
}

/// Change the seed or the type. Returns 1 if that made a different galaxy
/// and 0 if it was the one already here, in which case nothing moved.
#[unsafe(no_mangle)]
pub extern "C" fn lobby_set_world(seed_hi: u32, seed_lo: u32, galaxy: u32) -> u32 {
    u32::from(lobby().set_world(seed(seed_hi, seed_lo), galaxy_type(galaxy)))
}

#[unsafe(no_mangle)]
pub extern "C" fn lobby_resize(width: f32, height: f32) {
    lobby().preview.resize(width, height);
}

// --- the galaxy -----------------------------------------------------------

/// One number for the whole galaxy, in two halves. Compared by the harness
/// against the constants `worldgen::fixture` pins natively; a target whose
/// arithmetic drifted fails exactly one of the two.
#[unsafe(no_mangle)]
pub extern "C" fn lobby_galaxy_checksum_hi() -> u32 {
    (lobby().checksum >> 32) as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn lobby_galaxy_checksum_lo() -> u32 {
    lobby().checksum as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn lobby_star_count() -> u32 {
    lobby().galaxy.stars.len() as u32
}

/// A star's position, in galaxy units from the galaxy's centre.
#[unsafe(no_mangle)]
pub extern "C" fn lobby_star_x(star: u32) -> f64 {
    lobby()
        .galaxy
        .star(star)
        .map(|s| s.position.x)
        .unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn lobby_star_y(star: u32) -> f64 {
    lobby()
        .galaxy
        .star(star)
        .map(|s| s.position.y)
        .unwrap_or(0.0)
}

/// `worldgen::StarClass`, as its discriminant.
#[unsafe(no_mangle)]
pub extern "C" fn lobby_star_class(star: u32) -> u32 {
    lobby()
        .galaxy
        .star(star)
        .map(|s| s.star_class as u32)
        .unwrap_or(0)
}

/// One field of a star's name — [`NAME_WORD`], [`NAME_NUMBER`] or
/// [`NAME_PART`]. A star is read as "Word-Number".
#[unsafe(no_mangle)]
pub extern "C" fn lobby_star_name(star: u32, field: u32) -> u32 {
    lobby()
        .galaxy
        .star(star)
        .map(|s| name_field(s.name, field))
        .unwrap_or(NONE)
}

/// Whether the game could start at this star: whether its system, as
/// `Galaxy::system` generates it, has at least one station.
#[unsafe(no_mangle)]
pub extern "C" fn lobby_star_has_station(star: u32) -> u32 {
    u32::from(
        lobby()
            .has_station
            .get(star as usize)
            .copied()
            .unwrap_or(false),
    )
}

// --- the preview ----------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn lobby_pan(dx: f32, dy: f32) {
    lobby().preview.pan(dx, dy);
}

/// Zoom about a point on the canvas. `factor` is worked out by the host.
#[unsafe(no_mangle)]
pub extern "C" fn lobby_zoom(at_x: f32, at_y: f32, factor: f32) {
    lobby().preview.zoom(at_x, at_y, factor);
}

/// The pointer is here. The star under it, if any, becomes the hovered one.
#[unsafe(no_mangle)]
pub extern "C" fn lobby_hover(x: f32, y: f32) {
    lobby().hover(x, y);
}

#[unsafe(no_mangle)]
pub extern "C" fn lobby_leave() {
    lobby().hovered = None;
}

/// The star under the pointer, or [`NONE`].
#[unsafe(no_mangle)]
pub extern "C" fn lobby_hovered() -> u32 {
    lobby().hovered.unwrap_or(NONE)
}

/// Where a star lands on the canvas, in CSS pixels, through the camera as
/// it stands. The page never needs it — the wasm projects everything itself
/// — but `scratchpad/builder-check.mjs` does, to put the pointer on a star
/// and then check the pick is the nearest one.
#[unsafe(no_mangle)]
pub extern "C" fn lobby_star_screen_x(star: u32) -> f32 {
    let lobby = lobby();
    lobby
        .galaxy
        .star(star)
        .map(|s| lobby.preview.to_screen(s.position.x, s.position.y).0)
        .unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn lobby_star_screen_y(star: u32) -> f32 {
    let lobby = lobby();
    lobby
        .galaxy
        .star(star)
        .map(|s| lobby.preview.to_screen(s.position.x, s.position.y).1)
        .unwrap_or(0.0)
}

/// Mark a station as the pending start. It is drawn as a marker on the
/// preview and ringed in the diagram.
#[unsafe(no_mangle)]
pub extern "C" fn lobby_set_spawn(star: u32, station: u32) {
    lobby().spawn = Some((star, station));
}

#[unsafe(no_mangle)]
pub extern "C" fn lobby_clear_spawn() {
    lobby().spawn = None;
}

/// Somebody suggested a star. A ring spreads out from it and fades.
#[unsafe(no_mangle)]
pub extern "C" fn lobby_ping(star: u32) {
    lobby().ping(star);
}

/// Let `dt` seconds go by, for the pings.
#[unsafe(no_mangle)]
pub extern "C" fn lobby_advance(dt: f32) {
    lobby().advance(dt);
}

/// Whether anything on the preview is still moving and wants another frame.
#[unsafe(no_mangle)]
pub extern "C" fn lobby_animating() -> u32 {
    u32::from(lobby().animating())
}

/// Rebuild the shape buffer with the galaxy preview. The shapes are in
/// canvas pixels: the host paints them with no transform but the pixel ratio.
#[unsafe(no_mangle)]
pub extern "C" fn lobby_render() {
    lobby().paint(list());
}

/// Rebuild the shape buffer with the inspected system, fitted to a panel of
/// this size. Empty when no star is open. Same buffer as [`lobby_render`],
/// so the host paints one and then the other.
#[unsafe(no_mangle)]
pub extern "C" fn lobby_render_system(width: f32, height: f32) {
    let lobby = lobby();
    lobby.paint_system(width, height, list());
}

/// Pointer to the current shapes. Only valid until the next render, and
/// memory growth can move it, so re-read it every time.
#[unsafe(no_mangle)]
pub extern "C" fn lobby_draw_ptr() -> *const f32 {
    list().as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn lobby_draw_len() -> usize {
    list().len()
}

// --- the inspected system -------------------------------------------------

/// Open a star's system in the side panel. Generated afresh from
/// `Galaxy::system`, not read from the cache "has a station" is answered
/// from — see [`lobby::Lobby`].
#[unsafe(no_mangle)]
pub extern "C" fn lobby_inspect(star: u32) {
    lobby().inspect(star);
}

/// The star whose system is open, or [`NONE`].
#[unsafe(no_mangle)]
pub extern "C" fn lobby_inspected() -> u32 {
    lobby()
        .inspected
        .as_ref()
        .map(|(id, _)| *id)
        .unwrap_or(NONE)
}

#[unsafe(no_mangle)]
pub extern "C" fn lobby_body_count() -> u32 {
    inspected().map(|s| s.bodies.len() as u32).unwrap_or(0)
}

/// `worldgen::BodyKind`, as its discriminant.
#[unsafe(no_mangle)]
pub extern "C" fn lobby_body_kind(body: u32) -> u32 {
    inspected()
        .and_then(|s| s.body(body))
        .map(|b| b.kind as u32)
        .unwrap_or(0)
}

/// A body's position, in world units from its star.
#[unsafe(no_mangle)]
pub extern "C" fn lobby_body_x(body: u32) -> f64 {
    inspected()
        .and_then(|s| s.body(body))
        .map(|b| b.position.x)
        .unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn lobby_body_y(body: u32) -> f64 {
    inspected()
        .and_then(|s| s.body(body))
        .map(|b| b.position.y)
        .unwrap_or(0.0)
}

/// One field of a body's name. A body borrows its star's word and number —
/// both come back as [`lobby_no_number`] — and is read as the star's name
/// followed by [`NAME_PART`] as a roman numeral.
#[unsafe(no_mangle)]
pub extern "C" fn lobby_body_name(body: u32, field: u32) -> u32 {
    inspected()
        .and_then(|s| s.body(body))
        .map(|b| name_field(b.name, field))
        .unwrap_or(NONE)
}

/// Where the last [`lobby_render_system`] drew a body, in panel pixels, for
/// the host to put a label beside.
#[unsafe(no_mangle)]
pub extern "C" fn lobby_body_panel_x(body: u32) -> f32 {
    lobby()
        .placed
        .bodies
        .get(body as usize)
        .map(|p| p.0)
        .unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn lobby_body_panel_y(body: u32) -> f32 {
    lobby()
        .placed
        .bodies
        .get(body as usize)
        .map(|p| p.1)
        .unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn lobby_station_count() -> u32 {
    inspected().map(|s| s.stations.len() as u32).unwrap_or(0)
}

/// `worldgen::StationKind`, as its discriminant.
#[unsafe(no_mangle)]
pub extern "C" fn lobby_station_kind(station: u32) -> u32 {
    inspected()
        .and_then(|s| s.station(station))
        .map(|st| st.kind as u32)
        .unwrap_or(0)
}

/// The body a station is attached to, or [`NONE`] for one in deep space.
#[unsafe(no_mangle)]
pub extern "C" fn lobby_station_parent(station: u32) -> u32 {
    inspected()
        .and_then(|s| s.station(station))
        .and_then(|st| st.parent_body)
        .unwrap_or(NONE)
}

/// A station's position, in world units from its star — its parent's
/// position and its own orbit added up.
#[unsafe(no_mangle)]
pub extern "C" fn lobby_station_x(station: u32) -> f64 {
    inspected()
        .and_then(|s| s.absolute_position(worldgen::Node::Station(station)))
        .map(|p| p.x)
        .unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn lobby_station_y(station: u32) -> f64 {
    inspected()
        .and_then(|s| s.absolute_position(worldgen::Node::Station(station)))
        .map(|p| p.y)
        .unwrap_or(0.0)
}

/// One field of a station's name. Read as "Word Number", with
/// [`NAME_PART`] as a mark after it.
#[unsafe(no_mangle)]
pub extern "C" fn lobby_station_name(station: u32, field: u32) -> u32 {
    inspected()
        .and_then(|s| s.station(station))
        .map(|st| name_field(st.name, field))
        .unwrap_or(NONE)
}

/// Where the last [`lobby_render_system`] drew a station, in panel pixels.
#[unsafe(no_mangle)]
pub extern "C" fn lobby_station_panel_x(station: u32) -> f32 {
    lobby()
        .placed
        .stations
        .get(station as usize)
        .map(|p| p.0)
        .unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn lobby_station_panel_y(station: u32) -> f32 {
    lobby()
        .placed
        .stations
        .get(station as usize)
        .map(|p| p.1)
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests;
