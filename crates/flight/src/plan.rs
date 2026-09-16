//! A trip, worked out once and then read at a time.
//!
//! # Why it is closed form
//!
//! The same trip is flown by a browser stepping sixty times a second, by a
//! player holding 24x, and one day by a native server catching up on an hour
//! of somebody's disconnection in a single call. Those three have to agree
//! about where the ship is, and the only way to make that true rather than
//! hope it is to never integrate: a [`Plan`] is a fixed list of segments with
//! fixed durations, and [`state_at`] evaluates position, velocity, heading and
//! fuel **from the elapsed minutes alone**. Nothing accumulates, nothing has a
//! tick of its own, and calling it with 0, then 3, then 3.5 gives the same
//! answers as calling it with 3.5.
//!
//! That is the same rule `crates/health` is built on, for the same reason —
//! see its note about cutting an interval at every boundary. Here there is
//! nothing to cut, because there is nothing that changes rate inside a
//! segment.
//!
//! # The shape of a trip
//!
//! Everything happens along one straight line: the ship turns to face the
//! arrival point, burns towards it, and stops on it.
//!
//! 1. **Align** — turn to the bearing. Bang-bang: full angular acceleration
//!    for half the turn, full deceleration for the other half, so the ship
//!    starts and finishes at rest. Skipped when already pointing there.
//! 2. **Burn** — the forward engines, flat out.
//! 3. **Brake** — one of two, whichever is quicker:
//!    - **flip**: coast through a half turn, then the *same* forward engines
//!      pointing the other way;
//!    - **backward engines**: no turn at all, and only if there are any.
//! 4. **Arrive** — docked if the target is a station and the ship has an
//!    airlock, holding otherwise.
//!
//! There is no coasting phase in the middle and no speed limit. A trip is
//! flat out to the changeover and braking from there, which is the same shape
//! `physics::travel_days` quotes and the same one the world generator laid its
//! systems out against.

use worldgen::math::DVec2;

use crate::angle;
use crate::data;
use crate::dynamics::Dynamics;

/// What a trip is aimed at.
///
/// A point is a place rather than a thing: it has no arrival radius, because
/// there is nothing there to keep clear of.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Target {
    Station(u32),
    Body(u32),
    Point(DVec2),
}

impl Target {
    /// How far short of the target the trip finishes.
    pub fn arrival_radius(self) -> f64 {
        match self {
            Target::Station(_) => data::ARRIVAL_RADIUS_STATION,
            Target::Body(_) => data::ARRIVAL_RADIUS_BODY,
            Target::Point(_) => 0.0,
        }
    }

    /// The world node it names, if it names one.
    pub fn node(self) -> Option<worldgen::Node> {
        match self {
            Target::Station(id) => Some(worldgen::Node::Station(id)),
            Target::Body(id) => Some(worldgen::Node::Body(id)),
            Target::Point(_) => None,
        }
    }

    /// The number that crosses the wasm boundary. No strings do.
    pub fn code(self) -> u32 {
        match self {
            Target::Station(_) => 0,
            Target::Body(_) => 1,
            Target::Point(_) => 2,
        }
    }
}

/// Which part of a trip the ship is in.
///
/// The discriminants cross the wasm boundary and index `PHASE_NAMES` in
/// `web/ship.js`, so they are written out and not renumbered.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum Phase {
    /// Turning to face the arrival point. Not moving.
    Align = 0,
    /// Under the forward engines.
    Burn = 1,
    /// Coasting, turning end over end so the engines can brake.
    Flip = 2,
    /// Slowing down.
    Brake = 3,
    /// There. The plan is over and the world has taken the ship off it.
    Arrived = 4,
}

impl Phase {
    pub fn code(self) -> u32 {
        self as u32
    }
}

