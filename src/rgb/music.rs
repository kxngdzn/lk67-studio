//! Music reactive input: WASAPI (system loopback or default microphone) -> FFT bands.
//!
//! The capture thread owns all COM objects and publishes a small snapshot behind a
//! mutex. Rendering never blocks on audio: it reads whatever the last snapshot was.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use rustfft::FftPlanner;
use rustfft::num_complex::Complex;
use wasapi::{DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat, initialize_mta};

use crate::model::lighting::AudioSource;

pub const BANDS: usize = 16;
const FFT_SIZE: usize = 2048;
const SAMPLE_RATE: usize = 48_000;

#[derive(Clone, Debug)]
pub struct AudioSnapshot {
    pub bands: [f32; BANDS],
    pub level: f32,
    /// 1.0 on a detected beat; the renderer decays it.
    pub beat: f32,
    pub peak: f32,
    pub active: bool,
    pub error: Option<String>,
    pub sample_rate: u32,
}

impl Default for AudioSnapshot {
    fn default() -> Self {
        AudioSnapshot {
            bands: [0.0; BANDS],
            level: 0.0,
            beat: 0.0,
            peak: 0.0,
            active: false,
            error: None,
            sample_rate: SAMPLE_RATE as u32,
        }
    }
}

pub struct AudioEngine {
    shared: Arc<Mutex<AudioSnapshot>>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    source: AudioSource,
    running: bool,
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEngine {
    pub fn new() -> Self {
        AudioEngine {
            shared: Arc::new(Mutex::new(AudioSnapshot::default())),
            stop: Arc::new(AtomicBool::new(false)),
            handle: None,
            source: AudioSource::Loopback,
            running: false,
        }
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn snapshot(&self) -> AudioSnapshot {
        self.shared
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        self.running = false;
        if let Ok(mut guard) = self.shared.lock() {
            *guard = AudioSnapshot::default();
        }
    }

    pub fn start(&mut self, source: AudioSource) {
        self.stop();
        self.source = source;
        self.stop = Arc::new(AtomicBool::new(false));
        if let Ok(mut guard) = self.shared.lock() {
            *guard = AudioSnapshot {
                active: true,
                error: None,
                sample_rate: SAMPLE_RATE as u32,
                ..AudioSnapshot::default()
            };
        }
        let shared = self.shared.clone();
        let stop = self.stop.clone();
        self.handle = Some(
            thread::Builder::new()
                .name("lk67-audio".into())
                .spawn(move || capture_loop(source, shared, stop))
                .expect("spawn audio thread"),
        );
        self.running = true;
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        self.stop();
    }
}

fn capture_loop(source: AudioSource, shared: Arc<Mutex<AudioSnapshot>>, stop: Arc<AtomicBool>) {
    if let Err(err) = run_capture(source, shared.clone(), stop) {
        if let Ok(mut guard) = shared.lock() {
            guard.error = Some(err);
            guard.active = false;
            guard.bands = [0.0; BANDS];
            guard.level = 0.0;
        }
    }
}

fn run_capture(
    source: AudioSource,
    shared: Arc<Mutex<AudioSnapshot>>,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    initialize_mta()
        .ok()
        .map_err(|e| format!("COM init failed: {e}"))?;
    let enumerator = DeviceEnumerator::new().map_err(|e| format!("no audio devices: {e}"))?;
    let direction = match source {
        AudioSource::Loopback => Direction::Render,
        AudioSource::Microphone => Direction::Capture,
    };
    let device = enumerator
        .get_default_device(direction)
        .map_err(|e| format!("default device: {e}"))?;
    let mut client = device
        .get_iaudioclient()
        .map_err(|e| format!("audio client: {e}"))?;
    let format = WaveFormat::new(32, 32, SampleType::Float, SAMPLE_RATE, 2, None);
    let (_, min_period) = client
        .get_device_period()
        .map_err(|e| format!("device period: {e}"))?;
    let mode = StreamMode::PollingShared {
        autoconvert: true,
        buffer_duration_hns: min_period,
    };
    client
        .initialize_client(&format, Direction::Capture, mode, None)
        .map_err(|e| format!("initialize capture (loopback needs a render device): {e}"))?;
    let capture = client
        .get_audiocaptureclient()
        .map_err(|e| format!("capture client: {e}"))?;
    client
        .start_stream()
        .map_err(|e| format!("start stream: {e}"))?;

    let mut deque: VecDeque<u8> = VecDeque::with_capacity(64 * 1024);
    let mut analyzer = Analyzer::new(shared.clone());
    while !stop.load(Ordering::Relaxed) {
        match capture.read_from_device_to_deque(&mut deque) {
            Ok(_) => {}
            Err(err) => {
                let _ = client.stop_stream();
                return Err(format!("read audio: {err}"));
            }
        }
        // 32-bit float stereo frames = 8 bytes. Decode into mono.
        while deque.len() >= 8 {
            let mut frame = [0u8; 8];
            for byte in frame.iter_mut() {
                *byte = deque.pop_front().unwrap_or(0);
            }
            let left = f32::from_le_bytes([frame[0], frame[1], frame[2], frame[3]]);
            let right = f32::from_le_bytes([frame[4], frame[5], frame[6], frame[7]]);
            analyzer.push((left + right) * 0.5);
        }
        analyzer.publish();
        thread::sleep(Duration::from_millis(8));
    }
    let _ = client.stop_stream();
    Ok(())
}

struct Analyzer {
    fft: Arc<dyn rustfft::Fft<f32>>,
    window: Vec<f32>,
    samples: Vec<f32>,
    scratch: Vec<Complex<f32>>,
    ranges: Vec<(usize, usize)>,
    history: VecDeque<f32>,
    smooth_bands: [f32; BANDS],
    last_beat: Instant,
    shared: Arc<Mutex<AudioSnapshot>>,
    published: Option<AudioSnapshot>,
    last_publish: Instant,
    rms: f32,
}

impl Analyzer {
    fn new(shared: Arc<Mutex<AudioSnapshot>>) -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);
        let window = (0..FFT_SIZE)
            .map(|i| {
                let x = i as f32 / FFT_SIZE as f32;
                0.5 - 0.5 * (std::f32::consts::TAU * x).cos()
            })
            .collect();
        // Log-spaced bands from 40 Hz to 16 kHz.
        let bin_hz = SAMPLE_RATE as f32 / FFT_SIZE as f32;
        let mut ranges = Vec::with_capacity(BANDS);
        for band in 0..BANDS {
            let t0 = band as f32 / BANDS as f32;
            let t1 = (band + 1) as f32 / BANDS as f32;
            let low = 40.0 * (16_000.0f32 / 40.0).powf(t0);
            let high = 40.0 * (16_000.0f32 / 40.0).powf(t1);
            let start = ((low / bin_hz).floor() as usize)
                .max(1)
                .min(FFT_SIZE / 2 - 1);
            let end = ((high / bin_hz).ceil() as usize)
                .max(start + 1)
                .min(FFT_SIZE / 2);
            ranges.push((start, end));
        }
        Analyzer {
            fft,
            window,
            samples: Vec::with_capacity(FFT_SIZE),
            scratch: vec![Complex::new(0.0, 0.0); FFT_SIZE],
            ranges,
            history: VecDeque::with_capacity(64),
            smooth_bands: [0.0; BANDS],
            last_beat: Instant::now() - Duration::from_secs(2),
            shared,
            published: None,
            last_publish: Instant::now() - Duration::from_secs(1),
            rms: 0.0,
        }
    }

