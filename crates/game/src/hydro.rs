//! The hydroponic bay: six trays, and the standing order that fills them.
//!
//! The bay grows what the cold store is short of. The manager sets three
//! targets (`manager.rs`) — vegetables, blocks of tofu and fibre, the crop
//! a bandage is made of — and the bay plants whichever of the three it is
//! furthest behind on. When all are met it **hibernates**: nothing grows,
//! nothing is planted, and whatever is in the trays is kept exactly as it is
//! until the store falls back under the mark.
//!
//! Nothing here happens by itself. The bay says what wants doing and the Bim
//! walks over and does it, one tray at a time, the same as every other switch
//! and fixture aboard.

use crate::clock::DAY;
use crate::draw::{Color, DrawList};
use crate::math::{Rect, Vec2, vec2};
use crate::room::{GLOW, GLOW_DIM, PANEL, PANEL_EDGE, PANEL_LIT, STEEL};

/// Trays in the bay. One plant apiece.
pub const SPOTS: usize = 6;

/// What a tray can be growing.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Crop {
    /// Greens: the two that go into a stew, and the salad beside a bowl.
    Veg,
    /// Soy, which is pressed into the tofu a bowl is built on.
    Soy,
    /// Fibre: nothing anybody eats. What the drug lab makes a bandage out
    /// of, two stalks to a roll — see `shipdesign::recipes`. It goes into
    /// the cold store like the other two, counted apart, and the world
    /// carries the harvest into the hold.
    Fibre,
}

impl Crop {
    /// Game minutes from planting to ripe. Soy takes half again as long as
    /// greens do, which is the whole reason the bay has to choose; fibre
    /// comes up in a day like greens.
    pub fn ripens_in(self) -> f32 {
        match self {
            Crop::Veg => DAY,
            Crop::Soy => 1.5 * DAY,
            Crop::Fibre => DAY,
        }
    }

    /// 1, 2 and 3; 0 means an empty tray. The host names them.
    pub fn code(self) -> u32 {
        match self {
            Crop::Veg => 1,
            Crop::Soy => 2,
            Crop::Fibre => 3,
        }
    }

    pub fn from_code(code: u32) -> Option<Crop> {
        match code {
            1 => Some(Crop::Veg),
            2 => Some(Crop::Soy),
            3 => Some(Crop::Fibre),
            _ => None,
        }
    }
}

/// What the bay would like doing next, at a tray.
#[derive(Clone, Copy, PartialEq)]
pub enum Job {
    /// Lift a ripe plant and put it in the store.
    Harvest(usize),
    /// Put this in an empty tray.
    Plant(usize, Crop),
}

impl Job {
    pub fn spot(self) -> usize {
        match self {
            Job::Harvest(i) | Job::Plant(i, _) => i,
        }
    }
}

#[derive(Clone, Copy)]
struct Plant {
    crop: Crop,
    /// Game minutes in the tray so far.
    grown: f32,
}

impl Plant {
    fn ripe(&self) -> bool {
        self.grown >= self.crop.ripens_in()
    }

    /// How far along, 0 to 1.
    fn share(&self) -> f32 {
        (self.grown / self.crop.ripens_in()).clamp(0.0, 1.0)
    }
}

pub struct Bay {
    /// The frame itself, which is furniture and gets walked round.
    pub frame: Rect,
    /// Which side the Bim works it from, as a unit step out of the frame.
    side: Vec2,
    trays: [Rect; SPOTS],
    spots: [Option<Plant>; SPOTS],

    /// Whether the bay is following the manager's target at all. On from the
    /// start: a bay that has to be switched on is a bay that is off when the
    /// player has not noticed it, and the target it works to is met at dawn
    /// anyway, so it sits quietly until the store dips.
    automated: bool,
    /// A standing order from the player: fill every tray with this, target or
    /// no target. It overrides the demand and hibernation both — "plant this,
    /// no matter what".
    forced: Option<Crop>,
    /// Set while the store is at or over target and there is nothing to do.
    hibernating: bool,
    /// What the manager asked for, in vegetables, blocks of tofu and fibre.
    /// Pushed in rather than read out, so everything the bay needs to
    /// decide with is in the bay and a tray can be worked without the
    /// manager in reach.
    want: (u32, u32, u32),
    /// Seconds of glow left after the Bim works a tray, so the bay reads as
    /// having been touched.
    stir: f32,
}