/// Why a trip could not be planned.
///
/// The discriminants cross the wasm boundary and index `PLAN_ERRORS` in
/// `web/ship.js`; `0` is left free for "nothing went wrong", the shape every
/// other code in this workspace has.
///
/// Two of them are never returned by [`plan_trip`] and live here anyway, for
/// the same reason `EditError::Locked` lives beside the placement errors: a
/// player is given **one** table of reasons rather than two.
/// [`PlanError::TargetUndiscovered`] is a question about what the crew have
/// seen, which is `world`'s to answer, and [`PlanError::NoFuelAboard`] is the
/// shape of the same thing for a ship that has none at all.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum PlanError {
    /// No engine pushing the ship along its own nose. Nothing to set off with.
    NoForwardEngine = 1,
    /// No thrusters, and the trip needs a turn — either to point at the target
    /// or to brake with the engines it has.
    CannotRotate = 2,
    /// Nowhere aboard to fly it from.
    NoHelm = 3,
    /// Not enough unreserved fuel for the burns this trip would take.
    NotEnoughFuel = 4,
    /// Nobody has seen that yet. **Never returned by [`plan_trip`]** — see the
    /// note above.
    TargetUndiscovered = 5,
    /// The ship is already at the arrival point, or inside the radius round
    /// the target.
    AlreadyThere = 6,
    /// No fuel aboard at all. **Never returned by [`plan_trip`]**, which sees
    /// only how much is unreserved and answers [`PlanError::NotEnoughFuel`].
    NoFuelAboard = 7,
}

impl PlanError {
    pub fn code(self) -> u32 {
        self as u32
    }
}

/// A rotation, as something that can be evaluated at a time.
///
/// Two shapes, and the second one is only reachable through an abort: a swing
/// starts and ends at rest, and a halt starts at a rate and ends at rest. A
/// shape that *starts* at a rate and ends at another is deliberately absent —
/// nothing needs one, and the two here are the only ones whose sweep can be
/// written down without integrating.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Spin {
    /// Not turning.
    Still,
    /// Bang-bang through `delta` radians: accelerate for half the time,
    /// decelerate for the other half.
    Swing { delta: f64, duration: f64 },
    /// An angular rate brought to a stop, sweeping `delta` on the way.
    Halt { delta: f64, duration: f64 },
}

impl Spin {
    /// The bang-bang turn through `delta` at angular acceleration `alpha`.
    ///
    /// A turn shorter than [`data::ALIGN_TOLERANCE`] is [`Spin::Still`]: a
    /// rotation phase of no length is one the walker would have to
    /// special-case, and a thousandth of a radian is not a manoeuvre.
    pub fn swing(delta: f64, alpha: f64) -> Spin {
        if delta.abs() <= data::ALIGN_TOLERANCE || !(alpha > 0.0) {
            return Spin::Still;
        }
        let half = (delta.abs() / alpha).sqrt();
        Spin::Swing {
            delta,
            duration: 2.0 * half,
        }
    }

    /// An angular rate decelerated to nothing. The sweep falls out of it:
    /// half the rate for the whole time.
    pub fn halt(rate: f64, alpha: f64) -> Spin {
        if rate == 0.0 || !(alpha > 0.0) {
            return Spin::Still;
        }
        let duration = rate.abs() / alpha;
        Spin::Halt {
            delta: rate * duration / 2.0,
            duration,
        }
    }

    pub fn duration(self) -> f64 {
        match self {
            Spin::Still => 0.0,
            Spin::Swing { duration, .. } | Spin::Halt { duration, .. } => duration,
        }
    }

    /// How far round it goes in total.
    pub fn delta(self) -> f64 {
        match self {
            Spin::Still => 0.0,
            Spin::Swing { delta, .. } | Spin::Halt { delta, .. } => delta,
        }
    }

    /// The heading `t` minutes in, starting from `from`.
    pub fn angle_at(self, from: f64, t: f64) -> f64 {
        match self {
            Spin::Still => from,
            Spin::Swing { delta, duration } => {
                let half = duration / 2.0;
                let t = t.clamp(0.0, duration);
                let swept = if t <= half {
                    delta * (t / half) * (t / half) / 2.0
                } else {
                    let left = (duration - t) / half;
                    delta * (1.0 - left * left / 2.0)
                };
                from + swept
            }
            Spin::Halt { delta, duration } => {
                let t = t.clamp(0.0, duration);
                let left = 1.0 - t / duration;
                from + delta * (1.0 - left * left)
            }
        }
    }

    /// The angular rate `t` minutes in. What an abort needs, to know how much
    /// spin it has to take out.
    pub fn rate_at(self, t: f64) -> f64 {
        match self {
            Spin::Still => 0.0,
            Spin::Swing { delta, duration } => {
                let half = duration / 2.0;
                let t = t.clamp(0.0, duration);
                // `2 * delta / duration` is the peak rate: the average rate is
                // half of it and the average rate is `delta / duration`.
                let peak = 2.0 * delta / duration;
                if t <= half {
                    peak * t / half
                } else {
                    peak * (duration - t) / half
                }
            }
            Spin::Halt { delta, duration } => {
                let t = t.clamp(0.0, duration);
                2.0 * delta / duration * (1.0 - t / duration)
            }
        }
    }

