//! Whether a design is a ship the crew could actually live on.
//!
//! [`validate`] answers in [`Issue`]s rather than a bool, because a player
//! needs to be shown *where* the fault is. An `Error` blocks Accept; a
//! `Warning` is the design saying what it will be like to live with.
//!
//! # Where the required list comes from
//!
//! The seven fixtures below are **exactly what the test room's chains need
//! today**, and nothing else:
//!
//! - a meal is `ColdStore` → `Worktop` → `Hob` → `Table` (in a `Chair`) →
//!   `Dishwasher`;
//! - a night's sleep is a `Bunk`;
//! - a trip to the heads is a `Toilet` and then a `Basin`.
//!
//! `Bunk` and `Chair` are counted against the crew rather than merely
//! required, because two Bims cannot share one bed or one seat.
//!
//! That list is a mirror, not a design. **If stage 5 changes what a chain
//! walks to, this list changes with it** — a ship validated against a stale
//! list is a ship whose crew starve standing in front of the fixture that was
//! never required.
//!
//! # Radiation
//!
//! [`exposure`] is the other half of this file and it is not a list of
//! fixtures at all: it is a flood fill from **outside the ship** through
//! everything that does not shield. What it reaches and finds a part in is a
//! tile the outside can see into, and a Bim standing there is a Bim being
//! irradiated.
//!
//! It is a **warning**, not an error, and it is the loudest thing on the
//! page. A ship with a hole in it is a ship you can still fly, right up until
//! it is not; refusing to let a player accept one would be the design phase
//! having an opinion about how to play, and saying nothing would be letting
//! them kill the crew by accident. So: first in the list, tinted on the deck
//! whether or not anybody is pointing at the row, and never a refusal.

use physics::{Facing, ResourceId};

use crate::design::{Grid, ShipDesign};
use crate::parts::{Layer, PartKind};

/// Whether an issue stops the design being accepted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum Severity {
    Error = 0,
    Warning = 1,
}

/// What is wrong.
///
/// The discriminants cross the wasm boundary and index `ISSUE_LINES` in
/// `web/ship.js`; they are written out and not renumbered. Errors are
/// numbered from 1 and warnings from 20, so the two never have to be told
/// apart by arithmetic — but [`Issue::severity`] is what decides, not the
/// range.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum IssueCode {
    /// The ship is in more than one piece.
    Disconnected = 1,
    TooFewBunks = 2,
    TooFewChairs = 3,
    NoTable = 4,
    NoColdStore = 5,
    NoWorktop = 6,
    NoHob = 7,
    NoDishwasher = 8,
    NoToilet = 9,
    NoBasin = 10,
    /// Somewhere a Bim has to stand is off the ship, has no deck, or has
    /// something solid in it.
    UseSpotBlocked = 11,
    /// Two parts nobody could walk between.
    UseSpotsCutOff = 12,

    /// Nothing to push the ship anywhere.
    NoEngine = 20,
    /// Engines, but not on every axis — a ship that cannot stop, or cannot
    /// steer.
    NoEngineOnAxis = 21,
    /// No hydroponic bay. The food aboard is all the food there will be.
    NoHydroBay = 22,
    NoBroomLocker = 23,
    /// The outside can see in. Tiles the radiation reaches — see
    /// [`exposure`], and the module note above for why this is a warning
    /// rather than a refusal.
    RadiationExposure = 24,
    /// Nothing to eat aboard. The bay grows more, but it grows it slowly and
    /// a crew that launches with an empty cold store is a crew eating
    /// whatever the bay has when it runs out.
    NoFoodAboard = 25,
    /// Nowhere to fly the ship from.
    NoHelm = 26,
}

impl IssueCode {
    pub fn code(self) -> u32 {
        self as u32
    }
}

