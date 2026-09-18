//! Every word on the screen.
//!
//! The simulation knows crew member 0, part kind 7 and event code 14, and
//! nothing else about them: no strings come out of the rules crates, and
//! that is deliberate — a native server will one day run the same crates
//! and it has no words to say. These tables are where the words are, indexed
//! by the codes each crate writes out and never renumbers. Adding a part, an
//! event or a job is a variant there and a name here.

use physics::ResourceId;
use shipdesign::parts::PartKind;
use world::{Refusal, WorldEvent};

/// Who is aboard, by lobby slot. The room calls crew 0 James.
pub const CREW_NAMES: [&str; 4] = ["James", "Kate", "Priya", "Tomas"];

pub fn crew_name(who: u32) -> String {
    CREW_NAMES
        .get(who as usize)
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("Crew {}", who + 1))
}

/// What the people living on a station are called. The world knows a
/// resident as a station and a seat and nothing else, so the names are
/// dealt out here, by station and seat, off one list: enough that the two
/// on one station never share a name, and the same pair every time the ship
/// comes back.
pub const RESIDENT_NAMES: [&str; 16] = [
    "Ada", "Tomas", "Priya", "Yusuf", "Mei", "Olu", "Sanne", "Ravi", "Ines", "Kofi", "Hana",
    "Bram", "Leila", "Jonas", "Nour", "Emil",
];

pub fn resident_name(station: u32, who: u32) -> String {
    RESIDENT_NAMES[((station * 2 + who) as usize) % RESIDENT_NAMES.len()].to_string()
}

/// What each part is called. Indexed by the `PartKind` discriminant in
/// `crates/shipdesign/src/parts.rs`.
pub const PART_NAMES: [&str; 36] = [
    "Deck plating",
    "Wall",
    "Door",
    "Engine",
    "Bunk",
    "Cold store",
    "Worktop",
    "Hob",
    "Dishwasher",
    "Table",
    "Chair",
    "Toilet",
    "Basin",
    "Hydroponic bay",
    "Broom locker",
    "Structure",
    "Outside wall",
    "Helm",
    "Reactor",
    "Power conduit",
    "Battery",
    "Fuel tank",
    "Life support",
    "Airlock",
    "Sensor array",
    "Shelf",
    "Shower",
    "Thruster",
    "Heavy engine",
    "Diagonal wall",
    "Diagonal outside wall",
    "Smelter",
    "Workbench",
    "Suit locker",
    "Armoury",
    "Drug lab",
];

pub fn part_name(kind: PartKind) -> &'static str {
    PART_NAMES
        .get(kind as usize)
        .copied()
        .unwrap_or("Something")
}

/// The palette, grouped the way a ship is thought about rather than the way
/// the enum is numbered. The rows are built off `PartKind::ALL` rather than
/// off this list, so a kind added to the enum and forgotten here still gets
/// a button — under "Anything else", where it is obvious.
pub const PART_GROUPS: &[(&str, &[u32])] = &[
    // Hull first, in the order a ship is actually built: deck, skin, then the
    // ways through it. The frame is not a tool of its own — see NOT_A_TOOL.
    ("Hull", &[0, 1, 29, 16, 30, 2, 23]),
    ("Systems", &[3, 28, 27, 17, 18, 19, 20, 21, 22, 24]),
    ("Crew", &[4, 9, 10, 26]),
    ("Galley", &[5, 6, 7, 8]),
    ("Heads", &[11, 12]),
    ("Storage", &[25, 33]),
    ("Bay", &[13, 14]),
    ("Workshop", &[31, 32, 34, 35]),
];

/// The Build tab's categories, the way a colonist-game player thinks about
/// them rather than the way a shipwright does: what holds the ship
/// together, what it is lived in with, what makes things, and so on. Every
/// kind the palette offers is in exactly one — `every_buildable_part_is_in_one_build_group`
/// pins it — and the frame is left out for `NOT_A_TOOL`'s reason. Each is
/// a name and a line saying what goes under it.
pub const BUILD_GROUPS: &[(&str, &str, &[u32])] = &[
    (
        "Structure",
        "The frame and the skin: deck to walk on, walls to divide it, the hull that keeps the outside out, and the ways through.",
        &[0, 1, 29, 16, 30, 2, 23],
    ),
    (
        "Furniture",
        "What the crew live with: somewhere to sleep, sit and eat, and somewhere to keep things.",
        &[4, 9, 10, 25, 14],
    ),
    (
        "Production",
        "Where something is made: the workshop benches, the armoury, the drug lab, and the bay that grows the food.",
        &[31, 32, 34, 35, 13],
    ),
    (
        "Galley",
        "Where a meal is cooked and cleared up after.",
        &[5, 6, 7, 8],
    ),
    ("Hygiene", "The heads and the shower.", &[11, 12, 26]),
    (
        "Power",
        "What makes power, what carries it, and what holds it.",
        &[18, 19, 20],
    ),
    (
        "Ship systems",
        "What flies the ship and keeps it alive: the helm, life support, the sensors, the tank, and the suits.",
        &[17, 22, 24, 21, 33],
    ),
    (
        "Propulsion",
        "The engines that push and the thrusters that turn.",
        &[3, 28, 27],
    ),
];

