//! The audio engine's run status, published by the realtime thread for the control thread to
//! observe. Neutral (no platform deps) so the fault→UI reaction is testable on Linux. The realtime
//! thread stores it into an `AtomicU8` at loop exit — a non-blocking publish, never a lock or a
//! channel the audio thread waits on (ADR-0001).

/// Why the engine's realtime loop is (or is not) running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EngineStatus {
    /// The realtime loop is running normally.
    #[default]
    Running,
    /// The loop exited because the control thread asked it to stop.
    StoppedByUser,
    /// The loop exited on its own because of a runtime fault (e.g. the device was invalidated).
    Faulted,
}

impl EngineStatus {
    /// Encode for the `AtomicU8` the realtime thread publishes through.
    pub fn as_u8(self) -> u8 {
        match self {
            EngineStatus::Running => 0,
            EngineStatus::StoppedByUser => 1,
            EngineStatus::Faulted => 2,
        }
    }

    /// Decode a published byte; any unknown value maps to `Running` (the safe default).
    pub fn from_u8(value: u8) -> Self {
        match value {
            1 => EngineStatus::StoppedByUser,
            2 => EngineStatus::Faulted,
            _ => EngineStatus::Running,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_u8() {
        for status in [
            EngineStatus::Running,
            EngineStatus::StoppedByUser,
            EngineStatus::Faulted,
        ] {
            assert_eq!(EngineStatus::from_u8(status.as_u8()), status);
        }
        assert_eq!(EngineStatus::default(), EngineStatus::Running);
        // An unknown byte decodes to the safe default.
        assert_eq!(EngineStatus::from_u8(99), EngineStatus::Running);
    }
}
