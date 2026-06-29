//! Optional audio playback (feature `audio`). Plays the genuine emergency chime
//! followed by a per-category spoken announcement when a new full-screen alert
//! appears. Decodes bundled WAVs via rodio.
//!
//! On a headless host (no audio device) `Sound::new` returns `None` and every
//! call is a no-op, so this never blocks startup.

use jalert_receiver::model::Category;
use std::io::Cursor;

const CHIME: &[u8] = include_bytes!("../../assets/audio/chime.wav");
const A_PROTECT: &[u8] = include_bytes!("../../assets/audio/announce_protect.wav");
const A_EEW: &[u8] = include_bytes!("../../assets/audio/announce_eew.wav");
const A_TSUNAMI: &[u8] = include_bytes!("../../assets/audio/announce_tsunami.wav");
const A_VOLCANO: &[u8] = include_bytes!("../../assets/audio/announce_volcano.wav");

pub struct Sound {
    // Keep the stream alive for the lifetime of the app; dropping it stops audio.
    _stream: rodio::OutputStream,
    handle: rodio::OutputStreamHandle,
}

impl Sound {
    pub fn new() -> Option<Self> {
        let (stream, handle) = rodio::OutputStream::try_default().ok()?;
        Some(Sound { _stream: stream, handle })
    }

    /// Play the chime, then the announcement for `category` (if any), in the
    /// background. Each call mixes onto a fresh detached sink.
    pub fn play_alert(&self, category: Category) {
        let sink = match rodio::Sink::try_new(&self.handle) {
            Ok(s) => s,
            Err(_) => return,
        };
        if let Ok(d) = rodio::Decoder::new(Cursor::new(CHIME)) {
            sink.append(d);
        }
        if let Some(bytes) = announce(category) {
            if let Ok(d) = rodio::Decoder::new(Cursor::new(bytes)) {
                sink.append(d);
            }
        }
        sink.detach(); // play to completion without holding the handle
    }
}

fn announce(c: Category) -> Option<&'static [u8]> {
    match c {
        Category::CivilProtection => Some(A_PROTECT),
        Category::Eew | Category::Earthquake | Category::SeismicIntensity => Some(A_EEW),
        Category::Tsunami => Some(A_TSUNAMI),
        Category::Volcano => Some(A_VOLCANO),
        _ => None,
    }
}
