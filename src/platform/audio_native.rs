//! Native implementation of [`SoundOutput`] on top of rodio (CoreAudio /
//! WASAPI / ALSA through cpal).
//!
//! Samples are read straight from the Rangers sound archives — the files
//! the host's `m_SoundCreate` resolves in the original — named by
//! `MG_SOUND_PKGS` (a `:`-separated list, e.g. `DATA/Sound.pkg` and
//! `DATA/voicesRus.pkg`) or found next to `Data/robots.pkg`. The `Sound.X`
//! resource ids map to archive files through [`sound_pkg_map`]. Without
//! archives or an output device `ready() == false` and the mixer drops
//! every play, like the standalone C++ build.
//!
//! Each voice is one rodio source that applies its own volume and pan from
//! atomics, so `set_vol` / `set_pan` take effect while it plays.
//! `MG_SOUND_VOL` (0..1) is the host's master effects volume.

use super::sound_pkg_map::pkg_file_for;
use crate::matrix_game::sound::SoundOutput;
use crate::matrix_lib::base::pack::PkgArchive;
use rodio::{ChannelCount, Decoder, DeviceSinkBuilder, MixerDeviceSink, SampleRate, Source};
use std::collections::HashMap;
use std::io::Cursor;
use std::num::NonZero;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// A fully decoded sample, interleaved.
struct SampleData {
    data: Vec<f32>,
    channels: usize,
    rate: SampleRate,
}

/// Controls shared between a voice handle and its playing source.
struct Ctl {
    vol: AtomicU32,
    pan: AtomicU32,
    stop: AtomicBool,
    done: AtomicBool,
}

struct Voice {
    sample: Option<Arc<SampleData>>,
    looped: bool,
    ctl: Arc<Ctl>,
    started: bool,
}

/// Stereo source for one voice.
struct VoiceSource {
    sample: Arc<SampleData>,
    looped: bool,
    ctl: Arc<Ctl>,
    master: f32,
    frame: usize,
    right: bool,
}

impl Iterator for VoiceSource {
    type Item = rodio::Sample;

    fn next(&mut self) -> Option<Self::Item> {
        if self.ctl.stop.load(Ordering::Relaxed) {
            self.ctl.done.store(true, Ordering::Relaxed);
            return None;
        }
        let channels = self.sample.channels;
        let frames = self.sample.data.len() / channels;
        if self.frame >= frames {
            if self.looped && frames > 0 {
                self.frame = 0;
            } else {
                self.ctl.done.store(true, Ordering::Relaxed);
                return None;
            }
        }
        let vol = f32::from_bits(self.ctl.vol.load(Ordering::Relaxed)) * self.master;
        let pan = f32::from_bits(self.ctl.pan.load(Ordering::Relaxed));
        let base = self.frame * channels;
        let out = if self.right {
            let s = self.sample.data[base + (channels > 1) as usize];
            self.frame += 1;
            s * vol * (1.0 + pan.min(0.0))
        } else {
            self.sample.data[base] * vol * (1.0 - pan.max(0.0))
        };
        self.right = !self.right;
        Some(out as rodio::Sample)
    }
}