    /// The angular acceleration `t` minutes in — which way the thrusters are
    /// pushing, and how hard. Signed the same way as `delta`: positive is the
    /// heading climbing. A swing pushes one way for its first half and the
    /// other for its second; a halt pushes against the rate the whole way.
    ///
    /// Read by the picture and by nothing that decides anything: the sweep
    /// and the rate above are the plan, and this is the derivative of the
    /// rate written down beside it so the two cannot drift apart.
    pub fn alpha_at(self, t: f64) -> f64 {
        match self {
            Spin::Still => 0.0,
            Spin::Swing { delta, duration } => {
                let half = duration / 2.0;
                // The peak rate over the time it takes to reach it.
                let alpha = 2.0 * delta / duration / half;
                if t < 0.0 || t > duration {
                    0.0
                } else if t < half {
                    alpha
                } else {
                    -alpha
                }
            }
            Spin::Halt { delta, duration } => {
                if t < 0.0 || t > duration {
                    0.0
                } else {
                    -2.0 * delta / duration / duration
                }
            }
        }
    }
}

/// What the ship is doing to itself at one moment of a plan: the engines lit
/// and the thrusters pushing. Read off the segments the same way
/// [`state_at`] reads the position, so a picture of the exhaust agrees with
/// the ship it is behind at any speed and after any catch-up.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Effort {
    /// Acceleration along the plan's line, signed. Nothing while coasting or
    /// turning.
    pub accel: f64,
    /// Engines burning. Nothing while turning, and never the thrusters.
    pub engines: u32,
    /// Angular acceleration, signed: positive is the heading climbing.
    /// Nothing while the ship is not turning or is coasting through a flip.
    pub alpha: f64,
}

impl Effort {
    pub const NONE: Effort = Effort {
        accel: 0.0,
        engines: 0,
        alpha: 0.0,
    };
}

/// What the ship is doing to itself `minutes` after the plan began. Nothing
/// at all once the plan is over.
pub fn effort_at(plan: &Plan, minutes: f64) -> Effort {
    if minutes < 0.0 {
        return Effort::NONE;
    }
    let mut left = minutes;
    for segment in &plan.segments {
        if left < segment.duration {
            return Effort {
                accel: segment.accel,
                engines: segment.engines,
                alpha: segment.spin.alpha_at(left),
            };
        }
        left -= segment.duration;
    }
    Effort::NONE
}

/// One stretch of a trip during which nothing changes rate.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Segment {
    pub phase: Phase,
    pub duration: f64,
    /// Acceleration along the plan's `direction`. Negative brakes; the ship
    /// never moves anywhere but along that line.
    pub accel: f64,
    /// Engines burning through it. Zero while turning, and zero always for
    /// thrusters — they are not what the fuel is for.
    pub engines: u32,
    pub spin: Spin,
}

/// Where the ship is, and what it is doing, at one moment of a plan.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct State {
    pub position: DVec2,
    /// The velocity vector. Always along the plan's line — during a flip
    /// brake the ship is pointing backwards and still moving forwards, which
    /// is why heading and velocity are two answers rather than one.
    pub velocity: DVec2,
    pub speed: f64,
    pub heading: f64,
    pub phase: Phase,
    /// Fuel burnt since the trip that this plan is part of began, including
    /// anything burnt by a plan this one replaced.
    pub fuel: f64,
}

/// A trip: a line, a list of segments, and what it will cost.
#[derive(Clone, PartialEq, Debug)]
pub struct Plan {
    /// **The dynamics it was planned with**, kept rather than looked up. A
    /// design change mid-flight does not move an arrival that has already been
    /// promised; it changes the next trip.
    pub dynamics: Dynamics,
    pub start: DVec2,
    pub start_heading: f64,
    /// Speed along `direction` when the first segment begins. Zero for a trip
    /// that sets off from rest, which is every trip that is not an abort.
    pub start_speed: f64,
    /// Unit vector from `start` towards `arrival`.
    pub direction: DVec2,
    /// The heading that points along it.
    pub bearing: f64,
    pub arrival: DVec2,
    /// From `start` to `arrival` along the line.
    pub distance: f64,
    pub target: Target,
    /// Whether arriving means docking. A station with no airlock to get out
    /// of is a station to hold beside.
    pub docks: bool,
    /// Whether this plan is a stop rather than a trip.
    pub aborting: bool,
    pub segments: Vec<Segment>,
    /// Every unit of fuel the whole trip will take, `prior_fuel` included.
    pub fuel_required: f64,
    /// What a plan this one replaced had already burnt. An abort inherits it,
    /// so the fuel taken out of the hold at the end is the fuel actually
    /// burnt and not just the last leg's.
    pub prior_fuel: f64,
}