    fn push(&mut self, sample: f32) {
        let sample = if sample.is_finite() {
            sample.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        self.samples.push(sample);
        if self.samples.len() >= FFT_SIZE {
            self.analyze();
            // 50% overlap keeps the meter smooth.
            self.samples.drain(..FFT_SIZE / 2);
        }
    }

    fn analyze(&mut self) {
        for (i, sample) in self.samples.iter().take(FFT_SIZE).enumerate() {
            self.scratch[i] = Complex::new(sample * self.window[i], 0.0);
        }
        self.fft.process(&mut self.scratch);
        let norm = 2.0 / FFT_SIZE as f32;
        let mut bands = [0.0f32; BANDS];
        for (band, (start, end)) in self.ranges.iter().enumerate() {
            let mut peak = 0.0f32;
            for bin in *start..*end {
                let magnitude = self.scratch[bin].norm() * norm;
                peak = peak.max(magnitude);
            }
            // -68 dB..-6 dB mapped to 0..1; quiet content still moves the meter.
            let db = 20.0 * (peak.max(1e-7)).log10();
            bands[band] = ((db + 68.0) / 62.0).clamp(0.0, 1.0);
        }
        for band in 0..BANDS {
            let target = bands[band];
            self.smooth_bands[band] += (target - self.smooth_bands[band]) * 0.45;
        }
        let rms = (self
            .samples
            .iter()
            .take(FFT_SIZE)
            .map(|s| s * s)
            .sum::<f32>()
            / FFT_SIZE as f32)
            .sqrt();
        self.rms += (rms - self.rms) * 0.3;

        let now = Instant::now();
        if now.duration_since(self.last_beat) > Duration::from_millis(140) {
            let average = if self.history.is_empty() {
                self.rms
            } else {
                self.history.iter().sum::<f32>() / self.history.len() as f32
            };
            if self.rms > average * 1.45 && self.rms > 0.012 {
                if let Some(snapshot) = self.published.as_mut() {
                    snapshot.beat = 1.0;
                }
                if let Ok(mut guard) = self.shared.lock() {
                    guard.beat = 1.0;
                }
                self.last_beat = now;
            }
        }
        self.history.push_back(self.rms);
        while self.history.len() > 48 {
            self.history.pop_front();
        }
    }

    fn publish(&mut self) {
        if self.last_publish.elapsed() < Duration::from_millis(33) {
            return;
        }
        self.last_publish = Instant::now();
        let level = (self.rms * 4.0).clamp(0.0, 1.0);
        let peak = self.smooth_bands.iter().cloned().fold(0.0f32, f32::max);
        let mut snapshot = self.published.take().unwrap_or_default();
        snapshot.bands = self.smooth_bands;
        snapshot.level = level;
        snapshot.peak = peak;
        snapshot.active = true;
        snapshot.error = None;
        snapshot.sample_rate = SAMPLE_RATE as u32;
        if let Ok(mut guard) = self.shared.lock() {
            let beat = guard.beat.max(snapshot.beat);
            *guard = snapshot.clone();
            guard.beat = beat;
        }
        self.published = Some(snapshot);
    }
}