/// Kinds the palette does not offer, though the ship knows them. Structure
/// is the one: deck plating lays its own frame, so to a player the frame and
/// the deck are one thing and a second button for the half underneath would
/// be a trap.
pub const NOT_A_TOOL: &[u32] = &[15];

/// What a station sells, indexed by `physics::ResourceId`.
pub const RESOURCE_NAMES: [&str; 15] = [
    "Ore",
    "Metal",
    "Fuel",
    "Components",
    "Vegetables",
    "Tofu",
    "Galvum",
    "Emitters",
    "Suits",
    "Handguns",
    "Vests",
    "Medkits",
    "Rock",
    "Fibre",
    "Bandages",
];

pub fn resource_name(id: ResourceId) -> &'static str {
    RESOURCE_NAMES
        .get(id as usize)
        .copied()
        .unwrap_or("Something")
}

/// Where goods are stowed, indexed by `economy::Storage`.
pub const STORAGE_NAMES: [&str; 4] = ["Shelves", "Fuel tanks", "Cold stores", "Lockers"];

/// How many units a buy or sell button moves.
pub const TRADE_STEPS: [u32; 3] = [1, 10, 100];

/// Why an edit was refused. Indexed by `EditError`; 0 never appears because
/// 0 is "it took".
pub fn edit_line(code: u32) -> &'static str {
    match code {
        1 => "That falls outside the build area.",
        2 => "Something is already standing there.",
        3 => "There is no deck under it.",
        4 => "There is deck there already.",
        5 => "There is not the money left for that.",
        6 => "That part is not there any more.",
        7 => "Take what is standing on it off first.",
        8 => "The page asked for something that is not a part.",
        9 => "The design is settled — nothing can be moved now.",
        10 => "There is no structure under it. The frame goes down first.",
        11 => "There is already something in that tile on that layer.",
        12 => "There is not the money left for those goods.",
        13 => "Nowhere aboard to put them — the ship needs more storage.",
        14 => "There is not that much aboard to sell.",
        15 => "Sell what is in it first.",
        16 => "There are not the materials aboard to build that.",
        17 => "This station does not sell that.",
        _ => "That could not be done.",
    }
}

/// What is wrong with the design. Indexed by `IssueCode` in
/// `crates/shipdesign/src/validate.rs`. An issue with no line here is
/// dropped from the list rather than shown as a placeholder.
pub fn issue_line(code: u32) -> Option<&'static str> {
    Some(match code {
        1 => "The ship is in more than one piece.",
        2 => "Not enough bunks for the crew.",
        3 => "Not enough chairs for the crew.",
        4 => "No table to eat at.",
        5 => "No cold store to keep food in.",
        6 => "No worktop to prepare it on.",
        7 => "No hob to cook it on.",
        8 => "No dishwasher to clear up with.",
        9 => "No toilet.",
        10 => "No basin to wash at.",
        11 => "Somewhere a Bim has to stand is blocked.",
        12 => "Parts nobody could walk between.",
        20 => "No engine — the ship goes nowhere.",
        21 => "No engine pushes it forward, so it cannot set off.",
        22 => "No hydroponic bay. The food aboard is all the food there will be.",
        23 => "No broom locker, so nothing to sweep the deck with.",
        24 => "The outside can see in. The crew will be irradiated here.",
        25 => "Nothing to eat aboard.",
        26 => "No helm, so nobody can fly it.",
        27 => "No thruster, so nothing turns the ship.",
        28 => "No airlock, so no way off it — a station can only be held beside.",
        29 => "No sensor array. Nothing will be seen beyond eyesight.",
        30 => "No fuel aboard, so no trip can be started.",
        31 => {
            "The airlock is sealed in — no side of it opens onto space, so the ship cannot dock by it. Put it in the skin."
        }
        32 => {
            "An engine is firing into the ship — the tiles behind its bell have to be open space. Put it at the stern, bell outwards."
        }
        33 => "Nothing powers this. Run conduit under it from a reactor.",
        34 => {
            "This run draws more than its reactor makes. The batteries will go flat and the ship will brown out."
        }
        _ => return None,
    })
}