/// One fault, and where to point at it.
///
/// `parts` and `tiles` are how the page shows it: the tiles get a highlight.
/// Neither carries words — `ISSUE_LINES` in `web/ship.js` is where the
/// sentences live, the same way `MEMORY_LINES` holds the diary's.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Issue {
    pub severity: Severity,
    pub code: u32,
    pub parts: Vec<u32>,
    pub tiles: Vec<(u32, u32)>,
}

impl Issue {
    fn error(code: IssueCode, parts: Vec<u32>, tiles: Vec<(u32, u32)>) -> Issue {
        Issue {
            severity: Severity::Error,
            code: code.code(),
            parts,
            tiles,
        }
    }

    fn warning(code: IssueCode) -> Issue {
        Issue {
            severity: Severity::Warning,
            code: code.code(),
            parts: Vec::new(),
            tiles: Vec::new(),
        }
    }
}

/// The fixtures a ship must carry at least one of, each with the error it
/// raises by its absence.
///
/// A table rather than a run of `if`s so that the check and the test that
/// pins it read the same list — the failure this shape prevents is a required
/// part nobody remembered to give an error code.
pub static REQUIRED: [(PartKind, IssueCode); 7] = [
    (PartKind::Table, IssueCode::NoTable),
    (PartKind::ColdStore, IssueCode::NoColdStore),
    (PartKind::Worktop, IssueCode::NoWorktop),
    (PartKind::Hob, IssueCode::NoHob),
    (PartKind::Dishwasher, IssueCode::NoDishwasher),
    (PartKind::Toilet, IssueCode::NoToilet),
    (PartKind::Basin, IssueCode::NoBasin),
];

/// Whether a body can stand in this tile: deck under it, and either nothing
/// on top or something that does not block — a door, a chair.
pub fn walkable(design: &ShipDesign, grid: &Grid, tile: (i32, i32)) -> bool {
    if !grid.has_floor(tile) {
        return false;
    }
    let object = grid.get(Layer::Object, tile);
    object == 0
        || design
            .part(object)
            .is_none_or(|p| !p.kind.def().blocks_movement)
}

/// Everything wrong with a design, worst first is not promised — the host
/// sorts. An empty vector is a ship that can be accepted.
pub fn validate(design: &ShipDesign, crew_count: u32) -> Vec<Issue> {
    let grid = design.grid();
    let mut issues = Vec::new();

    // First, and deliberately: it is the one fault on this list that kills
    // people. The host paints the list in the order it arrives in.
    radiation(design, &grid, &mut issues);
    connectivity(design, &grid, &mut issues);
    crew(design, crew_count, &mut issues);
    required(design, &mut issues);
    // Reachability is only asked once every use spot is somewhere a body
    // could be. A spot with a wall in it is not in the walkable set at all,
    // so asking would report the same fault a second time under a different
    // name.
    if !use_spots(design, &grid, &mut issues) {
        reachability(design, &grid, &mut issues);
    }
    engines(design, &mut issues);
    comforts(design, &mut issues);

    issues
}

