//! Web tuner backend: `getUserMedia` → `MediaStreamAudioSourceNode` →
//! `AnalyserNode`. Each `poll()` reads the analyser's time-domain buffer through
//! the shared detector. Uses web-sys directly — cpal has no released WASM input.

use std::cell::{Cell, RefCell};
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
    /// Live mic stream, kept so its tracks can be stopped when the tuner turns off
    /// (frees the device, clears the browser "mic in use" indicator). `None` when
    /// no mic is captured.
    stream: Rc<RefCell<Option<MediaStream>>>,
    /// Bumped on every acquire *and* release. The async `getUserMedia` closure
    /// captures the generation it started in and keeps the mic only if it still
    /// matches — so a stream arriving after an off/re-toggle is stopped, not leaked.
    generation: Rc<Cell<u64>>,
    ctx: Option<AudioContext>,
    tracker: Option<PitchTracker>,
    buf: Vec<f32>,
    /// Mic acquisition in flight or established, so repeated gestures don't fire
    /// duplicate `getUserMedia` calls. Reset on release so the next enable re-acquires.
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
            stream: Rc::new(RefCell::new(None)),
            generation: Rc::new(Cell::new(0)),
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
        if on {
            // The mic is (re)acquired on the next gesture (`on_user_gesture` →
            // `request_mic`); just wake the graph here.
            if let Some(ctx) = &self.ctx {
                let _ = ctx.resume();
            }
        } else {
            self.reading = None;
            // Fully release the mic (parity with the native backend): stops every
            // track, drops the analyser, lets a later enable re-acquire.
            self.release_mic();
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

    /// Stop mic capture and release the device. The generation bump invalidates
    /// any in-flight `getUserMedia` so a late stream is stopped, not kept.
    fn release_mic(&mut self) {
        self.generation.set(self.generation.get().wrapping_add(1));
        if let Ok(mut slot) = self.stream.try_borrow_mut()
            && let Some(stream) = slot.take()
        {
            stop_tracks(&stream);
        }
        if let Ok(mut slot) = self.analyser.try_borrow_mut() {
            *slot = None;
        }
        self.requested = false;
        // Idle the graph while off (saves CPU/battery); resumed on re-enable.
        if let Some(ctx) = &self.ctx {
            let _ = ctx.suspend();
        }
    }

    fn request_mic(&mut self) {
        if self.requested {
            return;
        }
        self.requested = true;
        // This acquisition's generation; the closure keeps the mic only if it
        // still matches when the stream arrives.
        self.generation.set(self.generation.get().wrapping_add(1));
        let my_gen = self.generation.get();

        // Reuse the AudioContext across toggles (a fresh one each time would leak).
        let ctx = match &self.ctx {
            Some(c) => c.clone(),
            None => match AudioContext::new() {
                Ok(c) => {
                    self.ctx = Some(c.clone());
                    c
                }
                Err(e) => {
                    log::error!("tuner: AudioContext failed: {e:?}");
                    self.requested = false;
                    return;
                }
            },
        };
        let _ = ctx.resume();

        let Some(window) = web_sys::window() else {
            self.requested = false;
            return;
        };
        let media_devices = match window.navigator().media_devices() {
            Ok(m) => m,
            Err(e) => {
                log::error!("tuner: media_devices unavailable: {e:?}");
                self.requested = false;
                return;
            }
        };

        let constraints = MediaStreamConstraints::new();
        constraints.set_audio_bool(true);
        let promise = match media_devices.get_user_media_with_constraints(&constraints) {
            Ok(p) => p,
            Err(e) => {
                log::error!("tuner: getUserMedia call failed: {e:?}");
                self.requested = false;
                return;
            }
        };

        let analyser_slot = self.analyser.clone();
        let stream_slot = self.stream.clone();
        let generation = self.generation.clone();
        spawn_local(async move {
            match JsFuture::from(promise).await {
                Ok(stream_val) => {
                    let stream: MediaStream = stream_val.unchecked_into();
                    // Off (or re-toggled) while the prompt was pending: release
                    // this stream instead of leaving the mic live.
                    if generation.get() != my_gen {
                        stop_tracks(&stream);
                        return;
                    }
                    let source = match ctx.create_media_stream_source(&stream) {
                        Ok(s) => s,
                        Err(e) => {
                            stop_tracks(&stream);
                            return log::error!("tuner: source node failed: {e:?}");
                        }
                    };
                    let analyser = match ctx.create_analyser() {
                        Ok(a) => a,
                        Err(e) => {
                            stop_tracks(&stream);
                            return log::error!("tuner: analyser failed: {e:?}");
                        }
                    };
                    analyser.set_fft_size(WINDOW as u32);
                    if let Err(e) = source.connect_with_audio_node(&analyser) {
                        stop_tracks(&stream);
                        return log::error!("tuner: connect failed: {e:?}");
                    }
                    let _ = ctx.resume();
                    if let Ok(mut slot) = analyser_slot.try_borrow_mut() {
                        *slot = Some(analyser);
                    }
                    if let Ok(mut slot) = stream_slot.try_borrow_mut() {
                        *slot = Some(stream);
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

/// Stop every track of a `MediaStream`, releasing the underlying device.
fn stop_tracks(stream: &MediaStream) {
    let tracks = stream.get_tracks();
    for i in 0..tracks.length() {
        if let Ok(track) = tracks.get(i).dyn_into::<web_sys::MediaStreamTrack>() {
            track.stop();
        }
    }
}
