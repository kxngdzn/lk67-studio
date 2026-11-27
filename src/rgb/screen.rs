//! Screen follow ("ambilight") input: capture the screen (or a region) at a low frame
//! rate and reduce it to a small colour grid that the lighting engine samples per key.
//!
//! Capture uses GDI `BitBlt` into a 32-bit DIB. That is plenty for 15-30 FPS ambience
//! and avoids pulling in a full graphics capture stack.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::model::lighting::{ScreenConfig, ScreenRegion};

#[derive(Clone, Debug)]
pub struct ScreenSnapshot {
    pub grid: Vec<[u8; 3]>,
    pub gw: usize,
    pub gh: usize,
    pub active: bool,
    pub error: Option<String>,
    pub fps: u8,
    /// Size of the captured display in pixels (0 until the first frame).
    pub source_width: i32,
    pub source_height: i32,
}

impl Default for ScreenSnapshot {
    fn default() -> Self {
        ScreenSnapshot {
            grid: Vec::new(),
            gw: 0,
            gh: 0,
            active: false,
            error: None,
            fps: 0,
            source_width: 0,
            source_height: 0,
        }
    }
}

pub struct ScreenEngine {
    shared: Arc<Mutex<ScreenSnapshot>>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    cfg: ScreenConfig,
    running: bool,
}

impl Default for ScreenEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ScreenEngine {
    pub fn new() -> Self {
        ScreenEngine {
            shared: Arc::new(Mutex::new(ScreenSnapshot::default())),
            stop: Arc::new(AtomicBool::new(false)),
            handle: None,
            cfg: ScreenConfig::default(),
            running: false,
        }
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn snapshot(&self) -> ScreenSnapshot {
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
            *guard = ScreenSnapshot::default();
        }
    }

    pub fn start(&mut self, cfg: ScreenConfig) {
        self.stop();
        self.cfg = cfg;
        self.stop = Arc::new(AtomicBool::new(false));
        let fps = self.cfg.fps.clamp(5, 30);
        if let Ok(mut guard) = self.shared.lock() {
            *guard = ScreenSnapshot {
                gw: self.cfg.grid_x as usize,
                gh: self.cfg.grid_y as usize,
                grid: vec![[0, 0, 0]; self.cfg.grid_x as usize * self.cfg.grid_y as usize],
                active: true,
                fps,
                error: None,
                source_width: 0,
                source_height: 0,
            };
        }
        let shared = self.shared.clone();
        let stop = self.stop.clone();
        let cfg = self.cfg.clone();
        self.handle = Some(
            thread::Builder::new()
                .name("lk67-screen".into())
                .spawn(move || capture_loop(cfg, shared, stop))
                .expect("spawn screen thread"),
        );
        self.running = true;
    }
}

impl Drop for ScreenEngine {
    fn drop(&mut self) {
        self.stop();
    }
}