impl Plan {
    pub fn duration(&self) -> f64 {
        self.segments.iter().map(|s| s.duration).sum()
    }

    /// The heading the ship ends up on. What the world holds it at afterwards.
    pub fn final_heading(&self) -> f64 {
        let mut heading = self.start_heading;
        for segment in &self.segments {
            heading = segment.spin.angle_at(heading, segment.duration);
        }
        angle::wrap(heading)
    }
}

/// Where a plan has got to, `minutes` after it began.
///
/// Reads the plan; changes nothing. Called at whatever moment anybody asks —
/// once a step by the world, and again by the renderer between steps.
pub fn state_at(plan: &Plan, minutes: f64) -> State {
    let total = plan.duration();
    let mut left = minutes.clamp(0.0, total);
    let mut along = 0.0;
    let mut speed = plan.start_speed;
    let mut heading = plan.start_heading;
    let mut fuel = plan.prior_fuel;
    let mut phase = Phase::Arrived;
    let mut settled = false;

    for segment in &plan.segments {
        let t = left.min(segment.duration);
        along += speed * t + 0.5 * segment.accel * t * t;
        fuel += plan.dynamics.fuel_per_minute(segment.engines) * t;
        speed += segment.accel * t;
        heading = segment.spin.angle_at(heading, t);
        if !settled && t < segment.duration {
            phase = segment.phase;
            settled = true;
        }
        left -= t;
    }

    State {
        position: plan.start.add(plan.direction.scale(along)),
        velocity: plan.direction.scale(speed),
        speed,
        heading: angle::wrap(heading),
        phase,
        fuel,
    }
}

/// How much fuel the trip has burnt `minutes` in.
///
/// Zero at departure and [`Plan::fuel_required`] at arrival, flat through
/// every turn in between — a thruster burns nothing.
pub fn fuel_burned_at(plan: &Plan, minutes: f64) -> f64 {
    state_at(plan, minutes).fuel
}

/// Plan a trip from rest.
///
/// `fuel_available` is the **unreserved** fuel aboard: a reservation held for
/// a trip already under way is not fuel this one may spend. It is a parameter
/// rather than a field on [`Dynamics`] because it is a fact about the world at
/// one moment and the dynamics are a fact about the design.
pub fn plan_trip(
    dynamics: &Dynamics,
    start: DVec2,
    start_heading: f64,
    target: Target,
    target_position: DVec2,
    fuel_available: f64,
) -> Result<Plan, PlanError> {
    if !dynamics.has_helm {
        return Err(PlanError::NoHelm);
    }
    if !(dynamics.a_forward > 0.0) {
        return Err(PlanError::NoForwardEngine);
    }

    let to_target = target_position.sub(start);
    let straight = to_target.length();
    let distance = straight - target.arrival_radius();
    if !distance.is_finite() || distance <= data::STILL {
        return Err(PlanError::AlreadyThere);
    }
    let direction = to_target.scale(1.0 / straight);
    let bearing = angle::bearing(direction);

    let align = Spin::swing(angle::shortest(start_heading, bearing), dynamics.alpha);
    if align == Spin::Still && angle::shortest(start_heading, bearing).abs() > data::ALIGN_TOLERANCE
    {
        // `swing` refuses when there is nothing to turn with, and a ship that
        // cannot point at the target cannot go there.
        return Err(PlanError::CannotRotate);
    }

    let braking = choose_brake(dynamics, distance).ok_or(PlanError::CannotRotate)?;
    if braking.fuel > fuel_available + data::STILL {
        return Err(PlanError::NotEnoughFuel);
    }

    let mut segments = Vec::with_capacity(4);
    if align != Spin::Still {
        segments.push(Segment {
            phase: Phase::Align,
            duration: align.duration(),
            accel: 0.0,
            engines: 0,
            spin: align,
        });
    }
    segments.extend(braking.segments);

    Ok(Plan {
        dynamics: *dynamics,
        start,
        start_heading,
        // Every trip sets off from rest. A redirect is not an exception: it
        // stops first, and only then is a second plan made from where it
        // stopped — which is why there is no way to hand a speed in here.
        start_speed: 0.0,
        direction,
        bearing,
        arrival: start.add(direction.scale(distance)),
        distance,
        target,
        docks: matches!(target, Target::Station(_)) && dynamics.has_airlock,
        aborting: false,
        segments,
        fuel_required: braking.fuel,
        prior_fuel: 0.0,
    })
}