/// The one issue that is not just another row: radiation is the only fault
/// on the list that kills people, it is always first, and it is styled
/// louder than the errors. It does not block; it shouts.
pub const ISSUE_GRAVE: u32 = 24;

/// Why a trip could not be planned. Indexed by `flight::PlanError`.
pub fn plan_error(code: u32) -> &'static str {
    match code {
        1 => "No engine pushes the ship forward.",
        2 => "Nothing to turn with — it cannot aim or stop.",
        3 => "No helm to fly it from.",
        4 => "Not enough fuel that is not already spoken for.",
        5 => "Nobody has found that yet.",
        6 => "The ship is already there.",
        7 => "No fuel aboard at all.",
        _ => "That cannot be flown.",
    }
}

/// Why an order did nothing. Separate from the list above on purpose: "you
/// are not docked" and "you have no fuel" are different things to be told.
pub fn refusal(why: Refusal) -> &'static str {
    match why {
        Refusal::NotDocked => "not while the ship is away from a station",
        Refusal::Unaffordable => "there is not the money",
        Refusal::NoRoomAboard => "there is nowhere aboard to put it",
        Refusal::NotAboard => "there is not that much aboard to sell",
        Refusal::NotAtTheHelm => "nobody of yours is at the helm",
        Refusal::NotTravelling => "there is no trip to stop",
        Refusal::SumTooBig => "the sum will not go",
        Refusal::ComingAlongside => "the ship is coming alongside; wait until it is tied up",
        Refusal::NotSoldHere => "this station does not sell that",
        Refusal::UnderWay => "nothing is built on a ship that is moving",
        Refusal::WontFit => "that will not go there",
        Refusal::NoSuchSite => "that site is not there any more",
        Refusal::UnderConstruction => {
            "the ship stays put while something is being built — cancel the site, or let them finish"
        }
    }
}

/// Why a blueprint will not go where the pointer is, from
/// `World::can_place_site`: the ship is moving, the rules refuse the tile
/// (an `EditError`, said the designer's way), or the ship would then have
/// a fault it has not got (an `IssueCode`, likewise).
pub fn site_refusal_line(why: world::SiteRefusal) -> String {
    match why {
        world::SiteRefusal::UnderWay => "Nothing is built while the ship is moving.".into(),
        world::SiteRefusal::WontFit(code) => edit_line(code).into(),
        world::SiteRefusal::Fault(code) => issue_line(code)
            .map(|line| format!("It would go, but then: {line}"))
            .unwrap_or_else(|| "It would leave the ship with a fault.".into()),
    }
}