impl Bay {
    /// Along the bottom wall on the left: the one stretch of deck with nothing
    /// else on it. Clear of the table, so the Bim standing at a tray is not
    /// also standing against the table edge, and clear of the run between the
    /// table and the heads.
    pub fn new(interior: Rect) -> Bay {
        Bay::at(
            Rect::from_min_size(
                vec2(interior.min.x + 16.0, interior.max.y - 56.0),
                vec2(282.0, 56.0),
            ),
            vec2(0.0, -1.0),
        )
    }

    /// A bay wherever a layout puts it, worked from the side `side` steps
    /// out to. The trays run along the frame the long way: a bay lying
    /// east–west is six trays across, one standing north–south six trays
    /// down, and the Bim stands beside whichever tray it is working.
    pub fn at(frame: Rect, side: Vec2) -> Bay {
        let inner = frame.expand(-7.0);
        let across = side.x.abs() > side.y.abs();
        let trays = core::array::from_fn(|i| {
            if across {
                let height = inner.height() / SPOTS as f32;
                Rect::from_min_size(
                    vec2(inner.min.x, inner.min.y + i as f32 * height + 2.0),
                    vec2(inner.width(), height - 4.0),
                )
            } else {
                let width = inner.width() / SPOTS as f32;
                Rect::from_min_size(
                    vec2(inner.min.x + i as f32 * width + 2.0, inner.min.y),
                    vec2(width - 4.0, inner.height()),
                )
            }
        });
        Bay {
            frame,
            side,
            trays,
            spots: [None; SPOTS],
            automated: true,
            forced: None,
            hibernating: false,
            want: (0, 0, 0),
            stir: 0.0,
        }
    }

    /// Where the Bim stands to reach the trays: on the deck side, in front of
    /// whichever tray it is working.
    pub fn station(&self, spot: usize) -> Vec2 {
        let tray = self.trays[spot.min(SPOTS - 1)];
        if self.side.x.abs() > self.side.y.abs() {
            let x = if self.side.x < 0.0 {
                self.frame.min.x - 30.0
            } else {
                self.frame.max.x + 30.0
            };
            vec2(x, tray.center().y)
        } else if self.side.y > 0.0 {
            vec2(tray.center().x, self.frame.max.y + 30.0)
        } else {
            vec2(tray.center().x, self.frame.min.y - 30.0)
        }
    }

    // --- what the player asks of it ---------------------------------------

    pub fn automated(&self) -> bool {
        self.automated
    }

    pub fn set_automated(&mut self, on: bool) {
        self.automated = on;
    }

    pub fn forced(&self) -> Option<Crop> {
        self.forced
    }

    /// The standing order. `None` puts the bay back on the manager's demand.
    pub fn force(&mut self, crop: Option<Crop>) {
        self.forced = crop;
    }

    pub fn hibernating(&self) -> bool {
        self.hibernating
    }

    // --- what is in it ----------------------------------------------------

    /// What is growing in a tray: 0 empty, else the crop's code.
    pub fn crop_at(&self, spot: usize) -> u32 {
        self.spots
            .get(spot)
            .and_then(|s| s.as_ref())
            .map_or(0, |p| p.crop.code())
    }

    /// How far along a tray is, 0 to 1. An empty tray is 0.
    pub fn growth_at(&self, spot: usize) -> f32 {
        self.spots
            .get(spot)
            .and_then(|s| s.as_ref())
            .map_or(0.0, |p| p.share())
    }

    pub fn ripe_count(&self) -> u32 {
        self.spots
            .iter()
            .filter(|s| s.is_some_and(|p| p.ripe()))
            .count() as u32
    }

    // --- the clock --------------------------------------------------------

    /// Grow what is planted, and work out whether the bay has anything left to
    /// do. Hibernation is exactly "automated, nothing forced, and the store is
    /// at or over all three marks": the trays hold what they hold and the
    /// clock stops for them.
    pub fn update(
        &mut self,
        dt: f32,
        minutes: f32,
        veg: u32,
        tofu: u32,
        fibre: u32,
        want: (u32, u32, u32),
    ) {
        self.want = want;
        self.stir = (self.stir - dt).max(0.0);
        self.hibernating = self.automated
            && self.forced.is_none()
            && veg >= want.0
            && tofu >= want.1
            && fibre >= want.2;
        if self.hibernating {
            return;
        }
        for spot in self.spots.iter_mut().flatten() {
            if !spot.ripe() {
                spot.grown += minutes;
            }
        }
    }