/// The burn and the brake, as segments, for whichever of the two ways of
/// stopping is quicker.
struct Braking {
    segments: Vec<Segment>,
    fuel: f64,
}

/// Work both options out and take the shorter.
///
/// Neither is guaranteed: a ship with no thrusters cannot flip, and a ship
/// with no backward engine cannot brake without flipping. A ship with neither
/// could set off and never stop, which is why `None` here is a refusal to
/// plan rather than a trip with no braking phase.
fn choose_brake(dynamics: &Dynamics, distance: f64) -> Option<Braking> {
    let a_f = dynamics.a_forward;
    let flip = dynamics.can_rotate().then(|| {
        // Coasting through the half turn covers ground, so the burn is
        // shorter than it would be without one. `t1` is the positive root of
        //   a·t1² + a·t_flip·t1 − distance = 0
        // which is the distance identity written out: half the burn, all the
        // coast, and a braking leg the same length as the burn.
        let turn = Spin::swing(std::f64::consts::PI, dynamics.alpha);
        let t_flip = turn.duration();
        let t1 = (-t_flip + (t_flip * t_flip + 4.0 * distance / a_f).sqrt()) / 2.0;
        let fuel = dynamics.fuel_per_minute(dynamics.forward_engines) * 2.0 * t1;
        (
            2.0 * t1 + t_flip,
            Braking {
                segments: vec![
                    Segment {
                        phase: Phase::Burn,
                        duration: t1,
                        accel: a_f,
                        engines: dynamics.forward_engines,
                        spin: Spin::Still,
                    },
                    Segment {
                        phase: Phase::Flip,
                        duration: t_flip,
                        accel: 0.0,
                        engines: 0,
                        spin: turn,
                    },
                    Segment {
                        phase: Phase::Brake,
                        duration: t1,
                        accel: -a_f,
                        engines: dynamics.forward_engines,
                        spin: Spin::Still,
                    },
                ],
                fuel,
            },
        )
    });

    let a_b = dynamics.a_backward;
    let backward = (a_b > 0.0).then(|| {
        // No turn at all: burn until the changeover, then push the other way.
        // This is `physics::travel_days`' shape, split into its two legs.
        let t1 = (2.0 * distance * a_b / (a_f * (a_f + a_b))).sqrt();
        let t2 = t1 * a_f / a_b;
        let fuel = dynamics.fuel_per_minute(dynamics.forward_engines) * t1
            + dynamics.fuel_per_minute(dynamics.backward_engines) * t2;
        (
            t1 + t2,
            Braking {
                segments: vec![
                    Segment {
                        phase: Phase::Burn,
                        duration: t1,
                        accel: a_f,
                        engines: dynamics.forward_engines,
                        spin: Spin::Still,
                    },
                    Segment {
                        phase: Phase::Brake,
                        duration: t2,
                        accel: -a_b,
                        engines: dynamics.backward_engines,
                        spin: Spin::Still,
                    },
                ],
                fuel,
            },
        )
    });

    match (flip, backward) {
        (Some((ta, a)), Some((tb, b))) => Some(if tb < ta { b } else { a }),
        (Some((_, a)), None) => Some(a),
        (None, Some((_, b))) => Some(b),
        (None, None) => None,
    }
}

