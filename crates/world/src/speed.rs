//! How fast the world runs, and who decides.
//!
//! **Everybody, and the slowest wins.** There is one world and one clock in
//! it, so a speed is not a per-player view setting the way a camera is — it is
//! a change to what happens. Taking the slowest request means nobody is ever
//! carried past something they wanted to look at, and a pause by anybody is a
//! pause, which is the one control that has to work the instant it is asked
//! for.
//!
//! The alternative — a majority, or whoever asked last — was considered and is
//! worse in the one case that matters: the player who needs it slow is the
//! player something is going wrong for.

/// The speeds the world will run at.
///
/// The discriminants cross the wasm boundary and index the speed buttons in
/// `crates/app/src/names.rs`, so they are written out and not renumbered. The multiplier
/// is the interesting half and it is deliberately not the discriminant: a
/// pause is a speed of nothing, not a missing speed.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u32)]
pub enum Speed {
    Paused = 0,
    Real = 1,
    Triple = 2,
    Ten = 3,
    Top = 4,
}

impl Speed {
    /// Every speed, in the order the buttons are drawn.
    pub const ALL: [Speed; 5] = [
        Speed::Paused,
        Speed::Real,
        Speed::Triple,
        Speed::Ten,
        Speed::Top,
    ];

    /// How many game minutes a minute of real time is worth.
    pub fn multiplier(self) -> u32 {
        match self {
            Speed::Paused => 0,
            Speed::Real => 1,
            Speed::Triple => 3,
            Speed::Ten => 10,
            Speed::Top => crate::data::TOP_SPEED,
        }
    }

    pub fn code(self) -> u32 {
        self as u32
    }

    pub fn from_code(code: u32) -> Option<Speed> {
        Speed::ALL.get(code as usize).copied()
    }
}

/// What the world actually runs at: the slowest anybody has asked for.
///
/// An empty list is a paused world rather than a world at full speed — a
/// game with nobody in it should not be running.
pub fn effective(requests: &[Speed]) -> Speed {
    requests.iter().copied().min().unwrap_or(Speed::Paused)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_slowest_request_is_what_happens() {
        assert_eq!(effective(&[Speed::Top, Speed::Triple]), Speed::Triple);
        assert_eq!(effective(&[Speed::Top, Speed::Top]), Speed::Top);
        assert_eq!(
            effective(&[Speed::Real, Speed::Top, Speed::Ten]),
            Speed::Real
        );
    }

    #[test]
    fn a_pause_by_anybody_is_a_pause() {
        assert_eq!(effective(&[Speed::Top, Speed::Paused]), Speed::Paused);
        assert_eq!(effective(&[Speed::Paused]).multiplier(), 0);
        assert_eq!(effective(&[]), Speed::Paused);
    }

    /// The enum's order **is** the speed order, which is what makes `min` the
    /// right answer. If a speed is ever inserted in the middle this is what
    /// notices.
    #[test]
    fn the_codes_climb_with_the_multipliers() {
        for pair in Speed::ALL.windows(2) {
            assert!(pair[0] < pair[1]);
            assert!(pair[0].multiplier() < pair[1].multiplier());
        }
        assert_eq!(Speed::Top.multiplier(), crate::data::TOP_SPEED);
        assert_eq!(Speed::from_code(5), None);
    }
}