/// How much of what a site is made of has reached it, in words: "2 of 2
/// metal, 0 of 1 components".
pub fn site_progress(site: &world::BuildSite, design: &shipdesign::ShipDesign) -> String {
    site.recipe(design)
        .iter()
        .map(|&(id, units)| {
            format!(
                "{} of {units} {}",
                site.delivered[id as usize].min(units),
                resource_name(id).to_lowercase()
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// What the ship is doing, indexed by `world::ShipState::code`.
pub const STATE_NAMES: [&str; 6] = [
    "Docked",
    "Holding",
    "Under way",
    "Casting off",
    "Undocking",
    "Docking",
];

/// Which part of a trip the ship is in, indexed by `flight::Phase`.
pub const PHASE_NAMES: [&str; 5] = ["Aligning", "Burning", "Turning", "Braking", "Holding"];

/// What happened, as a sentence. `None` for an event with nothing to say —
/// there are none today, and the arm is here so a new event is a missing
/// line rather than a blank row.
pub fn event_line(event: WorldEvent) -> Option<String> {
    use health::HealthEvent;
    let who = |slot: u32| crew_name(slot);
    Some(match event {
        WorldEvent::Departed { .. } => "Under way.".into(),
        WorldEvent::Arrived { station: Some(id) } => format!("Docked at station {id}."),
        WorldEvent::Arrived { station: None } => "Holding station.".into(),
        WorldEvent::Aborted { .. } => "Stopping.".into(),
        WorldEvent::PlanFailed { error, .. } => {
            format!("Cannot fly there — {}", plan_error(error.code()))
        }
        WorldEvent::Discovered { .. } => "Something new on the scanner.".into(),
        WorldEvent::FrameChanged { frame } => {
            if frame.code() == 0 {
                "Out into open space.".into()
            } else {
                "Alongside.".into()
            }
        }
        WorldEvent::Traded { units, .. } if units >= 0 => format!("{units} aboard."),
        WorldEvent::Traded { units, .. } => format!("{} sold.", -units),
        WorldEvent::Refused { why, .. } => {
            format!("That could not be done — {}.", refusal(why))
        }
        WorldEvent::CastingOff { .. } => "Casting off: everybody back aboard.".into(),
        WorldEvent::Undocking { .. } => "Clear of the berth.".into(),
        WorldEvent::Docking { station } => format!("Coming alongside station {station}."),
        WorldEvent::Crafted { recipe } => format!("Made {}.", made_name(recipe)),
        WorldEvent::CraftLost { recipe } => {
            format!(
                "Nothing made: the materials for {} were gone.",
                made_name(recipe)
            )
        }
        WorldEvent::Mined { rock, ore, galvum } => {
            if rock == 0 && ore == 0 && galvum == 0 {
                "Back from outside with nothing.".into()
            } else {
                let mut got: Vec<String> = Vec::new();
                if rock > 0 {
                    got.push(format!("{rock} rock"));
                }
                if ore > 0 {
                    got.push(format!("{ore} ore"));
                }
                if galvum > 0 {
                    got.push(format!("{galvum} galvum"));
                }
                format!("Back from outside with {}.", got.join(", "))
            }
        }
        WorldEvent::Health { who: w, event } => match event {
            HealthEvent::RadiationDetected => {
                format!("{} has picked up a dose of radiation.", who(w))
            }
            HealthEvent::CriticalDose => {
                format!("{}'s dose is critical — it is doing damage.", who(w))
            }
            HealthEvent::RadiationSickness => format!("{} has radiation sickness.", who(w)),
            HealthEvent::SicknessSubsided => {
                format!(
                    "{}'s sickness has subsided; the dose is still critical.",
                    who(w)
                )
            }
            HealthEvent::BelowCritical => {
                format!("{}'s dose is below critical again.", who(w))
            }
            HealthEvent::DoseCleared => format!("{}'s dose is clear.", who(w)),
            HealthEvent::CancerOnset => format!("{} has cancer.", who(w)),
            HealthEvent::CancerAdvanced => format!("{}'s cancer has advanced.", who(w)),
            HealthEvent::CancerTerminal => format!("{}'s cancer is terminal.", who(w)),
            HealthEvent::Died => format!("{} has died.", who(w)),
        },
        WorldEvent::SitePlaced { kind, .. } => {
            format!("{} laid out.", part_name(kind))
        }
        WorldEvent::SiteCancelled { kind } => {
            format!("{} called off.", part_name(kind))
        }
        WorldEvent::Built { kind } => format!("{} built.", part_name(kind)),
        WorldEvent::BuildLost { kind } => {
            format!(
                "{} not built: the materials had gone, or it would no longer go there.",
                part_name(kind)
            )
        }
        WorldEvent::EnemyDown { station, who: w } => {
            format!("{} is down.", resident_name(station, w))
        }
    })
}

/// What a recipe makes, in words: "4 components".
pub fn made_name(recipe: u32) -> String {
    match shipdesign::RECIPES.get(recipe as usize) {
        Some(r) => format!(
            "{} {}",
            r.output.1,
            resource_name(r.output.0).to_lowercase()
        ),
        None => "something".into(),
    }
}

/// What each kind of body is called. Indexed by `worldgen::BodyKind`.
pub const BODY_KIND_NAMES: [&str; 4] = ["Rocky planet", "Gas giant", "Ice world", "Asteroid belt"];

/// What a rock tile of a mining site is made of, by `world::Rock` code.
pub const ROCK_NAMES: [&str; 3] = ["Rock", "Iron ore", "Galvum"];

/// And each kind of station, by `worldgen::StationKind`.
pub const STATION_KIND_NAMES: [&str; 5] =
    ["Orbital", "Refinery", "Mining outpost", "Derelict", "Relay"];

/// `worldgen::StarClass`, hottest first.
pub const STAR_CLASS_NAMES: [&str; 7] = ["O", "B", "A", "F", "G", "K", "M"];

/// Star words: 48, the generator's `STAR_WORDS`. A star is "Word-Number".
pub const STAR_WORDS: [&str; 48] = [
    "Tanis", "Vesper", "Halden", "Orrin", "Cassel", "Marrow", "Ilex", "Sorrel", "Brannoc",
    "Kestrel", "Ashby", "Corvane", "Dunmere", "Ferris", "Galt", "Harrow", "Isolde", "Jarrah",
    "Kell", "Lorne", "Maund", "Nerys", "Ostrel", "Perrin", "Quill", "Rath", "Selk", "Tarn",
    "Ulric", "Varn", "Wendel", "Yarrow", "Ambrel", "Bright", "Calder", "Dray", "Elm", "Fenwick",
    "Gorse", "Hollin", "Ivory", "Juniper", "Kirsch", "Lund", "Mossley", "Nook", "Orme", "Pell",
];

/// Station words: 32, the generator's `STATION_WORDS`. A station is "Word
/// Number", with its mark after if it has one.
pub const STATION_WORDS: [&str; 32] = [
    "Cordell Yard",
    "Meridian Dock",
    "Hask Platform",
    "Verity Ring",
    "Stannard Halt",
    "Lowry Anchorage",
    "Pike Terminal",
    "Dawes Berth",
    "Kite Reach",
    "Fallow Point",
    "Greave Works",
    "Ashlar Hub",
    "Tolley Landing",
    "Wren Gantry",
    "Copper Cross",
    "Sable Moor",
    "Harkin Depot",
    "Ember Station",
    "Ninefold Yard",
    "Quarry Spur",
    "Rook Haven",
    "Tamsin Dock",
    "Ullage Post",
    "Vane Outlook",
    "Wick Refuge",
    "Yeoman Reach",
    "Zephyr Hold",
    "Brindle Wharf",
    "Carrow Deep",
    "Dolan Rest",
    "Eyrie Platform",
    "Fisk Terminus",
];

/// A star's name off its three numbers.
pub fn star_name(name: worldgen::Name) -> String {
    let word = STAR_WORDS
        .get(name.word as usize)
        .copied()
        .unwrap_or("Star");
    format!("{word}-{}", name.number)
}

/// A station's name off its three numbers.
pub fn station_name(name: worldgen::Name) -> String {
    let word = STATION_WORDS
        .get(name.word as usize)
        .copied()
        .unwrap_or("Station");
    if name.part == worldgen::name::NO_NUMBER {
        format!("{word} {}", name.number)
    } else {
        format!("{word} {} Mk {}", name.number, name.part)
    }
}

// --- the room's words ------------------------------------------------------

/// The needs, in the order the room indexes them.
pub const NEED_NAMES: [&str; 6] = [
    "Rest",
    "Food",
    "Restroom",
    "Cleanliness",
    "Socializing",
    "Washing",
];

/// Which of those get a trigger in the schedule tab.
pub const TRIGGER_NEEDS: [u32; 3] = [0, 1, 5];

/// What each bar is, for the tooltip on its row.
pub fn need_tip(need: usize) -> &'static str {
    match need {
        0 => {
            "Runs down all day. The timetable is what sends the Bim to bed on an ordinary night — the level only decides whether a scheduled night is worth taking. The trigger under the timetable is the floor beneath that: past it the Bim turns in whatever the hour, unless a meal or the heads comes first."
        }
        1 => {
            "Past its trigger — a tenth, until you move it — the Bim goes and cooks itself a meal. Empty for eight hours and malnutrition sets in; a day of it is fatal."
        }
        2 => {
            "Under 10% the Bim takes itself to the toilet. If it cannot — shut in, under orders — it fidgets, then risks wetting itself, and an hour after the bar empties it has an accident. A meal cooked in a dirty galley — every tile within two of the hob with a mess on it, a wetting or worse, is a one-in-five chance — is food poisoning: for two days this runs three times as fast and empties straight into an accident."
        }
        3 => {
            "Not a clock like the others: it follows the mess within three tiles of the Bim and whatever the Bim has on itself. At nothing it treads carefully, then keeps away from the mess, then is sick in it every half hour."
        }
        4 => {
            "The one need that wants another Bim rather than a fixture. Past its trigger the Bim goes and finds the other one, and they stand and talk about whatever they have been doing. This bar is the comfortable end of it; what matters is the count of days underneath, because going without runs on a far longer clock — three days alone and a Bim is low and slow, five and it sits down on the deck, seven and it starts hurting itself."
        }
        5 => {
            "A day's grime, on the clock like food: it runs down over the waking day and the Bim wants a shower about once a day. Past its trigger it goes and takes one, where the ship has a shower — a room without one is a Bim that goes on wanting a wash and nothing worse. Separate from Cleanliness, which is the deck around it."
        }
        _ => "",
    }
}

pub const HEALTH_TIP: &str = "Only the worst stage of malnutrition actually costs health, and eating properly walks it back. The lines underneath name whatever is wrong.";

pub const AUTONOMY_TIP: &str = "Off, the Bim starts nothing by itself — no meals, no sleep, no trips to the toilet — but still does everything it is told. The levels carry on moving either way.";

pub const TARGET_TIP: &str = "A target is a standing order: keep at least this many in the cold store. Whenever the count falls below it, the work goes on the crew's list by itself and whoever is free does it — for vegetables and tofu, planting a tray in the hydroponic bay (greens, or soy for tofu) and carrying the harvest to the store; for stew, cooking a pot on the hob out of one vegetable and one block of tofu and putting it on the shelf. Once the count is back at the target the job comes off the list, and above it nothing is grown or cooked. 0 means never. How soon it gets done is the Planting and Cooking priorities on the Work tab.";

pub const TRIGGER_TIP: &str = "How low a need may get before the Bim breaks off and does something about it: past the mark on a row it takes itself to bed, or goes and cooks, on its own account. The timetable above says when it may sleep; the Rest threshold is the floor under that — past it the Bim turns in whatever the hour, unless a meal or the heads comes first. Untick one and that need still runs down and still tells on the Bim; only the errand stops.";

pub const SPEED_TIP: &str =
    "How fast the simulation runs. At 24x a whole game day goes by in about a minute.";

pub const BUILD_TIP: &str = "Lay out a part and the crew build it, out of what is on the shelves: whoever is free carries what it is made of to the site a load at a time, then stands beside it and puts it together — Hauling and Building on the Work tab say how soon. A site beyond the hull is reached in a suit, through the airlock. Nothing is built while the ship is moving, and the ship stays put while something is being built.";

pub const ITEMS_TIP: &str = "What is aboard, by where it is kept: ore, metal and components on the shelves; fuel in the tanks; vegetables and tofu in the cold store. The food is what the crew can eat now — the cold store is refilled from the manifest at every dock.";

/// Months of the ship's calendar. Twelve of them and no leap years — see
/// `crates/game/src/clock.rs`, which does the arithmetic; these are only
/// the words.
pub const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// How a Bim says each thing it remembers, by the code from
/// `crates/game/src/memory.rs`. First person, because it is its diary. `d`
/// is the one detail that came with the entry. Only things that actually
/// went wrong are in here, because only those are written down; an entry
/// with no line here is dropped from the page rather than padded out.
pub fn memory_line(what: u32, d: u32) -> Option<String> {
    Some(match what {
        20 => {
            if d == 1 {
                "Could not hold it. I would rather not talk about it.".into()
            } else {
                "Did not quite make it to the heads.".into()
            }
        }
        21 => "Was sick on the deck.".into(),
        22 => "Dropped off where I was standing.".into(),
        23 => match d {
            1 => "Getting hungry. Properly hungry.",
            2 => "I have not eaten in a long time.",
            3 => "I am starving. I can feel it in my hands.",
            _ => "Going hungry.",
        }
        .into(),
        24 => match d {
            1 => "Tired. I should sleep.",
            2 => "I have not slept in far too long.",
            3 => "I cannot keep my eyes open.",
            _ => "Going without sleep.",
        }
        .into(),
        25 => format!("Saw {} have an accident.", crew_or(d, "one of the crew")),
        26 => format!("Saw {} being sick.", crew_or(d, "one of the crew")),
        27 => {
            if d >= 7 {
                "Nobody has spoken to me in a week.".into()
            } else if d >= 5 {
                "The quiet is starting to get to me.".into()
            } else {
                "Feeling low. It has been a few days since anyone said anything.".into()
            }
        }
        28 => "Sat down on the deck and could not get up for a while.".into(),
        29 => format!("Hurt myself. {d} points of it."),
        30 => format!("{} died today.", crew_or(d, "One of the crew")),
        31 => "Something I ate. Cooked in that galley — I have never been so ill.".into(),
        _ => return None,
    })
}

fn crew_or(who: u32, fallback: &str) -> String {
    CREW_NAMES
        .get(who as usize)
        .map(|s| s.to_string())
        .unwrap_or_else(|| fallback.to_string())
}

/// What a Bim says it is talking about, by the code from `chat_topic`.
/// Third person and short: this goes in a bubble over its head.
///
/// Two code spaces, and they do not overlap: small talk comes back as a
/// `JOB_` code from 1, and the things that happened *to* it as a
/// `memory::What` code from 20.
pub fn chat_topic(code: u32) -> &'static str {
    match code {
        1 => "cooking",
        2 => "the cooker",
        3 => "that nap",
        4 => "the night",
        5 => "the heads",
        6 => "the fridge",
        7 => "that door",
        8 => "the lock",
        9 => "the dishwasher",
        10 => "cooking",
        11 => "the bay",
        12 => "leftovers",
        13 => "the sweeping",
        20 => "an accident",
        21 => "being sick",
        22 => "dropping off",
        23 => "being hungry",
        24 => "being tired",
        25 => "what happened",
        26 => "what happened",
        27 => "how it has been",
        28 => "a bad day",
        29 => "a bad day",
        30 => "the one who died",
        31 => "a bad meal",
        _ => "nothing much",
    }
}

/// What `spot_at` says is under the pointer. Must match the `SPOT_` codes
/// in `crates/game/src/room.rs`.
pub const SPOT_NAMES: [&str; 22] = [
    "Outside the hull",
    "Deck plating",
    "Bulkhead",
    "Worktop",
    "Chopping board",
    "Cold store",
    "Hob",
    "Dishwasher",
    "Table",
    "Chair",
    "Bunk",
    "Hydroponic bay",
    "Toilet",
    "Washbasin",
    "Bathroom door",
    "Deck plating",
    "Broom locker",
    "Door",
    "Helm",
    "Workbench",
    "Suit locker",
    "Shower",
];

/// Which spots are deck: the only ones a mess can be lying on.
pub const DECK_SPOTS: [u32; 2] = [1, 15];

/// The spots that are only a word for *where* the pointer is — outside,
/// deck, a bulkhead — rather than a thing the room has a picture of. Aboard
/// the ship every part the room does not draw reads as one of these, so the
/// ship's readout names those off the design instead.
pub const PLAIN_SPOTS: [u32; 4] = [0, 1, 2, 15];

/// What is on the deck there, by the code from `spot_mess`. Ordered least
/// bad first, the same as `filth::Mess`.
pub const MESS_NAMES: [&str; 5] = ["", "Grime", "Wet", "Soiled", "Vomit"];

/// The three stages of going without sleep.
pub const DROWSINESS: [&str; 4] = [
    "",
    "Sleepy — fumbling, errands a quarter longer",
    "Sleep deprived — errands half as long again",
    "Past it — errands twice as long, and dropping off on its feet",
];

/// The three stages of going without food.
pub const CONDITIONS: [&str; 4] = [
    "",
    "Mild malnutrition — moving slowly",
    "Malnutrition — slower, and tiring twice as fast",
    "Extreme malnutrition — losing health",
];

/// How badly the Bim needs the toilet.
pub const URGES: [&str; 4] = [
    "",
    "Needs the toilet — fidgeting",
    "Needs the toilet badly — may not make it",
    "Bursting — an accident within the hour",
];

/// How far gone it is for want of a clean place to stand.
pub const DISCOMFORTS: [&str; 4] = [
    "",
    "Uneasy about the mess — treading carefully",
    "Sickened by the mess — keeping away from it",
    "Sickened by the mess — being sick in it every half hour",
];

/// And for want of anybody to talk to.
pub const LONELINESS: [&str; 4] = [
    "",
    "Desocialized — low, and a tenth slower at everything",
    "Badly desocialized — sits down on the deck every few hours",
    "Isolated — hurting itself, and past ten days it may stop altogether",
];

/// The errand codes shared by `activity()` and `agenda_job()`.
pub fn job_name(code: u32) -> &'static str {
    match code {
        1 => "Making food",
        2 => "Working the cooker",
        3 => "Nap",
        4 => "Sleep",
        5 => "Using the toilet",
        6 => "Fridge door",
        7 => "Bathroom door",
        8 => "Door lock",
        9 => "Starting the wash",
        11 => "Tending the bay",
        12 => "Eating leftovers",
        13 => "Sweeping up",
        14 => "Talking",
        15 => "Cooking stew for the store",
        16 => "Warming up a stew",
        17 => "Taking a shower",
        18 => "Making something",
        19 => "Mining outside",
        20 => "Carrying materials",
        21 => "Building",
        _ => "Busy",
    }
}

/// The same errands as the status line says them. A lie-down is left out:
/// the countdown from `rest_left()` is more use than the name.
pub fn activity_line(code: u32) -> Option<&'static str> {
    Some(match code {
        1 => "Making food…",
        2 => "Off to the cooker…",
        5 => "Using the toilet — a wash to follow",
        11 => "In the hydroponics…",
        12 => "Helping itself to the pot…",
        13 => "Sweeping the deck…",
        14 => "Having a word with the other one…",
        15 => "Cooking a stew for the store…",
        16 => "Warming a stew through…",
        17 => "In the shower…",
        18 => "At the bench…",
        19 => "Outside, mining…",
        20 => "Carrying a load to the site…",
        21 => "Building…",
        _ => return None,
    })
}