/// Give a trip up and come to rest, as fast as the ship can.
///
/// The line is kept: the ship stops somewhere along the one it was already
/// flying, never reverses, and never sets off anywhere else. Three things can
/// be in the way and all three are handled the same way — by stopping them
/// first:
///
/// - **it is mid-turn**, so the spin is taken out before anything else;
/// - **it is pointing the wrong way to brake**, so it turns to whichever
///   attitude the quicker brake wants;
/// - **it is not moving at all**, in which case there is nothing to do but
///   stop turning, which is what an abort during the align phase is.
pub fn abort(plan: &Plan, minutes: f64) -> Plan {
    let now = state_at(plan, minutes);
    let dynamics = plan.dynamics;
    let (index, local) = segment_at(plan, minutes);
    let rate = plan
        .segments
        .get(index)
        .map(|s| s.spin.rate_at(local))
        .unwrap_or(0.0);

    let mut segments = Vec::with_capacity(3);
    let halt = Spin::halt(rate, dynamics.alpha);
    if halt != Spin::Still {
        segments.push(Segment {
            phase: Phase::Flip,
            duration: halt.duration(),
            accel: 0.0,
            engines: 0,
            spin: halt,
        });
    }
    let after_halt = halt.angle_at(now.heading, halt.duration());

    let mut fuel = now.fuel;
    if now.speed > data::STILL {
        if let Some(stop) = choose_stop(&dynamics, now.speed, plan.bearing, after_halt) {
            fuel += stop.fuel;
            segments.extend(stop.segments);
        }
    }

    // However many segments there are, the ship only ever moves along the
    // line it was already on — so where it ends up is the distance those
    // segments cover, from where it is now.
    let travelled: f64 = {
        let mut along = 0.0;
        let mut speed = now.speed;
        for segment in &segments {
            along += speed * segment.duration + 0.5 * segment.accel * segment.duration.powi(2);
            speed += segment.accel * segment.duration;
        }
        along
    };
    let arrival = now.position.add(plan.direction.scale(travelled));

    Plan {
        dynamics,
        start: now.position,
        start_heading: now.heading,
        start_speed: now.speed,
        direction: plan.direction,
        bearing: plan.bearing,
        arrival,
        distance: travelled,
        target: Target::Point(arrival),
        docks: false,
        aborting: true,
        segments,
        fuel_required: fuel,
        prior_fuel: now.fuel,
    }
}

/// The turn and the brake that bring `speed` to nothing quickest.
fn choose_stop(dynamics: &Dynamics, speed: f64, bearing: f64, heading: f64) -> Option<Braking> {
    let mut options: Vec<(f64, Braking)> = Vec::with_capacity(2);

    // Flip and brake on the forward engines: the attitude wanted is tail
    // first, which is the bearing turned right round.
    let turn = Spin::swing(
        angle::shortest(heading, angle::wrap(bearing + std::f64::consts::PI)),
        dynamics.alpha,
    );
    let aligned_for_flip = turn != Spin::Still
        || angle::shortest(heading, bearing + std::f64::consts::PI).abs() <= data::ALIGN_TOLERANCE;
    if dynamics.a_forward > 0.0 && aligned_for_flip {
        let t = speed / dynamics.a_forward;
        options.push((
            turn.duration() + t,
            brake_segments(
                dynamics,
                turn,
                t,
                -dynamics.a_forward,
                dynamics.forward_engines,
            ),
        ));
    }

    // Or brake on the backward engines, which wants the ship still pointing
    // where it is going.
    let turn = Spin::swing(angle::shortest(heading, bearing), dynamics.alpha);
    let aligned_for_back =
        turn != Spin::Still || angle::shortest(heading, bearing).abs() <= data::ALIGN_TOLERANCE;
    if dynamics.a_backward > 0.0 && aligned_for_back {
        let t = speed / dynamics.a_backward;
        options.push((
            turn.duration() + t,
            brake_segments(
                dynamics,
                turn,
                t,
                -dynamics.a_backward,
                dynamics.backward_engines,
            ),
        ));
    }

    options
        .into_iter()
        .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(_, braking)| braking)
}

fn brake_segments(
    dynamics: &Dynamics,
    turn: Spin,
    duration: f64,
    accel: f64,
    engines: u32,
) -> Braking {
    let mut segments = Vec::with_capacity(2);
    if turn != Spin::Still {
        segments.push(Segment {
            phase: Phase::Flip,
            duration: turn.duration(),
            accel: 0.0,
            engines: 0,
            spin: turn,
        });
    }
    segments.push(Segment {
        phase: Phase::Brake,
        duration,
        accel,
        engines,
        spin: Spin::Still,
    });
    Braking {
        fuel: dynamics.fuel_per_minute(engines) * duration,
        segments,
    }
}

/// Which segment a moment falls in, and how far into it.
fn segment_at(plan: &Plan, minutes: f64) -> (usize, f64) {
    let mut left = minutes.max(0.0);
    for (i, segment) in plan.segments.iter().enumerate() {
        if left < segment.duration {
            return (i, left);
        }
        left -= segment.duration;
    }
    (plan.segments.len(), 0.0)
}