    /// Whether the bay is asking for anything at all.
    fn running(&self, veg: u32, tofu: u32, fibre: u32) -> bool {
        if self.forced.is_some() {
            return true;
        }
        self.automated && (veg < self.want.0 || tofu < self.want.1 || fibre < self.want.2)
    }

    /// The next tray wanting a hand, if any: anything ripe first — it is grown
    /// already and leaving it there serves nobody — then the empty trays.
    pub fn wants_work(&self, veg: u32, tofu: u32, fibre: u32) -> Option<Job> {
        if !self.running(veg, tofu, fibre) {
            return None;
        }
        if let Some(i) = self.spots.iter().position(|s| s.is_some_and(|p| p.ripe())) {
            return Some(Job::Harvest(i));
        }
        let empty = self.spots.iter().position(|s| s.is_none())?;
        Some(Job::Plant(empty, self.wanted(veg, tofu, fibre)))
    }

    /// Which crop the bay is furthest behind on, as a *share* of what was
    /// asked for, counting what is already in the trays.
    ///
    /// The share is the part that matters. Comparing plain shortfalls would
    /// have the bay plant the bigger target over and over — a target of forty
    /// greens and twenty soy is short of greens by more at every step of the
    /// way — and five trays of greens would go in while the soy it is just as
    /// far behind on never does. Measured proportionally the trays come out at
    /// roughly the ratio that was asked for, which is the whole point of a
    /// food unit having two halves.
    fn wanted(&self, veg: u32, tofu: u32, fibre: u32) -> Crop {
        if let Some(crop) = self.forced {
            return crop;
        }
        let coming = |crop: Crop| {
            self.spots
                .iter()
                .flatten()
                .filter(|p| p.crop == crop)
                .count() as f32
        };
        let short = |have: u32, target: u32, crop: Crop| {
            if target == 0 {
                return 0.0;
            }
            (target as f32 - have as f32 - coming(crop)) / target as f32
        };
        // Greens on a tie: they are the half of a food unit there is twice as
        // much of, and they come up in a day rather than a day and a half.
        let food = if short(tofu, self.want.1, Crop::Soy) > short(veg, self.want.0, Crop::Veg) {
            Crop::Soy
        } else {
            Crop::Veg
        };
        // Fibre only when it was asked for. A target of nought reads as a
        // shortfall of nought above, and a store over-committed on food —
        // greens short by one with two coming up — reads as *less* than
        // nought, so without this a bay nobody asked for fibre would plant
        // it the moment the food was in hand. That would re-roll every
        // probe seed pinned on the bay, for a crop nobody wanted.
        let (food_have, food_want) = match food {
            Crop::Veg => (veg, self.want.0),
            _ => (tofu, self.want.1),
        };
        if self.want.2 > 0
            && short(fibre, self.want.2, Crop::Fibre) > short(food_have, food_want, food)
        {
            Crop::Fibre
        } else {
            food
        }
    }

    /// Do one tray's work, now that the Bim's hands are on it. Returns the
    /// crop lifted, for the store to take in.
    ///
    /// The job is worked out again at this moment rather than being carried
    /// from when the errand started: by the time the Bim gets here the store
    /// may have moved, and what is true when the hand arrives is what counts.
    pub fn work(&mut self, job: Job) -> Option<Crop> {
        self.stir = STIR_TIME;
        match job {
            Job::Harvest(i) => {
                let plant = self.spots.get_mut(i)?.take()?;
                Some(plant.crop)
            }
            Job::Plant(i, crop) => {
                let spot = self.spots.get_mut(i)?;
                if spot.is_none() {
                    *spot = Some(Plant { crop, grown: 0.0 });
                }
                None
            }
        }
    }

    // --- drawing ----------------------------------------------------------

    pub fn draw(&self, list: &mut DrawList) {
        // The frame, and the water channel down the middle of it.
        list.rect(self.frame.center(), self.frame.size(), 0.0, 0.0, PANEL);
        list.stroke_rect(
            self.frame.center(),
            self.frame.size(),
            0.0,
            0.0,
            1.5,
            PANEL_EDGE,
        );

        let lit = if self.hibernating { 0.12 } else { 1.0 };
        for (i, tray) in self.trays.iter().enumerate() {
            list.rect(tray.center(), tray.size(), 0.0, 0.0, PANEL_LIT);
            // The grow light over each tray, out in hibernation.
            list.rect(
                vec2(tray.center().x, tray.min.y + 3.0),
                vec2(tray.width() - 6.0, 3.0),
                0.0,
                0.0,
                if self.hibernating { PANEL_EDGE } else { GLOW },
            );
            list.rect(
                vec2(tray.center().x, tray.center().y),
                vec2(tray.width() - 8.0, tray.height() - 12.0),
                0.0,
                0.0,
                GLOW_DIM.alpha(0.10 * lit),
            );

            let Some(plant) = self.spots[i] else { continue };
            draw_plant(list, *tray, plant.crop, plant.share(), lit);
        }

        // A hand on the bay leaves it stirring for a moment, so working a tray
        // reads as having done something even when the tray looks the same.
        if self.stir > 0.0 {
            let fade = (self.stir / STIR_TIME).clamp(0.0, 1.0);
            list.stroke_rect(
                self.frame.center(),
                self.frame.size() + vec2(6.0, 6.0),
                0.0,
                0.0,
                2.0,
                GLOW.alpha(0.5 * fade),
            );
        }
    }
}