fn capture_loop(cfg: ScreenConfig, shared: Arc<Mutex<ScreenSnapshot>>, stop: Arc<AtomicBool>) {
    let fps = cfg.fps.clamp(5, 30);
    let frame_time = Duration::from_secs_f32(1.0 / fps as f32);
    let gw = cfg.grid_x.max(4) as usize;
    let gh = cfg.grid_y.max(2) as usize;
    match Capturer::new() {
        Ok(mut capturer) => loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let started = Instant::now();
            let (source_width, source_height) = capturer.size();
            match capturer.sample(&cfg.region, gw, gh) {
                Ok(grid) => {
                    if let Ok(mut guard) = shared.lock() {
                        guard.grid = grid;
                        guard.gw = gw;
                        guard.gh = gh;
                        guard.active = true;
                        guard.fps = fps;
                        guard.error = None;
                        guard.source_width = source_width;
                        guard.source_height = source_height;
                    }
                }
                Err(err) => {
                    if let Ok(mut guard) = shared.lock() {
                        guard.error = Some(err);
                        guard.active = false;
                    }
                }
            }
            let elapsed = started.elapsed();
            if elapsed < frame_time {
                thread::sleep(frame_time - elapsed);
            }
        },
        Err(err) => {
            if let Ok(mut guard) = shared.lock() {
                guard.error = Some(err);
                guard.active = false;
            }
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::ffi::c_void;

    use windows_sys::Win32::Graphics::Gdi::{
        BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC,
        DeleteObject, GetDC, GetDIBits, ReleaseDC, SelectObject,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

    const SRCCOPY: u32 = 0x00CC_0020;
    const BI_RGB: u32 = 0;
    const DIB_RGB_COLORS: u32 = 0;

    pub struct Capturer {
        screen: *mut c_void,
        memory: *mut c_void,
        bitmap: *mut c_void,
        previous: *mut c_void,
        width: i32,
        height: i32,
        buffer: Vec<u8>,
    }

    impl Capturer {
        pub fn new() -> Result<Self, String> {
            let width = unsafe { GetSystemMetrics(SM_CXSCREEN) }.max(1);
            let height = unsafe { GetSystemMetrics(SM_CYSCREEN) }.max(1);
            unsafe {
                let screen = GetDC(std::ptr::null_mut());
                if screen.is_null() {
                    return Err("GetDC failed".into());
                }
                let memory = CreateCompatibleDC(screen);
                if memory.is_null() {
                    ReleaseDC(std::ptr::null_mut(), screen);
                    return Err("CreateCompatibleDC failed".into());
                }
                let bitmap = CreateCompatibleBitmap(screen, width, height);
                if bitmap.is_null() {
                    DeleteDC(memory);
                    ReleaseDC(std::ptr::null_mut(), screen);
                    return Err("CreateCompatibleBitmap failed".into());
                }
                let previous = SelectObject(memory, bitmap);
                Ok(Capturer {
                    screen,
                    memory,
                    bitmap,
                    previous,
                    width,
                    height,
                    buffer: vec![0u8; (width as usize) * (height as usize) * 4],
                })
            }
        }

        pub fn size(&self) -> (i32, i32) {
            (self.width, self.height)
        }

        pub fn sample(
            &mut self,
            region: &ScreenRegion,
            gw: usize,
            gh: usize,
        ) -> Result<Vec<[u8; 3]>, String> {
            let (x, y, w, h) = if region.is_full() {
                (0, 0, self.width, self.height)
            } else {
                let x = region.x.clamp(0, self.width - 1);
                let y = region.y.clamp(0, self.height - 1);
                let w = region.width.clamp(1, self.width - x);
                let h = region.height.clamp(1, self.height - y);
                (x, y, w, h)
            };
            unsafe {
                let ok = BitBlt(self.memory, 0, 0, w, h, self.screen, x, y, SRCCOPY);
                if ok == 0 {
                    return Err("BitBlt failed".into());
                }
                let mut info: BITMAPINFO = std::mem::zeroed();
                info.bmiHeader = BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h, // top-down rows
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    biSizeImage: 0,
                    biXPelsPerMeter: 0,
                    biYPelsPerMeter: 0,
                    biClrUsed: 0,
                    biClrImportant: 0,
                };
                let lines = GetDIBits(
                    self.memory,
                    self.bitmap,
                    0,
                    h as u32,
                    self.buffer.as_mut_ptr() as *mut c_void,
                    &mut info,
                    DIB_RGB_COLORS,
                );
                if lines == 0 {
                    return Err("GetDIBits failed".into());
                }
            }

            let mut grid = vec![[0u8, 0, 0]; gw * gh];
            let step_x = (w as usize / gw).max(1);
            let step_y = (h as usize / gh).max(1);
            // Subsample inside each cell so a 4K screen does not cost 8M pixel reads.
            let sample_step = (step_x / 12).max(1);
            for gy in 0..gh {
                for gx in 0..gw {
                    let mut sum = [0u32; 3];
                    let mut count = 0u32;
                    let base_y = (gy * h as usize / gh).min(h as usize - 1);
                    let base_x = (gx * w as usize / gw).min(w as usize - 1);
                    let mut py = base_y;
                    while py < (base_y + step_y).min(h as usize) {
                        let mut px = base_x;
                        while px < (base_x + step_x).min(w as usize) {
                            let offset = (py * w as usize + px) * 4;
                            if offset + 2 < self.buffer.len() {
                                // GDI DIB is BGRA.
                                sum[0] += self.buffer[offset + 2] as u32;
                                sum[1] += self.buffer[offset + 1] as u32;
                                sum[2] += self.buffer[offset] as u32;
                                count += 1;
                            }
                            px += sample_step;
                        }
                        py += sample_step;
                    }
                    if count > 0 {
                        grid[gy * gw + gx] = [
                            (sum[0] / count).min(255) as u8,
                            (sum[1] / count).min(255) as u8,
                            (sum[2] / count).min(255) as u8,
                        ];
                    }
                }
            }
            Ok(grid)
        }
    }

    impl Drop for Capturer {
        fn drop(&mut self) {
            unsafe {
                SelectObject(self.memory, self.previous);
                DeleteObject(self.bitmap);
                DeleteDC(self.memory);
                ReleaseDC(std::ptr::null_mut(), self.screen);
            }
        }
    }

    // All access happens on the capture thread.
    unsafe impl Send for Capturer {}
}

#[cfg(windows)]
use platform::Capturer;

#[cfg(not(windows))]
struct Capturer;

#[cfg(not(windows))]
impl Capturer {
    fn new() -> Result<Self, String> {
        Err("screen capture is currently Windows-only".into())
    }

    fn size(&self) -> (i32, i32) {
        (0, 0)
    }

    fn sample(
        &mut self,
        _region: &ScreenRegion,
        _gw: usize,
        _gh: usize,
    ) -> Result<Vec<[u8; 3]>, String> {
        Err("screen capture is currently Windows-only".into())
    }
}
