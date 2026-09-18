//! The crew's panels: everything on a screen that is about the Bims rather
//! than about the deck they are standing on. Shared by the room and the
//! ship, which run the same room — aboard, stepped by the world.
//!
//! One copy on purpose: the selected crew member's needs and health and
//! diary, the agendas, the tray with the timetable, the work list and the
//! management row, the fixture menus, and the tooltips everything hangs
//! off. What is *not* here is the canvas and the pointer, because those are
//! each screen's own — the room fits the deck to a window and the ship turns
//! it with the hull — and the screen hands the room its coordinates.
//!
//! # Tooltips are asked for, never stumbled into
//!
//! Every tooltip hangs off an affordance: a word underlined with
//! [`theme::asks`], or a `?` from [`theme::question_mark`]. Never a row, a
//! bar or a panel — a player crossing the needs panel on the way to the deck
//! should get nothing.
//!
//! # A highlight is not a tooltip
//!
//! Resting on a management row rings the fixture it names on the deck.
//! Nothing pops up, nothing is said, and the ring is gone the moment the
//! pointer moves — [`CrewPanels::points`] is the mechanism, and it hangs off
//! a whole row on purpose, because every row is about exactly one place.
//! The ring is worked out afresh every frame from what is hovered, so a
//! panel that folds away under the pointer cannot leave a fixture lit.

use bevy_egui::egui;
use bims::game::Game;
use bims::manager::Stock;
use bims::room::*;
use bims::{bim, door, health, manager, schedule, task};
use physics::ResourceId;
use ship::game::Overlay;
use shipdesign::CARGO_SLOTS;
use shipdesign::parts::PartKind;

use crate::format::{clock_text, date_text, span_text};
use crate::names::*;
use crate::theme;

/// Below this the pointer moved so little that it counts as a click, not a
/// sweep, in points.
pub const CLICK_SLOP: f32 = 4.0;

/// Which fixture answers each of the three targets: the bay grows the
/// first two, the hob makes the third. Indexed by `manager::Stock`.
const KEEP_SPOTS: [u32; 3] = [SPOT_BAY, SPOT_BAY, SPOT_HOB];

/// Which fixture each job on the work list is about, so resting on a row
/// rings the place it happens. Hauling is the crop carry, and where a haul
/// *ends* is the thing worth pointing at.
const WORK_SPOTS: [u32; 9] = [
    SPOT_LOCKER,
    SPOT_BAY,
    SPOT_BAY,
    SPOT_FRIDGE,
    SPOT_HOB,
    SPOT_HELM,
    SPOT_BENCH,
    SPOT_SUIT_LOCKER,
    // A site is wherever it was laid out; nothing fixed to ring.
    SPOT_NOTHING,
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Schedule,
    Work,
    Management,
    /// What the selected crew member has on it: the armour slots and
    /// the weapon, with its numbers. On both screens.
    Inventory,
    /// What the ship view is drawn to show over the ship: the plain deck,
    /// or the electricity. Only on the ship's screen, like Actions.
    View,
    /// Things the player does with the pointer on the crew's behalf. Only
    /// on the ship's screen: the room has no outside.
    Actions,
    /// Parts to lay out for the crew to build, by category, with a search
    /// box over them; and the sites laid out so far. Only on the ship's
    /// screen: the room is not a ship.
    Build,
    /// The helm and the ship's facts. Only on the ship's screen, and drawn
    /// by it: the tray lays out the tabs and leaves the body to the
    /// caller, since everything on it is the world's rather than the
    /// room's.
    Ship,
}

/// A tool the pointer is holding, picked on the Actions tab. One at a time,
/// and none is the ordinary pointer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    /// Marking rocks outside to be mined: a click on a rock marks it, a
    /// second click unmarks it.
    Mine,
    /// Laying out a part of this kind: a blueprint follows the pointer, `R`
    /// turns it, and a click lays it down as a construction site for the
    /// crew to carry to and build.
    Build(PartKind),
}

/// One thing the benches make, for the Management tab's second table: what
/// it is, how many are aboard, where they are kept, the standing order to
/// keep that many made, and the most the hold could take.
pub struct Craft {
    pub resource: ResourceId,
    pub held: u32,
    pub target: u32,
    pub most: u32,
    pub kept_in: &'static str,
    /// The recipe in words, for the tooltip.
    pub recipe: String,
}

/// What the Actions tab has to know that the room does not: whether the
/// ship is at a mining site and how many rocks are marked, and — going the
/// other way — that the marks are to be cleared.
pub struct Actions {
    pub at_site: bool,
    pub marked: usize,
    /// How many of those a walk could get to: the rest wait on a rock in
    /// front of them.
    pub reachable: usize,
    pub clear: bool,
    /// And what the View tab shows over the ship. Read in and written
    /// back, like `clear`.
    pub overlay: Overlay,
    /// What the benches make, for the Management tab, and — going the
    /// other way — the targets changed there this frame, as `Order::Keep`s
    /// to be.
    pub crafts: Vec<Craft>,
    pub keep: Vec<(ResourceId, u32)>,
    /// The Build tab: whether the ship is at rest, which is the only time
    /// anything is laid out or built; how much of each material is aboard
    /// and not spoken for by a site, by `ResourceId`; the sites laid out;
    /// and — going the other way — the sites to be called off.
    pub at_rest: bool,
    pub free: [u32; CARGO_SLOTS],
    pub sites: Vec<Site>,
    pub cancel: Vec<u32>,
    /// The camera, for the View tab: which way is up, and whether it
    /// follows the crew member you steer. Read in and written back.
    pub head_up: bool,
    pub follow: bool,
    /// Whether the ship is docked, which is when there is a station to
    /// trade with; and — going the other way — that the Station button
    /// was pressed this frame.
    pub docked: bool,
    pub station: bool,
}

/// One construction site, for the Build tab's list: what and where, how
/// much of what it is made of has been carried to it in words, and whether
/// all of it has.
pub struct Site {
    pub id: u32,
    pub kind: PartKind,
    pub at: (u32, u32),
    pub progress: String,
    pub stocked: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sort {
    Up,
    Down,
    Name,
}

/// An open fixture menu: which fixture, and where on the window it was
/// asked for.
pub struct Menu {
    fixture: u32,
    at: egui::Pos2,
    /// Opened this frame: the press that opened it must not shut it.
    fresh: bool,
}

/// What a menu row does when it is clicked: walks the player's Bim over
/// to do it.
type Errand = Box<dyn FnOnce(&mut Game)>;

/// One row of a fixture's menu.
struct Item {
    label: String,
    hint: String,
    disabled: bool,
    run: Option<Errand>,
}

impl Item {
    fn note(label: &str, hint: String) -> Item {
        Item {
            label: label.into(),
            hint,
            disabled: true,
            run: None,
        }
    }

    fn run(
        label: impl Into<String>,
        hint: impl Into<String>,
        disabled: bool,
        f: impl FnOnce(&mut Game) + 'static,
    ) -> Item {
        Item {
            label: label.into(),
            hint: hint.into(),
            disabled,
            run: Some(Box::new(f)),
        }
    }
}

pub struct CrewPanels {
    /// The one the mouse steers.
    pub player: usize,
    pub crew_count: u32,
    tray_open: bool,
    tab: Tab,
    /// The tool the pointer is holding, if any. See [`Tool`].
    pub tool: Option<Tool>,
    brush: u32,
    /// Whether a drag across the timetable is under way.
    painting: bool,
    /// The character sheet's page, per crew member: `true` for the diary.
    diary_open: Vec<bool>,
    sort: Option<Sort>,
    /// The spot the pointer's row rang last frame, and the one the rows
    /// hovered this frame want.
    ringed: u32,
    wanted: u32,
    menu: Option<Menu>,
    /// The Build tab: what is typed in its search box, and which category
    /// is open — `None` for the list of categories.
    search: String,
    open_group: Option<usize>,
    /// The inventory pop-up: up from the moment the player's crew member
    /// is recruited until it is shut or they are let go, and whether they
    /// were recruited last frame, which is how the moment is noticed.
    inventory_open: bool,
    was_recruited: bool,
}

impl CrewPanels {
    pub fn new(player: usize, crew_count: u32) -> CrewPanels {
        CrewPanels {
            player,
            crew_count,
            tray_open: true,
            tab: Tab::Schedule,
            tool: None,
            brush: 1,
            painting: false,
            diary_open: vec![false; crew_count as usize],
            sort: Some(Sort::Up),
            ringed: SPOT_NOTHING,
            wanted: SPOT_NOTHING,
            menu: None,
            search: String::new(),
            open_group: None,
            inventory_open: false,
            was_recruited: false,
        }
    }