/// How long the bay glows after a tray is worked.
const STIR_TIME: f32 = 1.2;

const SOIL: Color = Color::rgb(0.16, 0.14, 0.11);
const LEAF: Color = Color::rgb(0.44, 0.68, 0.24);
const LEAF_DARK: Color = Color::rgb(0.30, 0.50, 0.16);
const BEAN: Color = Color::rgb(0.86, 0.84, 0.62);
const STEM: Color = Color::rgb(0.36, 0.55, 0.28);
/// Fibre is a paler, taller stalk: straw rather than greens, so a tray of
/// it reads as not-food across the room.
const STRAW: Color = Color::rgb(0.80, 0.86, 0.60);
/// A ripe tray is ringed, so "ready" reads across the room.
const RIPE: Color = Color::rgb(0.98, 0.82, 0.35);

fn draw_plant(list: &mut DrawList, tray: Rect, crop: Crop, share: f32, lit: f32) {
    let at = tray.center();
    list.rect(
        vec2(at.x, tray.max.y - 7.0),
        vec2(tray.width() - 10.0, 8.0),
        0.0,
        0.0,
        SOIL,
    );

    // Everything grows out of the soil line: a seedling is a stem and nothing
    // else, and the leaves come in as it fills out.
    let base = vec2(at.x, tray.max.y - 10.0);
    // Fibre grows a third taller than the food, and in straw.
    let tall = if crop == Crop::Fibre { 1.35 } else { 1.0 };
    let height = (5.0 + 20.0 * share) * tall * lit.max(0.55);
    list.rect(
        base - vec2(0.0, height * 0.5),
        vec2(2.0, height),
        0.0,
        0.0,
        if crop == Crop::Fibre { STRAW } else { STEM },
    );

    match crop {
        Crop::Veg => {
            let spread = 3.0 + 9.0 * share;
            for side in [-1.0f32, 1.0] {
                list.ellipse(
                    base - vec2(-side * spread * 0.6, height * 0.62),
                    vec2(spread, spread * 0.68),
                    side * 0.5,
                    if share > 0.66 { LEAF } else { LEAF_DARK },
                );
            }
        }
        Crop::Soy => {
            // Pods rather than leaves, and only once it is well on.
            let spread = 2.5 + 6.0 * share;
            for side in [-1.0f32, 1.0] {
                list.ellipse(
                    base - vec2(-side * spread * 0.5, height * 0.5),
                    vec2(spread * 0.7, spread * 1.2),
                    side * 0.35,
                    LEAF_DARK,
                );
            }
            if share > 0.6 {
                for side in [-1.0f32, 1.0] {
                    list.ellipse(
                        base - vec2(-side * 4.0, height * 0.85),
                        vec2(4.0, 6.0),
                        side * 0.3,
                        BEAN,
                    );
                }
            }
        }
        Crop::Fibre => {
            // A sheaf: two more stalks leaning out from the first, and no
            // leaves to speak of — it is the stalk that is the crop.
            let lean = 0.12 + 0.18 * share;
            for side in [-1.0f32, 1.0] {
                list.rect(
                    base - vec2(-side * height * 0.18, height * 0.45),
                    vec2(1.6, height * 0.9),
                    side * lean,
                    0.0,
                    STRAW,
                );
            }
            // The seed head, once it is well on.
            if share > 0.5 {
                list.ellipse(
                    base - vec2(0.0, height * 0.98),
                    vec2(3.0, 5.0),
                    0.0,
                    BEAN,
                );
            }
        }
    }

    if share >= 1.0 {
        list.stroke_rect(tray.center(), tray.size(), 0.0, 0.0, 1.5, RIPE.alpha(0.9));
    }
    let _ = STEEL;
}