/// Where the outside can see in.
///
/// A flood fill from a **one-tile ring outside the build area**, 4-neighbour
/// only, through every tile whose object-layer part does not shield. A tile
/// the fill reaches and that holds any part at all is exposed.
///
/// Three things about that are load-bearing:
///
/// - **The ring is outside.** Starting from the edge tiles of the build area
///   would make a ship built flush to the edge look sealed by the edge, which
///   is a wall that does not exist.
/// - **4-neighbour, never 8.** Two shielding parts that touch only at a
///   corner seal that corner. Eight-way would leak through every diagonal
///   join and no hull drawn by hand would ever pass.
/// - **A shielding part's own tiles are never exposed**, because the fill
///   cannot enter them. That falls out of the rule rather than being a
///   special case: the hull is what is keeping the radiation out, so the hull
///   is not what is being irradiated.
///
/// A door does **not** shield. Neither does a plain internal wall. What does
/// is in `PartDef::shields`.
pub fn exposure(design: &ShipDesign) -> ExposureMap {
    let side = design.build_area;
    let grid = design.grid();

    // The fill runs over a grid one tile bigger all round, so the ring has
    // somewhere to be. Offsets by one throughout; `at` is the only place that
    // arithmetic happens.
    let span = side as usize + 2;
    let at = |(x, y): (i32, i32)| ((y + 1) as usize) * span + (x + 1) as usize;

    let shields = |tile: (i32, i32)| {
        let id = grid.get(Layer::Object, tile);
        id != 0 && design.part(id).is_some_and(|p| p.kind.def().shields)
    };

    let mut seen = vec![false; span * span];
    let mut stack: Vec<(i32, i32)> = Vec::new();
    let edge = side as i32;
    for i in -1..=edge {
        for start in [(i, -1), (i, edge), (-1, i), (edge, i)] {
            if !seen[at(start)] {
                seen[at(start)] = true;
                stack.push(start);
            }
        }
    }

    let mut exposed = vec![false; (side as usize) * (side as usize)];
    let mut tiles: Vec<(u32, u32)> = Vec::new();
    while let Some((x, y)) = stack.pop() {
        for (dx, dy) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
            let next = (x + dx, y + dy);
            if next.0 < -1 || next.1 < -1 || next.0 > edge || next.1 > edge {
                continue;
            }
            if seen[at(next)] || shields(next) {
                continue;
            }
            seen[at(next)] = true;
            stack.push(next);
        }
    }

    // In row order, so the list is stable and reads the way the ship does.
    for y in 0..side {
        for x in 0..side {
            let tile = (x as i32, y as i32);
            if seen[at(tile)] && grid.occupied(tile) {
                exposed[(y as usize) * (side as usize) + x as usize] = true;
                tiles.push((x, y));
            }
        }
    }

    ExposureMap {
        side,
        exposed,
        tiles,
    }
}

/// Which tiles the outside can see into.
///
/// **The input the play phase wants** for radiation: a Bim standing in one of
/// these is in the open, whatever the deck under it looks like. Nothing
/// consumes it that way yet; the design phase draws it and warns about it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ExposureMap {
    side: u32,
    exposed: Vec<bool>,
    tiles: Vec<(u32, u32)>,
}

impl ExposureMap {
    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    /// Every exposed tile, in row order.
    pub fn tiles(&self) -> &[(u32, u32)] {
        &self.tiles
    }

    pub fn side(&self) -> u32 {
        self.side
    }

    pub fn contains(&self, (x, y): (i32, i32)) -> bool {
        if x < 0 || y < 0 || x as u32 >= self.side || y as u32 >= self.side {
            return false;
        }
        self.exposed[(y as usize) * (self.side as usize) + x as usize]
    }
}

/// The exposure map as an issue: the tiles, and everything with a tile or a
/// use spot in one of them.
///
/// Use spots are in it because a part can be perfectly well shielded and
/// still be worked from a tile that is not — a worktop against a breached
/// wall is a Bim standing in the open for twenty minutes a meal.
fn radiation(design: &ShipDesign, grid: &Grid, issues: &mut Vec<Issue>) {
    let map = exposure(design);
    if map.is_empty() {
        return;
    }
    let mut parts: Vec<u32> = parts_on(design, grid, map.tiles());
    for part in &design.parts {
        if parts.contains(&part.id) {
            continue;
        }
        if part.use_spots().iter().any(|&spot| map.contains(spot)) {
            parts.push(part.id);
        }
    }
    parts.sort_unstable();
    issues.push(Issue {
        severity: Severity::Warning,
        code: IssueCode::RadiationExposure.code(),
        parts,
        tiles: map.tiles().to_vec(),
    });
}

/// Whether the design has any `Error` in it. What the Accept toggle is
/// disabled on.
pub fn has_errors(issues: &[Issue]) -> bool {
    issues.iter().any(|i| i.severity == Severity::Error)
}

