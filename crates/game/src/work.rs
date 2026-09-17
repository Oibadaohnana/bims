//! What the crew get on with, and the order the player wants it done in.
//!
//! Every errand a Bim takes on *of its own accord* is one of these jobs. The
//! player gives each a number from [`HIGHEST`] to [`LOWEST`], and a Bim with
//! nothing pressing works through whatever is going in that order. They all
//! start equal, so out of the box this changes nothing and the ship behaves
//! exactly as it did before anybody touched the panel.
//!
//! What it does **not** touch is the body. Sleep and the heads are not work —
//! there is no row for them and no number to set — and a Bim past its hunger
//! is fed whatever the cook row says, because a priority list is a statement
//! about what to do next and not a licence to starve. See
//! `Game::consider_errand`, which is the only place any of this is read.
//!
//! No strings cross the wasm boundary, so the ship knows [`Job::Clean`] and
//! the host knows "Cleaning". **Adding a job is three edits**: a variant here,
//! a name in `JOB_NAMES` in `web/bims.js`, and the range in
//! `scratchpad/work.rs` that checks every job is nameable. Miss the second and
//! the row renders blank; miss the third and the probe passes on a job nobody
//! can read.

/// The jobs, in the order they are listed and in the order the codes run.
///
/// The code is the whole identity across the boundary — the host indexes its
/// name table with it — so variants are appended rather than inserted.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Job {
    /// Sweeping the deck.
    Clean,
    /// Sowing an empty tray in the bay.
    Plant,
    /// Lifting a ripe one out of it.
    Cut,
    /// Carrying what was lifted to the cold store. It has no errand of its
    /// own yet — nothing aboard is fetched or moved except a harvest — so
    /// this is the back half of a [`Job::Cut`], and a cutting waits on
    /// whichever of the two is set later. When something else worth hauling
    /// arrives, this is the row it goes under.
    Haul,
    /// Cooking: a meal for a hungry Bim, and stew for the cold store while
    /// the shelf holds fewer than the manager asked for — one vegetable and
    /// one block of tofu, chopped, cooked and put away in a tub. One row,
    /// because both are the galley, and a Bim that is hungry eats before it
    /// cooks for the shelf whatever the number says.
    Cook,
    /// Standing at the helm to control the ship. On offer while the ship is
    /// away from a berth and nobody is posted at the helm; whoever takes it
    /// is posted there — a standing order, like the player's own "take the
    /// helm" — and let go when the ship is tied up again. The room only
    /// knows the helm through `Game::set_helm`, which the world calls: the
    /// classic room has no helm and never offers this.
    Helm,
    /// Making something at a bench — the smelter, the workbench — while the
    /// world has an order for it: the hold short of a product the player
    /// asked to keep, the inputs aboard, and the station powered. One row
    /// for every bench, because what a Bim does at any of them is stand
    /// there; which recipe is the order's. See `game::Order`.
    Craft,
    /// A walk outside in a suit to gather ore, while the ship is holding
    /// at a belt with a suit aboard and room for what comes back. The world
    /// says when — `game::Eva` — and what a walk yields is the belt's.
    Mine,
}

impl Job {
    pub const ALL: [Job; 8] = [
        Job::Clean,
        Job::Plant,
        Job::Cut,
        Job::Haul,
        Job::Cook,
        Job::Helm,
        Job::Craft,
        Job::Mine,
    ];

    /// 0, then one per job. The host names them.
    pub fn code(self) -> u32 {
        self as u32
    }

    pub fn from_code(code: u32) -> Option<Job> {
        Job::ALL.get(code as usize).copied()
    }
}

/// The most important a job can be set to, and the least. Small is urgent:
/// a 1 is done before a 2, which is how every list of this shape reads.
pub const HIGHEST: u32 = 1;
pub const LOWEST: u32 = 5;
/// What everything starts at — the middle, so the first click in either
/// direction says something.
pub const DEFAULT: u32 = 3;

/// One number per job. The player's standing instruction to the ship rather
/// than to a Bim: there is one list and both crew work to it, the same as the
/// timetable and the action thresholds.
pub struct Priorities {
    level: [u32; Job::ALL.len()],
}

impl Priorities {
    pub fn new() -> Priorities {
        Priorities {
            level: [DEFAULT; Job::ALL.len()],
        }
    }

    pub fn of(&self, job: Job) -> u32 {
        self.level[job as usize]
    }

    /// Set one, clamped to the range. Out-of-range is clamped rather than
    /// refused: the number crosses the boundary as a bare `u32` and a silent
    /// no-op would leave the panel showing something the ship is not doing.
    pub fn set(&mut self, job: Job, level: u32) {
        self.level[job as usize] = level.clamp(HIGHEST, LOWEST);
    }

    /// What a click on the box does: one step less important, and round to
    /// the top again from the bottom. The cycling lives here rather than in
    /// the host so that the range has exactly one definition.
    pub fn cycle(&mut self, job: Job) -> u32 {
        let next = if self.of(job) >= LOWEST {
            HIGHEST
        } else {
            self.of(job) + 1
        };
        self.set(job, next);
        next
    }

    /// Which of two jobs is done first. Ties keep the order they were offered
    /// in, which is what makes an untouched list behave exactly as the fixed
    /// order did before there was a list at all.
    pub fn before(&self, a: Job, b: Job) -> bool {
        self.of(a) < self.of(b)
    }
}
