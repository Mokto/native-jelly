//! Where the television sends its sound, as last read by the port's sound-output reader.
//!
//! DTS passthrough (`devcaps::set_dts_passthrough`) feeds a DTS core as IEC 61937 bursts in stereo
//! PCM. A receiver's DTS decoder plays those; the set's own speakers, headphones or a Bluetooth
//! sink would play them as full-scale noise. Nothing counts as a receiver until the set has said so.

use std::sync::Mutex;

/// The set's `soundOutput` value, e.g. `external_arc`, `tv_speaker`, `bt_soundbar`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SoundOutput(Option<String>);

impl SoundOutput {
    pub fn named(output: &str) -> Self {
        Self(Some(output.to_owned()))
    }

    /// HDMI ARC/eARC or optical: the links that carry an IEC 61937 bitstream to an external
    /// decoder (the same two outputs dts_restore_plus passes a DTS core on).
    pub fn is_receiver(&self) -> bool {
        matches!(self.0.as_deref(), Some("external_arc" | "external_optical"))
    }

    /// For the log: the set's own word, or `unknown` before it has answered.
    pub fn label(&self) -> &str {
        self.0.as_deref().unwrap_or("unknown")
    }
}

static LATEST: Mutex<SoundOutput> = Mutex::new(SoundOutput(None));

/// The last answer. Performs no bus call.
pub fn latest() -> SoundOutput {
    LATEST.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// The reader's latest answer; an unknown output withdraws a value that can no longer be vouched for.
pub fn publish(output: SoundOutput) {
    *LATEST.lock().unwrap_or_else(|e| e.into_inner()) = output;
}

/// Ask for a fresh reading without waiting for it: the next [`latest`] sees it once the set has
/// answered. Off-device this does nothing.
pub fn refresh() {
    crate::tv::refresh_sound_output();
}

#[cfg(test)]
mod tests {
    use super::SoundOutput;

    #[test]
    fn only_arc_and_optical_count_as_a_receiver() {
        assert!(SoundOutput::named("external_arc").is_receiver());
        assert!(SoundOutput::named("external_optical").is_receiver());
        for other in ["tv_speaker", "headphone", "bt_soundbar", "tv_external_speaker", ""] {
            assert!(!SoundOutput::named(other).is_receiver(), "{other}");
        }
        assert!(!SoundOutput::default().is_receiver());
        assert_eq!(SoundOutput::default().label(), "unknown");
    }
}