/// One ship, not two.
///
/// Asked of the **structure** layer and of nothing else. The frame is what
/// the ship is; everything else stands on it, directly or through the deck,
/// so a frame in one piece is a ship in one piece and there is no second
/// question to ask. Before there was a structure layer this walked every
/// occupied tile, which meant a wall touching nothing but another wall
/// counted as holding the ship together.
fn connectivity(design: &ShipDesign, grid: &Grid, issues: &mut Vec<Issue>) {
    let side = grid.side();
    let mut seen = vec![false; (side as usize) * (side as usize)];
    let mut components: Vec<Vec<(u32, u32)>> = Vec::new();

    for y in 0..side {
        for x in 0..side {
            let i = (y as usize) * (side as usize) + x as usize;
            if seen[i] || !grid.has_structure((x as i32, y as i32)) {
                continue;
            }
            components.push(flood(grid, &mut seen, (x, y), |g, t| g.has_structure(t)));
        }
    }

    if components.len() < 2 {
        return;
    }

    // Point at everything but the biggest piece: the largest is what the
    // player thinks of as "the ship", and the strays are what has to move.
    let mut biggest = 0;
    for (i, c) in components.iter().enumerate() {
        if c.len() > components[biggest].len() {
            biggest = i;
        }
    }
    let mut tiles = Vec::new();
    for (i, c) in components.into_iter().enumerate() {
        if i != biggest {
            tiles.extend(c);
        }
    }
    let parts = parts_on(design, grid, &tiles);
    issues.push(Issue::error(IssueCode::Disconnected, parts, tiles));
}

/// A 4-connected flood fill from `start` over whatever `passable` admits.
/// Written as a stack rather than recursion: a 60 x 60 build area is 3600
/// tiles deep in the worst case and that is a real stack overflow in wasm.
fn flood(
    grid: &Grid,
    seen: &mut [bool],
    start: (u32, u32),
    passable: impl Fn(&Grid, (i32, i32)) -> bool,
) -> Vec<(u32, u32)> {
    let side = grid.side() as usize;
    let index = |(x, y): (u32, u32)| (y as usize) * side + x as usize;

    let mut out = Vec::new();
    let mut stack = vec![start];
    seen[index(start)] = true;
    while let Some((x, y)) = stack.pop() {
        out.push((x, y));
        for (dx, dy) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
            let next = (x as i32 + dx, y as i32 + dy);
            if !grid.inside(next) || !passable(grid, next) {
                continue;
            }
            let at = (next.0 as u32, next.1 as u32);
            if seen[index(at)] {
                continue;
            }
            seen[index(at)] = true;
            stack.push(at);
        }
    }
    out
}