impl Source for VoiceSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> ChannelCount {
        NonZero::new(2).unwrap()
    }
    fn sample_rate(&self) -> SampleRate {
        self.sample.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

pub struct NativeOutput {
    sink: Option<MixerDeviceSink>,
    pkgs: Vec<PkgArchive>,
    samples: HashMap<String, Option<Arc<SampleData>>>,
    voices: HashMap<u32, Voice>,
    next_voice: u32,
    master: f32,
}

impl NativeOutput {
    pub fn new() -> Self {
        let paths: Vec<String> = match std::env::var("MG_SOUND_PKGS") {
            Ok(list) => list.split(':').filter(|p| !p.is_empty()).map(String::from).collect(),
            Err(_) => ["Data/Sound.pkg", "Data/voicesRus.pkg", "../Data/Sound.pkg"]
                .iter()
                .filter(|p| std::path::Path::new(p).exists())
                .map(|p| p.to_string())
                .collect(),
        };
        let mut pkgs = Vec::new();
        for path in &paths {
            match std::fs::read(path)
                .map_err(anyhow::Error::from)
                .and_then(PkgArchive::from_bytes)
            {
                Ok(pkg) => pkgs.push(pkg),
                Err(e) => log::warn!("audio: cannot load {path}: {e}"),
            }
        }
        let master = std::env::var("MG_SOUND_VOL")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        // No archives or a muted host: stay silent without opening a device.
        let sink = if pkgs.is_empty() || master <= 0.0 {
            None
        } else {
            match DeviceSinkBuilder::open_default_sink() {
                Ok(mut sink) => {
                    sink.log_on_drop(false);
                    Some(sink)
                }
                Err(e) => {
                    log::warn!("audio: no output device: {e}");
                    None
                }
            }
        };
        log::info!(
            "audio: native output {} ({} archive(s), volume {master})",
            if sink.is_some() { "ready" } else { "silent" },
            pkgs.len(),
        );
        Self {
            sink,
            pkgs,
            samples: HashMap::new(),
            voices: HashMap::new(),
            next_voice: 1,
            master,
        }
    }

    /// Decode (once) the sample behind a `Sound.X` resource id.
    fn sample(&mut self, path: &str) -> Option<Arc<SampleData>> {
        if let Some(cached) = self.samples.get(path) {
            return cached.clone();
        }
        let decoded = decode_sample(&self.pkgs, path);
        if decoded.is_none() {
            log::debug!("audio: no sample for {path}");
        }
        self.samples.insert(path.to_string(), decoded.clone());
        decoded
    }
}

impl Default for NativeOutput {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolve a `Sound.X` id to its archive file and decode it. Public so
/// the headless check (`examples/native_sound_check.rs`) covers the same path.
pub fn decode_from_pkgs(pkgs: &[PkgArchive], path: &str) -> Result<(usize, u32, usize), String> {
    let file = pkg_file_for(path).ok_or_else(|| "no archive mapping".to_string())?;
    let bytes = pkgs
        .iter()
        .find_map(|pkg| pkg.read_file(file).ok())
        .ok_or_else(|| format!("{file} not in archives"))?;
    let source = Decoder::new(Cursor::new(bytes)).map_err(|e| format!("{file}: {e}"))?;
    let channels = source.channels().get() as usize;
    let rate = source.sample_rate().get();
    let frames = source.count() / channels;
    Ok((channels, rate, frames))
}

fn decode_sample(pkgs: &[PkgArchive], path: &str) -> Option<Arc<SampleData>> {
    let file = pkg_file_for(path)?;
    let bytes = pkgs.iter().find_map(|pkg| pkg.read_file(file).ok())?;
    let source = match Decoder::new(Cursor::new(bytes)) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("audio: cannot decode {file}: {e}");
            return None;
        }
    };
    let channels = source.channels().get() as usize;
    let rate = source.sample_rate();
    let data: Vec<f32> = source.map(|s| s as f32).collect();
    Some(Arc::new(SampleData { data, channels, rate }))
}

impl SoundOutput for NativeOutput {
    fn ready(&self) -> bool {
        self.sink.is_some()
    }

    fn create(&mut self, path: &str, looped: bool) -> u32 {
        if self.sink.is_none() {
            return 0;
        }
        let sample = self.sample(path);
        let v = self.next_voice;
        self.next_voice += 1;
        self.voices.insert(
            v,
            Voice {
                sample,
                looped,
                ctl: Arc::new(Ctl {
                    vol: AtomicU32::new(1.0f32.to_bits()),
                    pan: AtomicU32::new(0.0f32.to_bits()),
                    stop: AtomicBool::new(false),
                    done: AtomicBool::new(false),
                }),
                started: false,
            },
        );
        v
    }

    fn play(&mut self, voice: u32) {
        let (Some(sink), Some(v)) = (&self.sink, self.voices.get_mut(&voice)) else {
            return;
        };
        if v.started && !v.ctl.done.load(Ordering::Relaxed) {
            return;
        }
        v.started = true;
        let Some(sample) = v.sample.clone() else {
            v.ctl.done.store(true, Ordering::Relaxed);
            return;
        };
        v.ctl.done.store(false, Ordering::Relaxed);
        sink.mixer().add(VoiceSource {
            sample,
            looped: v.looped,
            ctl: v.ctl.clone(),
            master: self.master,
            frame: 0,
            right: false,
        });
    }

    fn set_pan(&mut self, voice: u32, pan: f32) {
        if let Some(v) = self.voices.get(&voice) {
            v.ctl.pan.store(pan.clamp(-1.0, 1.0).to_bits(), Ordering::Relaxed);
        }
    }

    fn set_vol(&mut self, voice: u32, vol: f32) {
        if let Some(v) = self.voices.get(&voice) {
            v.ctl.vol.store(vol.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
        }
    }

    fn is_playing(&self, voice: u32) -> bool {
        self.voices
            .get(&voice)
            .is_some_and(|v| v.started && !v.ctl.done.load(Ordering::Relaxed))
    }

    fn destroy(&mut self, voice: u32) {
        if let Some(v) = self.voices.remove(&voice) {
            v.ctl.stop.store(true, Ordering::Relaxed);
        }
    }

    /// The soundtrack belongs to the Rangers host process.
    fn set_music_volume(&mut self, _vol: f32) {}
}