    /// The room's crew has changed size — a ship docking brings a station's
    /// residents into its room, and leaving takes them out again.
    pub fn rebuild_crew(&mut self, count: u32) {
        if count == self.crew_count {
            return;
        }
        self.crew_count = count;
        self.diary_open = vec![false; count as usize];
        self.menu = None;
    }

    // --- pointing at the thing itself ---------------------------------------

    /// Ring `spot` on the deck while the pointer is on `response`. Rows are
    /// drawn outer first, so a cell inside a row that points somewhere else
    /// wins by coming later.
    pub fn points(&mut self, response: &egui::Response, spot: u32) {
        if response.hovered() && spot != SPOT_NOTHING {
            self.wanted = spot;
        }
    }

    /// Call once a frame, before any panel: nothing wants a ring yet.
    pub fn begin_frame(&mut self) {
        self.wanted = SPOT_NOTHING;
    }

    /// Call once a frame, after every panel: the ring follows the pointer.
    pub fn end_frame(&mut self, game: &mut Game) {
        if self.wanted != self.ringed {
            self.ringed = self.wanted;
            game.set_highlight(self.ringed);
        }
    }

    // --- fixture menus ------------------------------------------------------

    pub fn open_menu(&mut self, fixture: u32, at: egui::Pos2) {
        self.menu = Some(Menu {
            fixture,
            at,
            fresh: true,
        });
    }

    pub fn close_menu(&mut self) {
        self.menu = None;
    }

    pub fn menu_open(&self) -> bool {
        self.menu.is_some()
    }

    /// Who has the run of a shared part of the ship, or `None` for nobody.
    /// The room counts from 1 so that 0 can mean "free"; this turns that
    /// back into a name, and into `None` when it is the player's own Bim —
    /// being told you cannot cook because you are already cooking is no
    /// help.
    fn held_by(&self, code: u32, name: &dyn Fn(u32) -> String) -> Option<String> {
        if code == 0 || code as usize - 1 == self.player {
            return None;
        }
        Some(name(code - 1))
    }

