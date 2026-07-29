//! Web tuner backend: `getUserMedia` → `MediaStreamAudioSourceNode` →
//! `AnalyserNode`. Each `poll()` reads the analyser's time-domain buffer and
//! runs it through the shared pitch detector. cpal has no released WASM input
//! backend, so this uses web-sys directly.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::JsCast;
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{AnalyserNode, AudioContext, MediaStream, MediaStreamConstraints};

use crate::audio::pitch::{PitchTracker, WINDOW};
use crate::note::{NoteReading, Scale, Transposition};

pub struct WebTuner {
    enabled: bool,
    a4: f32,
    scale: Scale,
    transpose: Transposition,
    reading: Option<NoteReading>,
    analyser: Rc<RefCell<Option<AnalyserNode>>>,
    ctx: Option<AudioContext>,
    tracker: Option<PitchTracker>,
    buf: Vec<f32>,
    requested: bool,
}

impl WebTuner {
    pub fn new() -> Self {
        Self {
            enabled: false,
            a4: 440.0,
            scale: Scale::default(),
            transpose: Transposition::default(),
            reading: None,
            analyser: Rc::new(RefCell::new(None)),
            ctx: None,
            tracker: None,
            buf: vec![0.0; WINDOW],
            requested: false,
        }
    }

    pub fn set_enabled(&mut self, on: bool) {
        // Called every frame with the same value; only act on a real transition.
        if on == self.enabled {
            return;
        }
        self.enabled = on;
        if !on {
            self.reading = None;
        }
        // Suspend the audio graph while the tuner is off so the analyser stops
        // processing (saving CPU/battery), and resume it when it comes back on.
        // The mic MediaStream track itself stays live by design — releasing it
        // would force a fresh getUserMedia prompt on every re-enable.
        if let Some(ctx) = &self.ctx {
            let _ = if on { ctx.resume() } else { ctx.suspend() };
        }
    }

    pub fn set_a4(&mut self, a4: f32) {
        self.a4 = a4;
    }

    pub fn set_scale(&mut self, scale: Scale) {
        self.scale = scale;
    }

    pub fn set_transpose(&mut self, transpose: Transposition) {
        self.transpose = transpose;
    }

    pub fn reading(&self) -> Option<NoteReading> {
        self.reading
    }

    pub fn on_user_gesture(&mut self) {
        if self.enabled {
            self.request_mic();
        }
        if let Some(ctx) = &self.ctx {
            let _ = ctx.resume();
        }
    }

    pub fn poll(&mut self) {
        if !self.enabled {
            self.reading = None;
            return;
        }

        // Build the tracker once the AudioContext's sample rate is known.
        if self.tracker.is_none()
            && let Some(ctx) = &self.ctx
        {
            self.tracker = Some(PitchTracker::new(ctx.sample_rate() as u32));
        }

        let got = match self.analyser.try_borrow() {
            Ok(slot) => {
                if let Some(analyser) = slot.as_ref() {
                    analyser.get_float_time_domain_data(&mut self.buf);
                    true
                } else {
                    false
                }
            }
            Err(_) => false,
        };

        if got && let Some(tracker) = self.tracker.as_mut() {
            self.reading = tracker
                .detect(&self.buf)
                .and_then(|f| NoteReading::from_freq(f, self.a4, self.scale, self.transpose));
        }
    }

    fn request_mic(&mut self) {
        if self.requested {
            return;
        }
        self.requested = true;

        let ctx = match AudioContext::new() {
            Ok(c) => c,
            Err(e) => {
                log::error!("tuner: AudioContext failed: {e:?}");
                self.requested = false;
                return;
            }
        };
        self.ctx = Some(ctx.clone());

        let Some(window) = web_sys::window() else {
            return;
        };
        let media_devices = match window.navigator().media_devices() {
            Ok(m) => m,
            Err(e) => {
                log::error!("tuner: media_devices unavailable: {e:?}");
                return;
            }
        };

        let constraints = MediaStreamConstraints::new();
        constraints.set_audio_bool(true);
        let promise = match media_devices.get_user_media_with_constraints(&constraints) {
            Ok(p) => p,
            Err(e) => {
                log::error!("tuner: getUserMedia call failed: {e:?}");
                return;
            }
        };

        let analyser_slot = self.analyser.clone();
        spawn_local(async move {
            match JsFuture::from(promise).await {
                Ok(stream_val) => {
                    let stream: MediaStream = stream_val.unchecked_into();
                    let source = match ctx.create_media_stream_source(&stream) {
                        Ok(s) => s,
                        Err(e) => return log::error!("tuner: source node failed: {e:?}"),
                    };
                    let analyser = match ctx.create_analyser() {
                        Ok(a) => a,
                        Err(e) => return log::error!("tuner: analyser failed: {e:?}"),
                    };
                    analyser.set_fft_size(WINDOW as u32);
                    if let Err(e) = source.connect_with_audio_node(&analyser) {
                        return log::error!("tuner: connect failed: {e:?}");
                    }
                    let _ = ctx.resume();
                    if let Ok(mut slot) = analyser_slot.try_borrow_mut() {
                        *slot = Some(analyser);
                    }
                    log::info!("tuner: microphone connected");
                }
                Err(e) => {
                    log::warn!("tuner: microphone permission denied / unavailable: {e:?}");
                }
            }
        });
    }
}