/// What `order_move()` made of a right-click. Only the refusals are worth
/// saying out loud; the rest the Bim shows you by walking.
pub fn order_refused(code: u32) -> Option<&'static str> {
    match code {
        3 => Some("Can't get there — the bathroom door is locked."),
        4 => Some("Can't get there at all."),
        _ => None,
    }
}

/// The jobs on the work list, by `work::Job` code, and which fixture each
/// is about so resting on a row rings the place it happens.
pub const WORK_NAMES: [&str; 9] = [
    "Cleaning",
    "Planting",
    "Plant cutting",
    "Hauling",
    "Cooking",
    "Controlling the ship",
    "Making things",
    "Mining outside",
    "Building",
];

pub const IDLE_HINT: &str = "Click a fixture for its menu · 1 or drag to select · right-click the floor to move · r to recruit";

// --- arms and armour ------------------------------------------------------------

/// What a weapon is called, indexed by `bims::combat::WeaponKind::code`;
/// `0` is an empty slot.
pub const WEAPON_NAMES: [&str; 2] = ["—", "Laser pistol"];

/// What a piece of armour is called, indexed by `bims::combat::ArmourKind::code`;
/// `0` is an empty slot. Nothing to wear yet.
pub const ARMOUR_NAMES: [&str; 1] = ["—"];