    /// Build the rows for the fixture that was clicked. Nothing here is
    /// remote: every item walks the Bim over to do it by hand. Nor does
    /// anything wait for the Bim to be free — a new errand takes over, and
    /// what it displaced goes on the agenda to be finished afterwards.
    /// Every menu acts on the player's Bim; the other crew take no orders.
    fn items(&self, fixture: u32, game: &Game, name: &dyn Fn(u32) -> String) -> Vec<Item> {
        let who = self.player;
        let busy = game.is_busy(who);
        let takes_over: Option<&str> = busy.then_some("takes over — the rest waits its turn");
        // The fixture the click landed on, of each kind — there may be
        // several of any of them — and who has it. A meal is kept from
        // starting only when no galley at all is free.
        let (hob, fridge, washer) = (game.hit_hob(), game.hit_fridge(), game.hit_dishwasher());
        let (bath, shower_i, locker) = (game.hit_bath(), game.hit_shower(), game.hit_locker());
        let galley = self.held_by(game.galley_busy_by(who), name);
        let hob_held = self.held_by(game.hob_held_by(hob), name);
        let fridge_held = self.held_by(game.fridge_held_by(fridge), name);
        let washer_held = self.held_by(game.dishwasher_held_by(washer), name);
        let heads = self.held_by(game.heads_held_by(bath), name);
        let shower = self.held_by(game.shower_held_by(shower_i), name);
        let in_galley = |g: &Option<String>| g.as_ref().map(|n| format!("{n} is in the galley"));
        let mut items = Vec::new();
        match fixture {
            HIT_FRIDGE => {
                let veg = game.store_veg();
                let tofu = game.store_tofu();
                let no_stew = veg < 2;
                let no_bowl = tofu < 1 || veg < 1;
                items.push(Item::note(
                    "Cold store",
                    format!(
                        "{veg} veg, {tofu} tofu, {} stew ready — keeping {}, {} and {}",
                        game.store_stew(),
                        game.target(Stock::Veg),
                        game.target(Stock::Tofu),
                        game.target(Stock::Stew)
                    ),
                ));
                items.push(Item::run(
                    "Make a stew",
                    in_galley(&galley).unwrap_or_else(|| {
                        if no_stew {
                            "needs two vegetables".into()
                        } else {
                            takes_over
                                .unwrap_or("two vegetables, chopped and cooked")
                                .into()
                        }
                    }),
                    no_stew || galley.is_some(),
                    move |g| {
                        g.cook(who, Dish::Stew);
                    },
                ));
                items.push(Item::run(
                    "Make a bowl",
                    in_galley(&galley).unwrap_or_else(|| {
                        if no_bowl {
                            "needs a block of tofu and a salad".into()
                        } else {
                            takes_over
                                .unwrap_or("tofu chopped in with the salad, no cooking")
                                .into()
                        }
                    }),
                    no_bowl || galley.is_some(),
                    move |g| {
                        g.cook(who, Dish::Bowl);
                    },
                ));
                items.push(Item::run(
                    if game.fridge_is_open(fridge) {
                        "Close door"
                    } else {
                        "Open door"
                    },
                    in_galley(&fridge_held)
                        .unwrap_or_else(|| takes_over.unwrap_or("the Bim walks over to it").into()),
                    fridge_held.is_some(),
                    move |g| g.toggle_fridge(who, fridge),
                ));
            }
            HIT_STOVE => {
                let left = game.pot_servings(hob);
                if left > 0 {
                    let capacity = game.pot_capacity();
                    items.push(Item::run(
                        "Eat from the pot",
                        in_galley(&galley).unwrap_or_else(|| {
                            takes_over.map(|s| s.to_string()).unwrap_or_else(|| {
                                format!("{left} of {capacity} helpings left — no cooking")
                            })
                        }),
                        galley.is_some(),
                        move |g| {
                            g.eat_leftovers(who);
                        },
                    ));
                }
                let no_stock = game.store_veg() < 1 || game.store_tofu() < 1;
                let ready = game.store_stew();
                let keeping = game.target(Stock::Stew);
                items.push(Item::run(
                    "Cook a stew for the store",
                    in_galley(&galley).unwrap_or_else(|| {
                        if no_stock {
                            "needs a vegetable and a block of tofu".into()
                        } else {
                            takes_over
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| format!("{ready} ready — keeping {keeping}"))
                        }
                    }),
                    no_stock || galley.is_some(),
                    move |g| {
                        g.make_stew(who);
                    },
                ));
                let idle_left = game.stove_idle_left(hob);
                items.push(Item::run(
                    if game.stove_is_on(hob) {
                        "Turn off"
                    } else {
                        "Turn on"
                    },
                    in_galley(&hob_held).unwrap_or_else(|| {
                        takes_over.map(|s| s.to_string()).unwrap_or_else(|| {
                            if idle_left > 0.0 {
                                format!("left on — cuts out in {}", span_text(idle_left))
                            } else {
                                "the Bim walks over to it".into()
                            }
                        })
                    }),
                    hob_held.is_some(),
                    move |g| g.toggle_stove(who, hob),
                ));
            }
            HIT_DISHWASHER => {
                let loaded = game.dishwasher_loaded(washer);
                let capacity = game.dishwasher_capacity();
                let left = game.dishwasher_cycle_left(washer);
                let now = game.clock_minutes();
                if left > 0.0 {
                    items.push(Item::note(
                        "Running",
                        format!(
                            "{} left — done at {}",
                            span_text(left),
                            clock_text(now + left)
                        ),
                    ));
                }
                items.push(Item::run(
                    "Run now",
                    in_galley(&washer_held).unwrap_or_else(|| {
                        if left > 0.0 {
                            "already running".into()
                        } else if loaded == 0 {
                            "nothing in it".into()
                        } else {
                            takes_over.map(|s| s.to_string()).unwrap_or_else(|| {
                                format!(
                                    "{loaded} of {capacity} stowed — the Bim goes and presses it"
                                )
                            })
                        }
                    }),
                    left > 0.0 || loaded == 0 || washer_held.is_some(),
                    move |g| g.run_dishwasher(who, washer),
                ));
            }
            HIT_SHOWER => {
                let can = game.can_shower(who);
                items.push(Item::run(
                    "Take a shower",
                    shower
                        .as_ref()
                        .map(|n| format!("{n} is in it"))
                        .unwrap_or_else(|| {
                            if can {
                                takes_over.unwrap_or("and come out clean").into()
                            } else {
                                "can't get to it".into()
                            }
                        }),
                    !can || shower.is_some(),
                    move |g| {
                        g.take_shower(who);
                    },
                ));
            }
            HIT_TOILET => {
                let can = game.can_use_toilet(who);
                items.push(Item::run(
                    "Use",
                    heads
                        .as_ref()
                        .map(|n| format!("{n} is in there"))
                        .unwrap_or_else(|| {
                            if can {
                                takes_over.unwrap_or("and wash at the basin after").into()
                            } else {
                                "can't get to it — the door is locked".into()
                            }
                        }),
                    !can || heads.is_some(),
                    move |g| {
                        g.use_toilet(who);
                    },
                ));
            }
            HIT_DOOR => {
                let open = game.door_is_open();
                let locked = game.door_is_locked();
                let in_there = heads.as_ref().map(|n| format!("{n} is in there"));
                items.push(Item::run(
                    if open { "Close door" } else { "Open door" },
                    in_there.clone().unwrap_or_else(|| {
                        if locked {
                            "unlock it first".into()
                        } else {
                            takes_over.unwrap_or("the Bim walks over to it").into()
                        }
                    }),
                    locked || heads.is_some(),
                    move |g| g.toggle_door(who),
                ));
                items.push(Item::run(
                    if locked { "Unlock" } else { "Lock" },
                    in_there.unwrap_or_else(|| {
                        takes_over
                            .unwrap_or(if locked {
                                "at the panel"
                            } else {
                                "shuts it as well"
                            })
                            .into()
                    }),
                    heads.is_some(),
                    move |g| g.toggle_door_lock(who),
                ));
            }
            HIT_SHIP_DOOR => {
                // A powered door in a bulkhead: "Open" holds it open,
                // "Close" hands it back to itself, and a locked door is a
                // wall until it is unlocked.
                let door = game.hit_door();
                let held = game.ship_door_is_held(door);
                let locked = game.ship_door_is_locked(door);
                items.push(Item::run(
                    if held { "Close door" } else { "Hold door open" },
                    if locked {
                        "unlock it first".to_string()
                    } else {
                        takes_over
                            .unwrap_or(if held {
                                "and let it shut behind people again"
                            } else {
                                "so it stops shutting itself"
                            })
                            .into()
                    },
                    locked,
                    move |g| {
                        g.order_door(
                            who,
                            door,
                            if held {
                                door::Order::Close
                            } else {
                                door::Order::Open
                            },
                        )
                    },
                ));
                items.push(Item::run(
                    if locked { "Unlock" } else { "Lock" },
                    takes_over
                        .unwrap_or(if locked {
                            "at the panel"
                        } else {
                            "shuts it, and nobody gets through"
                        })
                        .to_string(),
                    false,
                    move |g| {
                        g.order_door(
                            who,
                            door,
                            if locked {
                                door::Order::Unlock
                            } else {
                                door::Order::Lock
                            },
                        )
                    },
                ));
            }
            HIT_LOCKER => {
                let dirty = game.dirty_tiles();
                let broom = self.held_by(game.broom_held_by(locker), name);
                items.push(Item::run(
                    "Sweep up",
                    broom
                        .as_ref()
                        .map(|n| format!("{n} has the broom"))
                        .unwrap_or_else(|| {
                            if dirty == 0 {
                                "the deck is clean".into()
                            } else {
                                takes_over.map(|s| s.to_string()).unwrap_or_else(|| {
                                    format!(
                                        "{dirty} patch{} of deck want it",
                                        if dirty == 1 { "" } else { "es" }
                                    )
                                })
                            }
                        }),
                    dirty == 0 || broom.is_some(),
                    move |g| {
                        g.sweep_up(who);
                    },
                ));
            }
            HIT_HYDRO => {
                // The bay the click landed on: each has its own trays, its
                // own switch and its own standing order.
                let bay = game.hit_bay();
                let spots = game.hydro_spots();
                let automated = game.hydro_automated(bay);
                let asleep = game.hydro_hibernating(bay);
                let forced = game.hydro_forced(bay);
                let ripe = game.hydro_ripe(bay);
                let mut growing = 0;
                let mut furthest: f32 = 0.0;
                for i in 0..spots {
                    if game.hydro_crop(bay, i) == 0 {
                        continue;
                    }
                    growing += 1;
                    furthest = furthest.max(game.hydro_growth(bay, i));
                }
                let along = if ripe > 0 || growing == 0 {
                    String::new()
                } else {
                    format!(", furthest {}% grown", (furthest * 100.0).round())
                };
                items.push(Item::note(
                    "Trays",
                    format!(
                        "{growing} of {spots} planted{}",
                        if ripe > 0 {
                            format!(", {ripe} ready to lift")
                        } else {
                            along
                        }
                    ),
                ));
                items.push(Item::note(
                    "Store",
                    format!(
                        "{} veg, {} tofu — keeping {} and {}",
                        game.store_veg(),
                        game.store_tofu(),
                        game.target(Stock::Veg),
                        game.target(Stock::Tofu)
                    ),
                ));
                items.push(Item::run(
                    if automated {
                        "Stop automating"
                    } else {
                        "Automate"
                    },
                    if automated {
                        if asleep {
                            "at target — holding what is planted"
                        } else {
                            "following the manager's target"
                        }
                    } else {
                        "grow whatever the store is short of"
                    },
                    false,
                    move |g| g.set_hydro_automated(bay, !automated),
                ));
                for (code, label, what) in [
                    (1, "Plant greens in every tray", "two of these in a stew"),
                    (
                        2,
                        "Plant soy in every tray",
                        "a day and a half, and it presses into tofu",
                    ),
                ] {
                    let on = forced == code;
                    items.push(Item::run(
                        if on {
                            format!("{label} ✓")
                        } else {
                            label.to_string()
                        },
                        if on {
                            "standing order — click to lift it".to_string()
                        } else {
                            format!("no matter the target · {what}")
                        },
                        false,
                        move |g| g.set_hydro_forced(bay, if on { 0 } else { code }),
                    ));
                }
            }
            HIT_BED => {
                let now = game.clock_minutes();
                for (minutes, label) in [(task::NAP_MINUTES, "Nap"), (task::SLEEP_MINUTES, "Sleep")]
                {
                    items.push(Item::run(
                        format!("{label} — {}", span_text(minutes)),
                        takes_over
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| format!("up around {}", clock_text(now + minutes))),
                        false,
                        move |g| {
                            g.rest(who, minutes);
                        },
                    ));
                }
            }
            _ => {}
        }
        items
    }

    /// Draw the open menu, if there is one, at the cursor it was opened
    /// at, and shut it on a click away.
    pub fn menu(&mut self, ctx: &egui::Context, game: &mut Game, name: &dyn Fn(u32) -> String) {
        let Some(menu) = &self.menu else { return };
        if !game.is_alive(self.player) {
            self.menu = None;
            return;
        }
        let items = self.items(menu.fixture, game, name);
        if items.is_empty() {
            self.menu = None;
            return;
        }
        let at = menu.at;
        let fresh = menu.fresh;
        let mut chosen: Option<Errand> = None;
        let response = egui::Area::new(egui::Id::new("fixture-menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(at)
            .constrain(true)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .fill(theme::PANEL)
                    .show(ui, |ui| {
                        ui.set_min_width(200.0);
                        for item in items {
                            let mut text = egui::text::LayoutJob::default();
                            text.append(
                                &item.label,
                                0.0,
                                egui::TextFormat {
                                    color: if item.disabled {
                                        theme::MUTED
                                    } else {
                                        theme::INK
                                    },
                                    ..Default::default()
                                },
                            );
                            if !item.hint.is_empty() {
                                text.append(
                                    &format!("\n{}", item.hint),
                                    0.0,
                                    egui::TextFormat {
                                        font_id: egui::FontId::proportional(11.0),
                                        color: theme::MUTED,
                                        ..Default::default()
                                    },
                                );
                            }
                            let button = egui::Button::new(text)
                                .frame(false)
                                .min_size(egui::vec2(200.0, 0.0));
                            let clicked = ui.add_enabled(!item.disabled, button).clicked();
                            if clicked {
                                chosen = item.run;
                            }
                        }
                    });
            });
        if let Some(run) = chosen {
            run(game);
            self.menu = None;
            return;
        }
        // Clicking away dismisses it — but not the press that opened it.
        let pressed = ctx.input(|i| i.pointer.any_pressed());
        let pos = ctx.input(|i| i.pointer.interact_pos());
        if let Some(menu) = &mut self.menu {
            if fresh {
                menu.fresh = false;
            } else if pressed && !pos.is_some_and(|p| response.response.rect.contains(p)) {
                self.menu = None;
            }
        }
    }

    // --- what the crew want -------------------------------------------------

    /// The header that says whose panel this is.
    fn who_header(&self, ui: &mut egui::Ui, who: u32, name: &dyn Fn(u32) -> String) {
        ui.horizontal(|ui| {
            let yours = who as usize == self.player;
            ui.label(egui::RichText::new(name(who)).strong().color(if yours {
                theme::YOURS
            } else {
                theme::INK
            }));
            ui.label(
                egui::RichText::new(if yours { "yours" } else { "her own" })
                    .small()
                    .color(theme::MUTED),
            );
        });
    }

    /// The selected crew member's panels: the bars, the health lines and
    /// the character sheet. One at a time, and only when somebody is
    /// picked: the right-hand side answers "who am I looking at", not
    /// "what is everybody up to". `true` when something was drawn.
    pub fn side(
        &mut self,
        ui: &mut egui::Ui,
        game: &mut Game,
        name: &dyn Fn(u32) -> String,
    ) -> bool {
        let Some(who) = (0..self.crew_count).find(|&w| game.is_selected(w as usize)) else {
            return false;
        };
        let w = who as usize;
        let alive = game.is_alive(w);
        self.who_header(ui, who, name);

        // The needs. Only the first crew member's words are affordances:
        // the explanation is the same for everybody and two sets of
        // underlines down one edge of the screen is noise, not help.
        egui::Grid::new(("needs", who))
            .num_columns(3)
            .spacing([8.0, 3.0])
            .show(ui, |ui| {
                for i in 0..game.need_count() {
                    let label = NEED_NAMES.get(i as usize).copied().unwrap_or("Need");
                    let level = game.need_level(w, i);
                    let urgent = game.need_trigger_on(i) && level < game.need_trigger(i);
                    let color = if urgent { theme::WARN } else { theme::INK };
                    if who == 0 {
                        theme::asks(ui, label, need_tip(i as usize));
                    } else {
                        ui.label(egui::RichText::new(label).color(color));
                    }
                    theme::bar(
                        ui,
                        110.0,
                        level,
                        if urgent { theme::WARN } else { theme::ACCENT },
                    );
                    ui.label(
                        egui::RichText::new(format!("{}%", (level * 100.0).round()))
                            .small()
                            .color(theme::MUTED),
                    );
                    ui.end_row();
                }
            });

        // How it is bearing up.
        ui.add_space(4.0);
        let points = game.health(w);
        let stage = game.malnutrition(w);
        let hurt = stage >= 3 || !alive;
        ui.horizontal(|ui| {
            if who == 0 {
                theme::asks(ui, "Health", HEALTH_TIP);
            } else {
                ui.label("Health");
            }
            theme::bar(
                ui,
                110.0,
                points / health::MAX_HEALTH,
                if hurt { theme::BAD } else { theme::ACCENT },
            );
            ui.label(
                egui::RichText::new(format!("{}", points.round()))
                    .small()
                    .color(theme::MUTED),
            );
        });
        let line = |ui: &mut egui::Ui, text: String, warn: bool, gone: bool| {
            if text.is_empty() {
                return;
            }
            let color = if gone {
                theme::MUTED
            } else if warn {
                theme::CAUTION
            } else {
                theme::INK
            };
            ui.label(egui::RichText::new(text).small().color(color));
        };
        line(
            ui,
            if alive {
                CONDITIONS[stage as usize].to_string()
            } else {
                format!("{} has died.", name(who))
            },
            stage > 0,
            !alive,
        );
        let tired = game.drowsiness(w);
        line(
            ui,
            if alive {
                DROWSINESS[tired as usize].into()
            } else {
                String::new()
            },
            tired > 0,
            false,
        );
        // Both of these are stages reached by a clock rather than levels,
        // so the bars above cannot show them.
        let urge = if alive { game.urge(w) } else { 0 };
        line(ui, URGES[urge as usize].into(), urge >= 2, false);
        let mess = if alive { game.discomfort(w) } else { 0 };
        line(ui, DISCOMFORTS[mess as usize].into(), mess >= 2, false);
        let ill = if alive { game.poisoning(w) } else { 0.0 };
        line(
            ui,
            if ill > 0.0 {
                format!("Food poisoning · {} hours to go", ill.ceil())
            } else {
                String::new()
            },
            ill > 0.0,
            false,
        );
        let alone = if alive { game.loneliness(w) } else { 0 };
        let days = game.days_alone(w).floor();
        line(
            ui,
            if alone > 0 {
                format!("{} · {days} days", LONELINESS[alone as usize])
            } else {
                String::new()
            },
            alone >= 2,
            false,
        );

        // The character sheet: two pages under the bars.
        ui.add_space(6.0);
        let diary = self.diary_open.get(w).copied().unwrap_or(false);
        ui.horizontal(|ui| {
            if theme::toggle(ui, !diary, "About").clicked() {
                self.diary_open[w] = false;
            }
            if theme::toggle(ui, diary, "Memory").clicked() {
                self.diary_open[w] = true;
            }
        });
        if !diary {
            egui::Grid::new(("about", who))
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Name").color(theme::MUTED));
                    ui.label(name(who));
                    ui.end_row();
                    ui.label(egui::RichText::new("Age").color(theme::MUTED));
                    ui.label(format!("{}", game.age(w)));
                    ui.end_row();
                    ui.label(egui::RichText::new("Born").color(theme::MUTED));
                    ui.label(date_text(
                        game.born_date(w),
                        game.born_month(w),
                        game.born_year(w),
                    ));
                    ui.end_row();
                });
        } else {
            self.diary(ui, game, w);
        }
        true
    }

    /// The Bim's own account of its days: newest day first, and within a
    /// day in the order it happened. An empty page is the *good* outcome:
    /// the diary keeps only what went wrong.
    fn diary(&self, ui: &mut egui::Ui, game: &Game, who: usize) {
        let count = game.memory_len(who);
        egui::ScrollArea::vertical()
            .max_height(220.0)
            .show(ui, |ui| {
                if count == 0 {
                    ui.label(egui::RichText::new("Nothing has gone wrong.").color(theme::MUTED));
                    return;
                }
                let mut days: Vec<(u32, Vec<(f32, String)>)> = Vec::new();
                for i in 0..count {
                    let day = game.memory_day(who, i);
                    let Some(words) =
                        memory_line(game.memory_what(who, i), game.memory_detail(who, i))
                    else {
                        continue;
                    };
                    let at = game.memory_at(who, i);
                    match days.iter_mut().find(|(d, _)| *d == day) {
                        Some((_, lines)) => lines.push((at, words)),
                        None => days.push((day, vec![(at, words)])),
                    }
                }
                days.sort_by_key(|a| std::cmp::Reverse(a.0));
                for (day, lines) in days {
                    ui.label(
                        egui::RichText::new(format!("Day {day}"))
                            .strong()
                            .color(theme::MUTED),
                    );
                    for (at, words) in lines {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                egui::RichText::new(clock_text(at))
                                    .small()
                                    .color(theme::MUTED),
                            );
                            ui.label(words);
                        });
                    }
                }
            });
    }

    // --- the agenda -----------------------------------------------------------

    /// One list per crew member with something on: the chain running, then
    /// the ones waiting behind it. `true` when anything was drawn.
    pub fn agendas(
        &mut self,
        ui: &mut egui::Ui,
        game: &Game,
        name: &dyn Fn(u32) -> String,
    ) -> bool {
        let mut drawn = false;
        for who in 0..self.crew_count {
            let w = who as usize;
            let count = game.agenda_len(w);
            if count == 0 {
                continue;
            }
            drawn = true;
            self.who_header(ui, who, name);
            for i in 0..count {
                let active = game.agenda_active(w, i) != 0;
                let done = game.agenda_progress(w, i);
                ui.horizontal(|ui| {
                    theme::bar(
                        ui,
                        70.0,
                        done,
                        if active { theme::ACCENT } else { theme::MUTED },
                    );
                    ui.label(
                        egui::RichText::new(job_name(game.agenda_job(w, i))).color(if active {
                            theme::INK
                        } else {
                            theme::MUTED
                        }),
                    );
                    ui.label(
                        egui::RichText::new(format!("{}%", (done * 100.0).round()))
                            .small()
                            .color(theme::MUTED),
                    );
                });
            }
        }
        drawn
    }

    // --- the tray ---------------------------------------------------------------

    /// Bottom-left, three tabs — seven on the ship, where there is a view
    /// to pick, actions to take outside, parts to build and a helm — and it
    /// folds away. Docked, a Station button sits with the tabs: not a tab
    /// but a press, which the screen answers with the trade window. True
    /// when the Ship tab is open, whose body the caller draws under the
    /// tabs itself.
    pub fn tray(
        &mut self,
        ui: &mut egui::Ui,
        game: &mut Game,
        mut actions: Option<&mut Actions>,
        name: &dyn Fn(u32) -> String,
    ) -> bool {
        let mut tabs = vec![
            (Tab::Schedule, "Schedule"),
            (Tab::Work, "Work"),
            (Tab::Management, "Management"),
            (Tab::Inventory, "Inventory"),
        ];
        if actions.is_some() {
            tabs.push((Tab::View, "View"));
            tabs.push((Tab::Actions, "Actions"));
            tabs.push((Tab::Build, "Build"));
            tabs.push((Tab::Ship, "Ship"));
        } else if matches!(self.tab, Tab::Actions | Tab::View | Tab::Build | Tab::Ship) {
            self.tab = Tab::Schedule;
        }
        let docked = actions.as_ref().is_some_and(|a| a.docked);
        let mut station = false;
        ui.horizontal(|ui| {
            for (tab, label) in tabs {
                if theme::toggle(ui, self.tab == tab && self.tray_open, label).clicked() {
                    self.tab = tab;
                    self.tray_open = true;
                }
            }
            if docked
                && ui
                    .button("Station")
                    .on_hover_text("Trade with the station")
                    .clicked()
            {
                station = true;
            }
            let fold = if self.tray_open { "Hide" } else { "Show" };
            if ui
                .button(fold)
                .on_hover_text(if self.tray_open {
                    "Hide the panel"
                } else {
                    "Show the panel"
                })
                .clicked()
            {
                self.tray_open = !self.tray_open;
            }
        });
        if let Some(actions) = actions.as_deref_mut() {
            actions.station = station;
        }
        if !self.tray_open {
            return false;
        }
        ui.separator();
        match self.tab {
            Tab::Ship => return true,
            Tab::Schedule => self.schedule(ui, game),
            Tab::Work => self.work(ui, game),
            Tab::Management => self.management(ui, game, actions),
            Tab::Inventory => {
                let who = self.inventory_who(game);
                self.inventory(ui, game, who, name);
            }
            Tab::Actions => {
                if let Some(actions) = actions {
                    self.actions(ui, actions);
                }
            }
            Tab::View => {
                if let Some(actions) = actions {
                    self.view(ui, actions);
                }
            }
            Tab::Build => {
                if let Some(actions) = actions {
                    self.build(ui, actions);
                }
            }
        }
        false
    }

    // --- the inventory --------------------------------------------------------

    /// Whose inventory the tab and the pop-up show: the selected crew
    /// member, or the one the player steers when nobody is picked.
    fn inventory_who(&self, game: &Game) -> usize {
        (0..self.crew_count)
            .map(|w| w as usize)
            .find(|&w| game.is_selected(w))
            .unwrap_or(self.player)
    }

    /// What a Bim has on it, as slots: head, body and leg protection down
    /// the left, top to bottom, and the weapon on the right with its
    /// numbers beside it. Slots, though nothing can be moved between them
    /// yet — the shape is the one changing equipment will want.
    fn inventory(
        &mut self,
        ui: &mut egui::Ui,
        game: &Game,
        who: usize,
        name: &dyn Fn(u32) -> String,
    ) {
        let gear = game.gear(who);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(name(who as u32)).strong());
            ui.label(
                egui::RichText::new(if game.is_armed(who) {
                    "combat mode — weapon drawn"
                } else if game.is_recruited() && who == self.player {
                    "combat mode"
                } else {
                    "weapon holstered"
                })
                .small()
                .color(if game.is_armed(who) {
                    theme::ACCENT
                } else {
                    theme::MUTED
                }),
            );
            theme::question_mark(ui, INVENTORY_TIP);
        });
        ui.add_space(4.0);
        ui.horizontal_top(|ui| {
            // The three armour slots, one above the other.
            ui.vertical(|ui| {
                for (i, worn) in [gear.head, gear.body, gear.legs].into_iter().enumerate() {
                    slot(ui, SLOT_NAMES[i], armour_name(worn), worn.is_some());
                }
            });
            ui.add_space(12.0);
            // The weapon slot, and its numbers beside it.
            ui.vertical(|ui| {
                slot(
                    ui,
                    SLOT_NAMES[3],
                    weapon_name(gear.weapon),
                    gear.weapon.is_some(),
                );
            });
            ui.add_space(8.0);
            if let Some(stats) = game.weapon_stats(who) {
                egui::Grid::new(("weapon-stats", who))
                    .num_columns(2)
                    .spacing([10.0, 2.0])
                    .show(ui, |ui| {
                        let row = |ui: &mut egui::Ui, label: &str, value: String| {
                            ui.label(egui::RichText::new(label).small().color(theme::MUTED));
                            ui.label(value);
                            ui.end_row();
                        };
                        row(ui, "Range", format!("{} tiles", stats.range));
                        theme::asks(ui, "Accuracy", ACCURACY_TIP);
                        ui.label(format!(
                            "{}% at {} tiles",
                            (stats.accuracy * 100.0).round(),
                            bims::combat::ACCURACY_RANGE
                        ));
                        ui.end_row();
                        row(ui, "Shot speed", format!("{} tiles/s", stats.speed));
                        row(ui, "Damage", format!("{} a shot", stats.damage));
                        row(ui, "Fire rate", format!("{} a second", stats.fire_rate));
                        theme::asks(ui, "DPS", DPS_TIP);
                        ui.label(egui::RichText::new(format!("{}", stats.dps())).strong());
                        ui.end_row();
                    });
            } else {
                ui.label(egui::RichText::new("Nothing in hand.").color(theme::MUTED));
            }
        });
    }

    /// The pop-up that opens the moment the crew member the player steers
    /// is recruited: its inventory, in a window of its own, until it is
    /// shut or the Bim is let go. Call once a frame after the tray.
    pub fn inventory_window(
        &mut self,
        ctx: &egui::Context,
        game: &Game,
        name: &dyn Fn(u32) -> String,
    ) {
        let recruited = game.is_recruited() && game.is_alive(self.player);
        if recruited && !self.was_recruited {
            self.inventory_open = true;
        }
        if !recruited {
            self.inventory_open = false;
        }
        self.was_recruited = recruited;
        if !self.inventory_open {
            return;
        }
        let mut open = true;
        egui::Window::new("Inventory")
            .id(egui::Id::new("inventory-window"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 60.0))
            .frame(crate::screens::room::panel_frame())
            .show(ctx, |ui| {
                self.inventory(ui, game, self.player, name);
            });
        self.inventory_open = open;
    }

    /// The parts, by category — a list of headings, one open at a time
    /// with a way back — or every part that matches what is typed in the
    /// search box; a row each with its picture's colour, its size and what
    /// it is made of, dimmed where the hold has not got it. Picking one
    /// puts the blueprint in the pointer's hand. Under the parts, the sites
    /// laid out so far, each with what has reached it and a way to call it
    /// off.
    fn build(&mut self, ui: &mut egui::Ui, actions: &mut Actions) {
        ui.set_max_width(380.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Build").small().color(theme::MUTED));
            theme::question_mark(ui, BUILD_TIP);
            ui.add_space(8.0);
            ui.label(egui::RichText::new("Search").small().color(theme::MUTED));
            let box_ = ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .desired_width(120.0)
                    .hint_text("table, wall, …"),
            );
            if !self.search.is_empty() && ui.small_button("×").on_hover_text("Clear").clicked() {
                self.search.clear();
                box_.request_focus();
            }
        });
        if !actions.at_rest {
            ui.label(
                egui::RichText::new("Nothing is built while the ship is moving. Sites can be laid out once it is at rest.")
                    .small()
                    .color(theme::WARN),
            );
        }

        // Which rows to show: a category's, or the search's matches from
        // every category.
        let needle = self.search.trim().to_lowercase();
        let rows: Vec<u32> = if !needle.is_empty() {
            BUILD_GROUPS
                .iter()
                .flat_map(|(_, _, kinds)| kinds.iter().copied())
                .filter(|&code| {
                    PartKind::from_code(code)
                        .is_some_and(|k| part_name(k).to_lowercase().contains(&needle))
                })
                .collect()
        } else if let Some(open) = self.open_group {
            BUILD_GROUPS
                .get(open)
                .map(|(_, _, kinds)| kinds.to_vec())
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        egui::ScrollArea::vertical()
            .max_height(260.0)
            .show(ui, |ui| {
                if needle.is_empty() {
                    match self.open_group {
                        None => {
                            // The categories, a button each, with a line
                            // under it saying what is inside.
                            for (i, (name, hint, _)) in BUILD_GROUPS.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    if ui
                                        .add(
                                            egui::Button::new(*name)
                                                .min_size(egui::vec2(120.0, 0.0)),
                                        )
                                        .clicked()
                                    {
                                        self.open_group = Some(i);
                                    }
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(*hint).small().color(theme::MUTED),
                                        )
                                        .wrap(),
                                    );
                                });
                            }
                            return;
                        }
                        Some(open) => {
                            ui.horizontal(|ui| {
                                if ui.button("< Back").clicked() {
                                    self.open_group = None;
                                }
                                if let Some((name, _, _)) = BUILD_GROUPS.get(open) {
                                    ui.label(egui::RichText::new(*name).strong());
                                }
                            });
                        }
                    }
                } else if rows.is_empty() {
                    ui.label(egui::RichText::new("Nothing by that name.").color(theme::MUTED));
                }
                for code in rows {
                    let Some(kind) = PartKind::from_code(code) else {
                        continue;
                    };
                    self.part_row(ui, kind, actions);
                }
            });

        // The sites laid out, and a way to call each off.
        if !actions.sites.is_empty() {
            ui.separator();
            ui.label(egui::RichText::new("Laid out").small().color(theme::MUTED));
            let mut cancel = None;
            for site in &actions.sites {
                ui.horizontal(|ui| {
                    ui.label(part_name(site.kind));
                    ui.label(
                        egui::RichText::new(format!("{}, {}", site.at.0, site.at.1))
                            .small()
                            .color(theme::MUTED),
                    );
                    ui.label(
                        egui::RichText::new(&site.progress)
                            .small()
                            .color(if site.stocked {
                                theme::ACCENT
                            } else {
                                theme::MUTED
                            }),
                    );
                    if ui.small_button("Cancel").clicked() {
                        cancel = Some(site.id);
                    }
                });
            }
            if let Some(id) = cancel {
                actions.cancel.push(id);
            }
        }
        let hint = match self.tool {
            Some(Tool::Build(kind)) => format!(
                "{} in hand: click the deck — or the space beside it — to lay it out; R turns it; right-click or Esc puts it down.",
                part_name(kind)
            ),
            _ => "Pick a part and click where it is to go. The crew carry what it is made of from the shelves and build it; a site beyond the hull is built in a suit.".to_string(),
        };
        ui.add(egui::Label::new(egui::RichText::new(hint).small().color(theme::MUTED)).wrap());
    }

    /// One part on the Build tab: its colour, a button that puts it in
    /// hand, its size, and what it is made of — each material dimmed to a
    /// warning where the hold has fewer free than the part wants.
    fn part_row(&mut self, ui: &mut egui::Ui, kind: PartKind, actions: &Actions) {
        let on = self.tool == Some(Tool::Build(kind));
        let (w, h) = kind.def().footprint;
        ui.horizontal(|ui| {
            theme::swatch(ui, theme::ship_color32(ship::Session::part_color(kind)));
            let button = egui::Button::new(part_name(kind)).min_size(egui::vec2(130.0, 0.0));
            let button = if on {
                button.fill(theme::RAISED_ON)
            } else {
                button
            };
            if ui.add(button).clicked() {
                self.tool = if on { None } else { Some(Tool::Build(kind)) };
            }
            if w != 1 || h != 1 {
                ui.label(
                    egui::RichText::new(format!("{w}×{h}"))
                        .small()
                        .color(theme::MUTED),
                );
            }
            let mut text = egui::text::LayoutJob::default();
            for (i, &(id, units)) in kind.def().recipe.iter().enumerate() {
                let short = actions.free[id as usize] < units;
                text.append(
                    &format!(
                        "{}{units} {}",
                        if i > 0 { ", " } else { "" },
                        resource_name(id).to_lowercase()
                    ),
                    0.0,
                    egui::TextFormat {
                        font_id: egui::FontId::proportional(11.0),
                        color: if short { theme::WARN } else { theme::MUTED },
                        ..Default::default()
                    },
                );
            }
            ui.label(text);
        });
    }

    /// The views: one row a way of looking at the ship, each a toggle, and
    /// one of them on. Plain is the ship as it is; Electricity draws the
    /// conduit under the deck and rings everything on it.
    fn view(&mut self, ui: &mut egui::Ui, actions: &mut Actions) {
        ui.set_max_width(360.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("View").small().color(theme::MUTED));
            theme::question_mark(
                ui,
                "What the ship view shows over the ship. A view changes the picture and nothing else.",
            );
        });
        for (overlay, label, hint) in [
            (Overlay::Plain, "Plain", "The ship as it is."),
            (
                Overlay::Electricity,
                "Electricity",
                "The power cables, and everything that makes, holds or draws power.",
            ),
        ] {
            ui.horizontal(|ui| {
                if theme::toggle(ui, actions.overlay == overlay, label).clicked() {
                    actions.overlay = overlay;
                }
                ui.label(egui::RichText::new(hint).small().color(theme::MUTED));
            });
        }
        if actions.overlay == Overlay::Electricity {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(
                        "Green is wired to a reactor; red is not. A cable joins the cable beside it, and powers whatever stands over it.",
                    )
                    .small()
                    .color(theme::MUTED),
                )
                .wrap(),
            );
        }
        // And the camera: which way is up, and whether it follows the crew
        // member you steer. This player's own, and no command.
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Camera").small().color(theme::MUTED));
        ui.horizontal(|ui| {
            if theme::toggle(ui, !actions.head_up, "North up").clicked() {
                actions.head_up = false;
            }
            if theme::toggle(ui, actions.head_up, "Head up").clicked() {
                actions.head_up = true;
            }
            ui.add_space(8.0);
            if theme::toggle(ui, actions.follow, "Follow").clicked() {
                actions.follow = true;
            }
            if theme::toggle(ui, !actions.follow, "Free camera").clicked() {
                actions.follow = false;
            }
        });
    }

    /// The tools: one row an action, each a toggle that puts the tool in
    /// the pointer's hand. Mine is the one there is. Selected, a click on
    /// the canvas is a mark rather than a selection, and the tab says how
    /// many rocks are marked and lets the lot be cleared.
    fn actions(&mut self, ui: &mut egui::Ui, actions: &mut Actions) {
        ui.set_max_width(360.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Action").small().color(theme::MUTED));
            theme::question_mark(
                ui,
                "Pick an action and use it on the canvas. Escape or a right-click puts the pointer down again.",
            );
        });
        ui.horizontal(|ui| {
            let on = self.tool == Some(Tool::Mine);
            let button = ui.add_enabled(actions.at_site, theme::toggle_button(on, "Mine"));
            if button.clicked() {
                self.tool = if on { None } else { Some(Tool::Mine) };
            }
            ui.label(
                egui::RichText::new("Mark rocks outside to be mined.")
                    .small()
                    .color(theme::MUTED),
            );
        });
        let on = self.tool == Some(Tool::Mine);
        let hint = if !actions.at_site {
            "Hold station at an asteroid belt to mine its rocks."
        } else if on {
            "Click a rock outside to mark it to be mined; click it again to unmark it. A Bim with mining on its work list takes a suit out and digs the marked rocks, nearest first."
        } else {
            "An asteroid is rock on the outside; the ore is three tiles in. Silver is iron ore, purple is galvum."
        };
        ui.add(egui::Label::new(egui::RichText::new(hint).small().color(theme::MUTED)).wrap());
        if actions.at_site {
            ui.horizontal(|ui| {
                let word = if actions.marked == 1 { "rock" } else { "rocks" };
                ui.label(format!("{} {word} marked", actions.marked));
                let out_of_reach = actions.marked.saturating_sub(actions.reachable);
                if out_of_reach > 0 {
                    ui.label(
                        egui::RichText::new(format!("{out_of_reach} out of reach"))
                            .color(theme::WARN),
                    );
                    theme::question_mark(
                        ui,
                        "A rock is mined from the tile beside it, straight on, never from a corner. One with rock on every side waits until a rock in front of it is mined — mark those too.",
                    );
                }
                if ui
                    .add_enabled(actions.marked > 0, egui::Button::new("Clear marks"))
                    .clicked()
                {
                    actions.clear = true;
                }
            });
        }
    }

    /// The day, one slot an hour, painted with a brush; and under it the
    /// action thresholds.
    fn schedule(&mut self, ui: &mut egui::Ui, game: &mut Game) {
        ui.horizontal(|ui| {
            for (slot, label, color) in [(1, "Sleep", theme::SLEEP), (0, "Everything", theme::ANY)] {
                theme::swatch(ui, color);
                if theme::toggle(ui, self.brush == slot, label).clicked() {
                    self.brush = slot;
                }
            }
            let above = (game.schedule_ignore_above() * 100.0).round();
            theme::question_mark(
                ui,
                &format!(
                    "Paint the hours the Bim should be asleep. It goes to bed when one comes round — unless it is already more than {above}% rested, in which case it ignores that one — and gets up as soon as it is fully rested."
                ),
            );
        });
        let now = (game.clock_minutes() / 60.0).floor() as usize % schedule::HOURS;
        let released = ui.input(|i| i.pointer.any_released());
        if released {
            self.painting = false;
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for hour in 0..schedule::HOURS {
                let asleep = game.schedule_slot(hour as u32) == 1;
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(18.0, 22.0), egui::Sense::click_and_drag());
                let fill = if asleep { theme::SLEEP } else { theme::ANY };
                ui.painter().rect_filled(rect, 3.0, fill);
                if hour == now {
                    ui.painter().rect_stroke(
                        rect,
                        3.0,
                        egui::Stroke::new(1.5, theme::ACCENT),
                        egui::StrokeKind::Inside,
                    );
                }
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    hour.to_string(),
                    egui::FontId::proportional(10.0),
                    theme::INK,
                );
                let response = response.on_hover_text(format!("{hour:02}:00"));
                if response.drag_started() || response.clicked() {
                    self.painting = true;
                    game.set_schedule_slot(hour as u32, self.brush);
                } else if self.painting && response.hovered() {
                    // Dragging across the strip paints the whole run in one
                    // gesture.
                    game.set_schedule_slot(hour as u32, self.brush);
                }
            }
        });

        // When the Bim sees to itself: a tick box for whether it watches
        // that need at all, and a slider for the level it acts on.
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Action threshold")
                    .small()
                    .color(theme::MUTED),
            );
            theme::question_mark(ui, TRIGGER_TIP);
        });
        egui::Grid::new("triggers")
            .num_columns(4)
            .spacing([8.0, 2.0])
            .show(ui, |ui| {
                for i in TRIGGER_NEEDS {
                    let mut on = game.need_trigger_on(i);
                    if ui.checkbox(&mut on, "").changed() {
                        game.set_need_trigger_on(i, on);
                    }
                    ui.label(
                        egui::RichText::new(NEED_NAMES.get(i as usize).copied().unwrap_or("Need"))
                            .color(if on { theme::INK } else { theme::MUTED }),
                    );
                    let mut at = (game.need_trigger(i) * 100.0).round() as u32;
                    if ui
                        .add(egui::Slider::new(&mut at, 0..=100).show_value(false))
                        .changed()
                    {
                        game.set_need_trigger(i, at as f32 / 100.0);
                    }
                    ui.label(
                        egui::RichText::new(format!("{at}%"))
                            .small()
                            .color(theme::MUTED),
                    );
                    ui.end_row();
                }
            });
    }

    /// The order the work gets done in. One row per job, built from the
    /// count the room reports rather than from the table of names, so a
    /// job added on that side shows up as a blank row rather than going
    /// missing.
    fn work(&mut self, ui: &mut egui::Ui, game: &mut Game) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Sort").small().color(theme::MUTED));
            for (sort, label) in [
                (Sort::Up, "Priority 1→5"),
                (Sort::Down, "Priority 5→1"),
                (Sort::Name, "Name A–Z"),
            ] {
                if theme::toggle(ui, self.sort == Some(sort), label).clicked() {
                    self.sort = Some(sort);
                }
            }
        });
        let count = game.work_count();
        let mut rows: Vec<(u32, &str)> = (0..count)
            .map(|job| (job, WORK_NAMES.get(job as usize).copied().unwrap_or("")))
            .collect();
        // Sorting is pressed rather than left on, so nothing moves under
        // the pointer on the click that changed it; the mark is cleared
        // when a box is clicked. Equal priorities keep declaration order.
        match self.sort {
            Some(Sort::Up) => rows.sort_by_key(|&(job, _)| (game.work_priority(job), job)),
            Some(Sort::Down) => {
                rows.sort_by_key(|&(job, _)| (std::cmp::Reverse(game.work_priority(job)), job))
            }
            Some(Sort::Name) => rows.sort_by_key(|&(_, name)| name.to_lowercase()),
            None => {}
        }
        let highest = game.work_highest();
        let lowest = game.work_lowest();
        egui::Grid::new("work").num_columns(2).spacing([12.0, 2.0]).show(ui, |ui| {
            ui.label(egui::RichText::new("Job").small().color(theme::MUTED));
            ui.label(egui::RichText::new("Priority").small().color(theme::MUTED));
            ui.end_row();
            let mut clicked = None;
            for (job, name) in rows {
                let row = ui.label(name);
                let level = game.work_priority(job);
                let button = ui
                    .add(egui::Button::new(level.to_string()).min_size(egui::vec2(28.0, 0.0)))
                    .on_hover_text(format!(
                        "Priority {level} — {highest} is done first, {lowest} last. Click to change."
                    ));
                if button.clicked() {
                    clicked = Some(job);
                }
                let spot = WORK_SPOTS.get(job as usize).copied().unwrap_or(SPOT_NOTHING);
                self.points(&row, spot);
                self.points(&button, spot);
                ui.end_row();
            }
            if let Some(job) = clicked {
                game.cycle_work_priority(job);
                self.sort = None;
            }
        });
    }

    /// What is aboard, and what to keep in stock: the three targets, by
    /// `manager::Stock`, each in the row of the thing it is a target for.
    fn management(&mut self, ui: &mut egui::Ui, game: &mut Game, actions: Option<&mut Actions>) {
        ui.horizontal(|ui| {
            let mut on = game.is_autonomous();
            if ui.checkbox(&mut on, "").changed() {
                game.set_autonomous(on);
            }
            theme::asks(ui, "Let the Bim decide", AUTONOMY_TIP);
        });
        egui::Grid::new("stock")
            .num_columns(4)
            .spacing([12.0, 2.0])
            .show(ui, |ui| {
                for head in ["Item", "Stock", "Location"] {
                    ui.label(egui::RichText::new(head).small().color(theme::MUTED));
                }
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Target").small().color(theme::MUTED));
                    theme::question_mark(ui, TARGET_TIP);
                });
                ui.end_row();
                let rows = [
                    ("Vegetables", game.store_veg(), Stock::Veg),
                    ("Tofu", game.store_tofu(), Stock::Tofu),
                    ("Stew", game.store_stew(), Stock::Stew),
                ];
                for (name, held, which) in rows {
                    let a = ui.label(name);
                    let b = ui.label(held.to_string());
                    let c = ui.label("Cold store");
                    let mut target = game.target(which);
                    let input = ui.add(
                        egui::DragValue::new(&mut target)
                            .range(0..=manager::MOST)
                            .speed(0.2),
                    );
                    if input.changed() {
                        game.set_target(which, target);
                    }
                    for r in [&a, &b, &c] {
                        self.points(r, SPOT_FRIDGE);
                    }
                    // The vegetable and tofu targets are the bay's orders and
                    // the stew target the hob's, so resting on the cell rings
                    // the place that answers it.
                    self.points(&input, KEEP_SPOTS[which as usize]);
                    ui.end_row();
                }
                // Then what the benches make, on the ship: the same kind of
                // standing order, kept in the hold rather than the cold
                // store, and answered by the smelter and the workbench.
                let Some(actions) = actions else {
                    return;
                };
                if actions.crafts.is_empty() {
                    return;
                }
                ui.label(
                    egui::RichText::new("Made aboard")
                        .small()
                        .color(theme::MUTED),
                );
                ui.end_row();
                for craft in &actions.crafts {
                    let tip = format!("Keep this many made. {}", craft.recipe);
                    ui.label(resource_name(craft.resource)).on_hover_text(&tip);
                    ui.label(craft.held.to_string());
                    ui.label(craft.kept_in);
                    let mut target = craft.target;
                    let input = ui
                        .add(
                            egui::DragValue::new(&mut target)
                                .range(0..=craft.most)
                                .speed(0.2),
                        )
                        .on_hover_text(&tip);
                    if input.changed() {
                        actions.keep.push((craft.resource, target));
                    }
                    ui.end_row();
                }
            });
    }

    // --- what the pointer is over -------------------------------------------

    /// The state worth naming beside a fixture, or "" for the things that
    /// have none.
    fn spot_state(game: &Game, spot: u32, x: f32, y: f32) -> String {
        match spot {
            SPOT_SHIP_DOOR => match game.door_at(x, y) {
                None => String::new(),
                Some(door) => {
                    if game.ship_door_is_locked(door) {
                        "locked".into()
                    } else if game.ship_door_is_held(door) {
                        "held open".into()
                    } else if game.ship_door_is_open(door) {
                        "open".into()
                    } else {
                        "shut".into()
                    }
                }
            },
            SPOT_FRIDGE => {
                let open = game.fridge_at(x, y).is_some_and(|i| game.fridge_is_open(i));
                if open { "open" } else { "" }.into()
            }
            SPOT_HOB => {
                let lit = game.hob_at(x, y).is_some_and(|i| game.stove_is_on(i));
                if lit { "lit" } else { "" }.into()
            }
            SPOT_BOARD => {
                let plates = game.plates();
                if plates == 1 {
                    "1 plate in the drawer".into()
                } else {
                    format!("{plates} plates in the drawer")
                }
            }
            SPOT_DISHWASHER => {
                let i = game.dishwasher_at(x, y).unwrap_or(0);
                if game.dishwasher_cycle_left(i) > 0.0 {
                    "running".into()
                } else if game.dishwasher_loaded(i) > 0 {
                    format!("{} plates in it", game.dishwasher_loaded(i))
                } else {
                    String::new()
                }
            }
            SPOT_BAY => {
                let ripe = game
                    .bay_at(x, y)
                    .map(|bay| game.hydro_ripe(bay))
                    .unwrap_or(0);
                if ripe > 0 {
                    format!("{ripe} ready to lift")
                } else {
                    String::new()
                }
            }
            SPOT_DOOR => {
                if game.door_is_locked() {
                    "locked".into()
                } else if game.door_is_open() {
                    "open".into()
                } else {
                    "shut".into()
                }
            }
            _ => String::new(),
        }
    }

    /// What the room makes of a point on the deck, in room coordinates: the
    /// spot code, the thing there and its state — "Hob · lit" — and
    /// whatever is lying on the deck at that spot, or "" when it is clean
    /// or not deck at all.
    pub fn spot_readout(game: &Game, x: f32, y: f32) -> (u32, String, String) {
        let spot = game.spot_at(x, y);
        let mut thing = SPOT_NAMES
            .get(spot as usize)
            .copied()
            .unwrap_or("Something")
            .to_string();
        let state = Self::spot_state(game, spot, x, y);
        if !state.is_empty() {
            thing = format!("{thing} · {state}");
        }
        let mut on_it = String::new();
        if DECK_SPOTS.contains(&spot) {
            let mess = MESS_NAMES
                .get(game.spot_mess(x, y) as usize)
                .copied()
                .unwrap_or("");
            if !mess.is_empty() {
                let deep = (game.spot_mess_depth(x, y) * 100.0).round();
                on_it = format!("{mess} — {deep}% fouled");
            }
        }
        (spot, thing, on_it)
    }
}

/// The room's own idea of who is steered: `bim::PLAYER`.
/// One slot of the inventory: a box with what is in it, the slot's name
/// under it. An empty slot is drawn hollow, a filled one lit. Painted on
/// a rect of a fixed size rather than laid out, so it is a slot and not
/// whatever space the window happens to have.
fn slot(ui: &mut egui::Ui, label: &str, content: &str, filled: bool) {
    ui.vertical(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(96.0, 42.0), egui::Sense::hover());
        let painter = ui.painter();
        painter.rect(
            rect,
            4.0,
            theme::PANEL_DEEP,
            egui::Stroke::new(1.0, if filled { theme::ACCENT } else { theme::LINE }),
            egui::StrokeKind::Inside,
        );
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            content,
            egui::FontId::proportional(13.0),
            if filled { theme::INK } else { theme::MUTED },
        );
        ui.label(egui::RichText::new(label).small().color(theme::MUTED));
        ui.add_space(4.0);
    });
}

pub fn player() -> usize {
    bim::PLAYER
}
