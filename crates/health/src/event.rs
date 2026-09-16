//! What just happened to a body, for whoever is watching.
//!
//! [`crate::update`] hands back a list of these and they are the **only**
//! thing that says a line was crossed. A caller that wants to say "James is
//! ill" watches for the event; a caller that wants to draw a bar reads the
//! state. Polling the state for a change instead would miss a crossing that
//! happened and reversed inside one update, which is exactly what a long
//! step does.
//!
//! One event per transition, in the order the transitions happened, however
//! long the step. An update that takes a Bim from nothing to radiation
//! sickness emits all three in order — the same three, in the same order,
//! that a minute-by-minute run would emit.
//!
//! No strings. The host names them, the way it names every other code that
//! crosses the boundary — see `MEMORY_LINES` in `web/bims.js` for the shape
//! this will take when it gets there.

/// The discriminants are written out because they will cross the wasm
/// boundary as numbers, and a reordered enum must not silently renumber
/// anything. `0` is left free for "nothing happened", which is the shape
/// every other code in this workspace has.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum HealthEvent {
    /// A dose where there was none. The meter appears here and stays until
    /// [`HealthEvent::DoseCleared`].
    RadiationDetected = 1,
    /// The dose has reached the level that does damage.
    CriticalDose = 2,
    /// Radiation sickness has set in.
    RadiationSickness = 3,
    /// The dose has fallen back out of sickness. Still critical.
    SicknessSubsided = 4,
    /// The dose has fallen back below critical. Nothing is being damaged any
    /// more, and the body mends again unless it has cancer.
    BelowCritical = 5,
    /// The dose is back at nothing. The meter goes away.
    DoseCleared = 6,
    /// Cancer has begun. There is no event for it ending, because it does
    /// not end.
    CancerOnset = 7,
    CancerAdvanced = 8,
    CancerTerminal = 9,
    /// The last event this body will ever emit.
    Died = 10,
}

impl HealthEvent {
    pub fn code(self) -> u32 {
        self as u32
    }
}
