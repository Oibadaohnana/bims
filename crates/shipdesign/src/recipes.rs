//! What the workstations make, and out of what.
//!
//! One table, [`RECIPES`], and one shape of chain for all of it: a Bim
//! stands at the recipe's [`station`](Recipe::station) for its
//! [`minutes`](Recipe::minutes), and the inputs come out of the hold and the
//! output goes in. The chain is the room's (`bims::task::Kind::Craft`) and
//! the hold is the world's, so **this crate does neither** — it says what a
//! recipe is, and `crates/world` moves the cargo when the room says a Bim
//! has finished one. A native server will move the same cargo off the same
//! table.
//!
//! # Mass
//!
//! Crafting **conserves mass**: an output weighs exactly what went into it,
//! and `every_recipe_holds_together` in the tests holds the resource table
//! to that. The smelter is the one exception, written down as
//! [`Recipe::vents`]: two ore at ten is twenty, one metal is eight, and the
//! twelve is slag and is vented. A recipe may lose mass only there, and may
//! gain it nowhere. That is the two entries making things adds to the
//! list in [`crate::materials`] of what changes a ship's mass.
//!
//! # A recipe is data, a job is a station
//!
//! There is no chain per product. A new thing to make is one row here, and
//! the row says which station; the work list has one job per station
//! (`bims::work::Job::Craft` is all of them today). A target per output —
//! "keep twenty components" — is the world's, set by the player, and it is
//! what turns a row into an errand: a recipe is on offer while the hold has
//! fewer of its output than the target, the inputs for one, room for the
//! output, a bench of its station aboard, and that station powered.

use physics::ResourceId;

use crate::parts::PartKind;

/// One thing the crew can make.
#[derive(Clone, Copy, Debug)]
pub struct Recipe {
    /// Which part a Bim stands at to make it. Has to draw power — a
    /// workstation that ran in a brownout would be one the reactor was
    /// not needed for.
    pub station: PartKind,
    /// Units of each resource taken out of the hold. Never empty, and no
    /// resource twice.
    pub inputs: &'static [(ResourceId, u32)],
    /// What goes in: one resource, so many units.
    pub output: (ResourceId, u32),
    /// How long a Bim stands at the bench, in game minutes.
    pub minutes: u32,
    /// Whether the recipe may **lose** mass — slag off the smelter. Never
    /// gain it. See the module note.
    pub vents: bool,
}

/// The table. Indexed by position, and the index is what crosses the wasm
/// boundary — `ship_recipe_*` — so a recipe is appended, never inserted.
pub static RECIPES: [Recipe; 6] = [
    Recipe {
        station: PartKind::Smelter,
        inputs: &[(ResourceId::Ore, 2)],
        output: (ResourceId::Metal, 1),
        minutes: 30,
        vents: true,
    },
    Recipe {
        station: PartKind::Workbench,
        inputs: &[(ResourceId::Metal, 1)],
        output: (ResourceId::Components, 4),
        minutes: 20,
        vents: false,
    },
    Recipe {
        station: PartKind::Workbench,
        inputs: &[
            (ResourceId::Metal, 1),
            (ResourceId::Components, 2),
            (ResourceId::Galvum, 1),
        ],
        output: (ResourceId::Emitter, 1),
        minutes: 60,
        vents: false,
    },
    // The armoury: what a Bim carries into a fight, wears into one, and is
    // patched up with after.
    Recipe {
        station: PartKind::Armoury,
        inputs: &[(ResourceId::Components, 2), (ResourceId::Emitter, 1)],
        output: (ResourceId::Handgun, 1),
        minutes: 45,
        vents: false,
    },
    Recipe {
        station: PartKind::Armoury,
        inputs: &[(ResourceId::Metal, 4), (ResourceId::Components, 2)],
        output: (ResourceId::Vest, 1),
        minutes: 40,
        vents: false,
    },
    Recipe {
        station: PartKind::Armoury,
        inputs: &[(ResourceId::Vegetable, 2), (ResourceId::Components, 1)],
        output: (ResourceId::Medkit, 1),
        minutes: 15,
        vents: false,
    },
];

impl Recipe {
    /// What the inputs weigh, all together.
    pub fn input_mass(&self) -> f64 {
        self.inputs
            .iter()
            .map(|&(id, units)| units as f64 * id.mass_per_unit())
            .sum()
    }

    /// What the output weighs.
    pub fn output_mass(&self) -> f64 {
        self.output.1 as f64 * self.output.0.mass_per_unit()
    }
}

/// Whether the table holds together: every recipe on a station that draws,
/// with something in, one thing out that is not also in, no input twice,
/// every count above nought, a length, and the mass rule — equal unless it
/// vents, and never more out than in.
pub fn recipes_are_sound() -> bool {
    RECIPES.iter().all(|r| {
        let station = r.station.def().draws();
        let inputs = !r.inputs.is_empty()
            && r.inputs.iter().enumerate().all(|(i, &(id, units))| {
                units > 0 && id != r.output.0 && !r.inputs[..i].iter().any(|&(seen, _)| seen == id)
            });
        let output = r.output.1 > 0;
        let (in_mass, out_mass) = (r.input_mass(), r.output_mass());
        let mass = if r.vents {
            out_mass <= in_mass
        } else {
            (out_mass - in_mass).abs() < 1e-9
        };
        station && inputs && output && r.minutes > 0 && mass
    })
}

/// Every recipe made at `station`, by index into [`RECIPES`].
pub fn at(station: PartKind) -> impl Iterator<Item = (usize, &'static Recipe)> {
    RECIPES
        .iter()
        .enumerate()
        .filter(move |(_, r)| r.station == station)
}