/// Every part id standing in any of `tiles`, on any layer, without repeats.
fn parts_on(design: &ShipDesign, grid: &Grid, tiles: &[(u32, u32)]) -> Vec<u32> {
    let mut ids: Vec<u32> = Vec::new();
    for &(x, y) in tiles {
        for layer in Layer::ALL {
            let id = grid.get(layer, (x as i32, y as i32));
            if id != 0 && design.part(id).is_some() && !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    ids.sort_unstable();
    ids
}

/// A bed and a seat each, for everybody who is coming.
fn crew(design: &ShipDesign, crew_count: u32, issues: &mut Vec<Issue>) {
    for (kind, code) in [
        (PartKind::Bunk, IssueCode::TooFewBunks),
        (PartKind::Chair, IssueCode::TooFewChairs),
    ] {
        if design.count(kind) < crew_count {
            issues.push(Issue::error(code, Vec::new(), Vec::new()));
        }
    }
}

fn required(design: &ShipDesign, issues: &mut Vec<Issue>) {
    for &(kind, code) in REQUIRED.iter() {
        if design.count(kind) == 0 {
            issues.push(Issue::error(code, Vec::new(), Vec::new()));
        }
    }
}

/// Every use spot has to be deck a body can stand on. Returns whether any
/// were not, which is what holds the reachability check back.
fn use_spots(design: &ShipDesign, grid: &Grid, issues: &mut Vec<Issue>) -> bool {
    let mut parts = Vec::new();
    let mut tiles = Vec::new();
    for part in &design.parts {
        for spot in part.use_spots() {
            if grid.inside(spot) && walkable(design, grid, spot) {
                continue;
            }
            if !parts.contains(&part.id) {
                parts.push(part.id);
            }
            // A spot off the edge of the build area has no tile to
            // highlight, so the part's own tiles are what gets rung instead.
            if grid.inside(spot) {
                tiles.push((spot.0 as u32, spot.1 as u32));
            } else {
                tiles.extend(part.tiles());
            }
        }
    }
    if parts.is_empty() {
        return false;
    }
    issues.push(Issue::error(IssueCode::UseSpotBlocked, parts, tiles));
    true
}

/// Everywhere a Bim has to stand has to be walkable to from everywhere else
/// it has to stand. Doors are walkable, so a route through one counts.
fn reachability(design: &ShipDesign, grid: &Grid, issues: &mut Vec<Issue>) {
    let mut spots: Vec<((u32, u32), u32)> = Vec::new();
    for part in &design.parts {
        for spot in part.use_spots() {
            // `use_spots` above has already established these are in bounds
            // and walkable; this only runs when it found nothing wrong.
            spots.push(((spot.0 as u32, spot.1 as u32), part.id));
        }
    }
    if spots.len() < 2 {
        return;
    }

    let side = grid.side();
    let mut seen = vec![false; (side as usize) * (side as usize)];
    let reached = flood(grid, &mut seen, spots[0].0, |g, t| walkable(design, g, t));

    let mut parts = Vec::new();
    let mut tiles = Vec::new();
    for (spot, id) in spots.into_iter().skip(1) {
        if reached.contains(&spot) {
            continue;
        }
        if !parts.contains(&id) {
            parts.push(id);
        }
        tiles.push(spot);
    }
    if parts.is_empty() {
        return;
    }
    issues.push(Issue::error(IssueCode::UseSpotsCutOff, parts, tiles));
}

/// Engines are a warning, never an error: a ship that cannot fly is still a
/// ship you can live on, and telling a player they may not accept one would
/// be the design phase having an opinion about how to play.
fn engines(design: &ShipDesign, issues: &mut Vec<Issue>) {
    let engines: Vec<&crate::design::PlacedPart> = design
        .parts
        .iter()
        .filter(|p| p.kind == PartKind::Engine)
        .collect();
    if engines.is_empty() {
        issues.push(Issue::warning(IssueCode::NoEngine));
        return;
    }
    let missing = Facing::ALL
        .iter()
        .any(|&axis| !engines.iter().any(|p| p.rotation.facing() == axis));
    if missing {
        issues.push(Issue::warning(IssueCode::NoEngineOnAxis));
    }
}

fn comforts(design: &ShipDesign, issues: &mut Vec<Issue>) {
    if design.count(PartKind::HydroBay) == 0 {
        issues.push(Issue::warning(IssueCode::NoHydroBay));
    }
    if design.count(PartKind::BroomLocker) == 0 {
        issues.push(Issue::warning(IssueCode::NoBroomLocker));
    }
    if design.count(PartKind::Helm) == 0 {
        issues.push(Issue::warning(IssueCode::NoHelm));
    }
    // Food is what is *aboard*, not what the ship could hold: a cold store
    // with nothing in it feeds nobody. The bay is a separate warning and a
    // separate problem — it makes more, slowly.
    let food = design.carrying(ResourceId::Vegetable) + design.carrying(ResourceId::Tofu);
    if food == 0 {
        issues.push(Issue::warning(IssueCode::NoFoodAboard));
    }
}