pub fn weapon_name(kind: Option<bims::combat::WeaponKind>) -> &'static str {
    WEAPON_NAMES[kind.map(|k| k.code() as usize).unwrap_or(0)]
}

pub fn armour_name(kind: Option<bims::combat::ArmourKind>) -> &'static str {
    ARMOUR_NAMES[kind.map(|k| k.code() as usize).unwrap_or(0)]
}

/// The three armour slots, top to bottom, and the weapon's.
pub const SLOT_NAMES: [&str; 4] = ["Head", "Body", "Legs", "Weapon"];

pub const INVENTORY_TIP: &str = "What the Bim has on it: head, body and leg protection down the left, the weapon in hand on the right. Recruited, a Bim is in combat mode — it draws the weapon and shoots at any enemy it can see and reach. Nothing can be changed yet.";
pub const ACCURACY_TIP: &str =
    "The odds of a shot landing on somebody ten tiles away. Nearer is better, further is worse.";
pub const DPS_TIP: &str =
    "Damage a second with every shot landing: the fire rate times the damage a shot does.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_part_has_a_name_and_every_name_a_part() {
        assert_eq!(PART_NAMES.len(), PartKind::ALL.len());
        for &kind in PartKind::ALL.iter() {
            assert!(!part_name(kind).is_empty());
        }
    }

    #[test]
    fn every_resource_and_storage_class_has_a_name() {
        assert_eq!(RESOURCE_NAMES.len(), ResourceId::ALL.len());
        assert_eq!(STORAGE_NAMES.len(), shipdesign::Storage::ALL.len());
    }

    #[test]
    fn the_word_tables_are_as_long_as_the_generator_expects() {
        assert_eq!(STAR_WORDS.len(), worldgen::name::STAR_WORDS as usize);
        assert_eq!(STATION_WORDS.len(), worldgen::name::STATION_WORDS as usize);
    }

    #[test]
    fn every_issue_the_validator_can_raise_has_a_line() {
        // The codes are written out in `IssueCode` and never renumbered:
        // 1 to 12 and 20 to 34, with the gap on purpose.
        for code in (1..=12).chain(20..=34) {
            assert!(issue_line(code).is_some(), "issue {code} has no line");
        }
    }

    #[test]
    fn the_work_list_names_every_job() {
        assert_eq!(WORK_NAMES.len(), bims::work::Job::ALL.len());
    }

    #[test]
    fn every_weapon_has_a_name_and_the_empty_slot_is_first() {
        // Slot 0 is empty; every kind's code indexes its name.
        assert_eq!(WEAPON_NAMES.len(), bims::combat::WeaponKind::ALL.len() + 1);
        for &kind in bims::combat::WeaponKind::ALL.iter() {
            assert!(!weapon_name(Some(kind)).is_empty());
            assert_ne!(weapon_name(Some(kind)), WEAPON_NAMES[0]);
        }
        // No armour exists yet: the table is the empty slot alone.
        assert_eq!(ARMOUR_NAMES.len(), 1);
        assert_eq!(armour_name(None), ARMOUR_NAMES[0]);
    }

    #[test]
    fn every_event_has_a_line() {
        // A code with no sentence is a row that never appears; the newest
        // event is the one most likely to have been forgotten.
        assert!(event_line(WorldEvent::EnemyDown { station: 3, who: 1 }).is_some());
    }

    #[test]
    fn every_buildable_part_is_in_one_build_group() {
        for &kind in PartKind::ALL.iter() {
            let code = kind as u32;
            let groups = BUILD_GROUPS
                .iter()
                .filter(|(_, _, kinds)| kinds.contains(&code))
                .count();
            let expected = if NOT_A_TOOL.contains(&code) { 0 } else { 1 };
            assert_eq!(groups, expected, "{kind:?} is in {groups} build groups");
        }
    }
}
