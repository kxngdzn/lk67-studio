//! Hardware abstraction.
//!
//! `KeyboardDriver` is the boundary between the app and the keyboard. The LK67
//! implementation talks HID when a device is open and otherwise (or when Dry-Run is
//! enabled) validates and logs the exact packets it would have sent.
//!
//! All protocol constants live in [`lk67`]; see the module docs there before trusting
//! any report ID or byte layout.

pub mod lk67;

use std::collections::VecDeque;
use std::fmt;
use std::time::Instant;

use hidapi::{HidApi, HidDevice};

use crate::model::Action;
use crate::model::layout::Layout;
use crate::model::lighting::Lighting;
use lk67::{
    DriverConfig, MUSIC_HEIGHTS, ReportTransport, action_entry, build_effect_report,
    build_get_keymap_page, build_get_led_report, build_key_color_reports, build_keymap_report,
    build_music_report, build_profile_report, build_screen_color_report, changed_keys,
};

/// Output/feature report lengths of a HID collection, or `None` if it cannot be
/// queried (standard keyboard/mouse collections refuse even a read handle).
#[cfg(windows)]
fn query_report_caps(path: &std::ffi::CStr) -> Option<(usize, usize)> {
    use windows_sys::Win32::Devices::HumanInterfaceDevice::{
        HIDP_CAPS, HidD_FreePreparsedData, HidD_GetPreparsedData, HidP_GetCaps,
    };
    use windows_sys::Win32::Foundation::{
        CloseHandle, GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    let wide: Vec<u16> = path
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return None;
    }
    let mut preparsed: isize = 0;
    if !unsafe { HidD_GetPreparsedData(handle, &mut preparsed) } {
        unsafe { CloseHandle(handle) };
        return None;
    }
    let mut caps: HIDP_CAPS = unsafe { std::mem::zeroed() };
    let status = unsafe { HidP_GetCaps(preparsed, &mut caps) };
    unsafe {
        HidD_FreePreparsedData(preparsed);
        CloseHandle(handle);
    }
    if status < 0 {
        return None;
    }
    Some((
        caps.OutputReportByteLength as usize,
        caps.FeatureReportByteLength as usize,
    ))
}

#[cfg(not(windows))]
fn query_report_caps(_path: &std::ffi::CStr) -> Option<(usize, usize)> {
    None
}

/// Live connection state shown in the UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceStatus {
    Disconnected,
    /// No hardware writes; packets are validated and logged.
    Simulation,
    Connected {
        path: String,
        product: String,
        manufacturer: String,
        interface: i32,
    },
}

impl DeviceStatus {
    pub fn label(&self) -> String {
        match self {
            DeviceStatus::Disconnected => "Not connected".into(),
            DeviceStatus::Simulation => "Dry-run".into(),
            DeviceStatus::Connected { product, .. } => {
                format!(
                    "{} connected",
                    if product.is_empty() { "LK67" } else { product }
                )
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct HidRecord {
    pub path: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub product: String,
    pub manufacturer: String,
    pub interface: i32,
    pub usage_page: u16,
    pub usage: u16,
    /// Bytes the collection accepts per output/feature report (0 = not supported).
    pub output_len: usize,
    pub feature_len: usize,
    pub is_match: bool,
}

impl HidRecord {
    pub fn title(&self) -> String {
        format!(
            "{:04X}:{:04X}  {}",
            self.vendor_id,
            self.product_id,
            if self.product.is_empty() {
                "(unnamed HID device)"
            } else {
                &self.product
            }
        )
    }

    pub fn is_vendor(&self) -> bool {
        self.usage_page >= 0xFF00
    }

    pub fn is_writable(&self) -> bool {
        self.output_len > 0 || self.feature_len > 0
    }

    pub fn is_write_restricted(&self) -> bool {
        !self.is_writable()
    }

    pub fn collection_kind(&self) -> &'static str {
        match (self.usage_page, self.usage) {
            (0x01, 0x06) => "keyboard, read-only",
            (0x01, 0x02) => "mouse, read-only",
            (0x01, 0x80) => "system control",
            (0x0C, _) => "consumer control",
            (page, _) if page >= 0xFF00 => "vendor",
            _ => "other",
        }
    }

    pub fn subtitle(&self) -> String {
        let mut parts = Vec::new();
        if !self.manufacturer.is_empty() {
            parts.push(self.manufacturer.clone());
        }
        parts.push(format!("interface {}", self.interface));
        parts.push(format!(
            "usage {:04X}/{:04X} ({})",
            self.usage_page,
            self.usage,
            self.collection_kind()
        ));
        parts.push(if self.is_writable() {
            format!("out {} / feature {}", self.output_len, self.feature_len)
        } else {
            "no writable report".to_string()
        });
        parts.push(self.path.clone());
        parts.join("  \u{2022}  ")
    }
}

#[derive(Clone, Debug)]
pub struct PacketLogEntry {
    pub at: Instant,
    pub label: String,
    pub bytes: Vec<u8>,
    pub ok: bool,
    pub note: String,
}

#[derive(Clone, Debug)]
pub enum DriverError {
    NotConnected,
    Hid(String),
}

impl fmt::Display for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriverError::NotConnected => f.write_str("no LK67 HID interface is open"),
            DriverError::Hid(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for DriverError {}

/// Everything the app needs from a keyboard backend.
pub trait KeyboardDriver {
    fn status(&self) -> &DeviceStatus;
    fn is_dry_run(&self) -> bool;
    fn set_dry_run(&mut self, dry_run: bool);
    fn rescan(&mut self);
    /// Raw report write (also used by the packet inspector).
    fn send_report(&mut self, label: &str, report: &[u8]) -> Result<(), DriverError>;
    /// Hardware effect, palette, brightness, speed and direction.
    fn apply_lighting(&mut self, lighting: &Lighting) -> Result<usize, DriverError>;
    /// Upload a per-key picture and show it (mode 13). Flash-backed and slow.
    fn upload_custom_layout(&mut self, lighting: &Lighting) -> Result<usize, DriverError>;
    /// Live music bars (`0x0D`), mode 20/22.
    fn stream_music_bands(&mut self, heights: &[u8; MUSIC_HEIGHTS]) -> Result<(), DriverError>;
    /// Live screen colour (`0x0E`), mode 21.
    fn stream_screen_color(&mut self, rgb: [u8; 3]) -> Result<(), DriverError>;
    /// Write the keymap entries that differ from the stock layout.
    fn apply_keymap(
        &mut self,
        layout: &Layout,
        normal: &[Action],
        func: &[Action],
        profile: u8,
    ) -> Result<usize, DriverError>;
    fn select_profile(&mut self, slot: u8) -> Result<(), DriverError>;
    /// Non-blocking read of hardware input reports (placeholder parsing).
    fn poll(&mut self) -> Vec<Vec<u8>>;
    fn log(&self) -> &VecDeque<PacketLogEntry>;
    fn clear_log(&mut self);
}

#[derive(Clone, Debug)]
struct HidCandidate {
    path: String,
    interface: i32,
    usage_page: u16,
    usage: u16,
    output_len: usize,
    feature_len: usize,
}

impl HidCandidate {
    fn is_vendor(&self) -> bool {
        self.usage_page >= 0xFF00
    }

    fn is_restricted(&self) -> bool {
        self.usage_page == 0x01 && matches!(self.usage, 0x02 | 0x06)
    }

    fn is_writable(&self) -> bool {
        self.output_len > 0 || self.feature_len > 0
    }
}

/// Actions the LK67 firmware can store in a keymap slot.
fn hardware_action(action: &Action) -> bool {
    match action {
        Action::Key { key } => lk67::vk_to_hid(key.vk()).is_some(),
        Action::Media { media } => lk67::vk_to_consumer(media.vk()).is_some(),
        Action::None | Action::Fn => true,
        Action::Combo { .. }
        | Action::Macro { .. }
        | Action::Rgb { .. }
        | Action::Profile { .. } => false,
    }
}

fn key_slot(index: usize) -> u16 {
    lk67::KEY_SLOTS.get(index).copied().unwrap_or(index as u16)
}

/// Pick the writable collection: exact configured usage, then a vendor-only
/// interface, then any vendor collection, then a keyboard/mouse fallback.
fn choose_candidate<'a>(
    candidates: &'a [HidCandidate],
    cfg: &DriverConfig,
) -> Option<&'a HidCandidate> {
    if candidates.is_empty() {
        return None;
    }
    if cfg.usage_page >= 0xFF00
        && let Some(found) = candidates.iter().find(|c| {
            c.usage_page == cfg.usage_page
                && c.usage == cfg.usage
                && c.is_writable()
                && (cfg.interface < 0 || c.interface == cfg.interface)
        })
    {
        return Some(found);
    }
    let mut dedicated: Vec<&HidCandidate> = candidates
        .iter()
        .filter(|c| {
            c.is_vendor()
                && c.is_writable()
                && !candidates
                    .iter()
                    .any(|o| o.interface == c.interface && o.is_restricted())
        })
        .collect();
    dedicated.sort_by_key(|c| std::cmp::Reverse(c.interface));
    if let Some(found) = dedicated.first() {
        return Some(found);
    }
    let mut vendors: Vec<&HidCandidate> = candidates
        .iter()
        .filter(|c| c.is_vendor() && c.is_writable())
        .collect();
    vendors.sort_by_key(|c| std::cmp::Reverse(c.interface));
    if let Some(found) = vendors.first() {
        return Some(found);
    }
    if cfg.interface >= 0 {
        if let Some(found) = candidates
            .iter()
            .find(|c| c.interface == cfg.interface && c.is_writable())
        {
            return Some(found);
        }
        if let Some(found) = candidates.iter().find(|c| c.interface == cfg.interface) {
            return Some(found);
        }
    }
    candidates
        .iter()
        .find(|c| c.is_writable())
        .or_else(|| candidates.first())
}

pub struct Lk67Driver {
    cfg: DriverConfig,
    dry_run: bool,
    api: Option<HidApi>,
    device: Option<HidDevice>,
    status: DeviceStatus,
    log: VecDeque<PacketLogEntry>,
    last_error: Option<String>,
    writes: u64,
    opened_usage: Option<(u16, u16)>,
    opened_caps: Option<(usize, usize)>,
}

impl Lk67Driver {
    pub fn new(cfg: DriverConfig, dry_run: bool) -> Self {
        let mut driver = Lk67Driver {
            cfg,
            dry_run,
            api: None,
            device: None,
            status: if dry_run {
                DeviceStatus::Simulation
            } else {
                DeviceStatus::Disconnected
            },
            log: VecDeque::with_capacity(200),
            last_error: None,
            writes: 0,
            opened_usage: None,
            opened_caps: None,
        };
        if !dry_run {
            driver.rescan();
        }
        driver
    }

    pub fn set_config(&mut self, cfg: DriverConfig) {
        self.cfg = cfg;
        self.cfg.normalize();
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn writes(&self) -> u64 {
        self.writes
    }

    fn note(
        &mut self,
        label: impl Into<String>,
        bytes: Vec<u8>,
        ok: bool,
        note: impl Into<String>,
    ) {
        self.log.push_back(PacketLogEntry {
            at: Instant::now(),
            label: label.into(),
            bytes,
            ok,
            note: note.into(),
        });
        while self.log.len() > 300 {
            self.log.pop_front();
        }
    }

    fn ensure_api(&mut self) -> Result<(), DriverError> {
        if self.api.is_none() {
            match HidApi::new() {
                Ok(api) => self.api = Some(api),
                Err(err) => {
                    let message = format!("HID init failed: {err}");
                    self.last_error = Some(message.clone());
                    return Err(DriverError::Hid(message));
                }
            }
        }
        Ok(())
    }

    /// Every HID device the OS knows about, for the discovery list.
    pub fn discover(&mut self) -> Vec<HidRecord> {
        if let Err(err) = self.ensure_api() {
            self.last_error = Some(err.to_string());
            return Vec::new();
        }
        let Some(api) = self.api.as_ref() else {
            return Vec::new();
        };
        let cfg = self.cfg.clone();
        api.device_list()
            .map(|info| {
                let vendor_id = info.vendor_id();
                let product_id = info.product_id();
                let (output_len, feature_len) = if info.usage_page() >= 0xFF00 {
                    query_report_caps(info.path()).unwrap_or((0, 0))
                } else {
                    (0, 0)
                };
                HidRecord {
                    path: info.path().to_string_lossy().into_owned(),
                    vendor_id,
                    product_id,
                    product: info.product_string().unwrap_or("").to_string(),
                    manufacturer: info.manufacturer_string().unwrap_or("").to_string(),
                    interface: info.interface_number(),
                    usage_page: info.usage_page(),
                    usage: info.usage(),
                    output_len,
                    feature_len,
                    is_match: vendor_id == cfg.vid
                        && product_id == cfg.pid
                        && (cfg.interface < 0 || info.interface_number() == cfg.interface),
                }
            })
            .collect()
    }

    /// Open a specific HID path (from the discovery list).
    pub fn open_path(&mut self, path: &str) -> Result<(), String> {
        if self.dry_run {
            return Err("Dry-run is enabled".into());
        }
        self.ensure_api().map_err(|e| e.to_string())?;
        let api = self.api.as_ref().ok_or("HID not initialised")?;
        let c_path = std::ffi::CString::new(path).map_err(|e| format!("bad path: {e}"))?;
        let meta = api
            .device_list()
            .find(|info| info.path() == c_path.as_c_str())
            .map(|info| (info.interface_number(), info.usage_page(), info.usage()));
        let device = api
            .open_path(&c_path)
            .map_err(|e| format!("open {path}: {e}"))?;
        let _ = device.set_blocking_mode(false);
        let product = device
            .get_product_string()
            .ok()
            .flatten()
            .unwrap_or_default();
        let manufacturer = device
            .get_manufacturer_string()
            .ok()
            .flatten()
            .unwrap_or_default();
        self.device = Some(device);
        self.opened_usage = meta.map(|m| (m.1, m.2));
        self.opened_caps = query_report_caps(c_path.as_c_str());
        self.status = DeviceStatus::Connected {
            path: path.to_string(),
            product,
            manufacturer,
            interface: meta.map(|m| m.0).unwrap_or(self.cfg.interface),
        };
        self.note("Open device", Vec::new(), true, path.to_string());
        Ok(())
    }

    fn try_open_configured(&mut self) -> bool {
        let cfg = self.cfg.clone();
        let Some(api) = self.api.as_ref() else {
            return false;
        };
        let candidates: Vec<HidCandidate> = api
            .device_list()
            .filter(|info| info.vendor_id() == cfg.vid && info.product_id() == cfg.pid)
            .map(|info| {
                let (output_len, feature_len) = query_report_caps(info.path()).unwrap_or((0, 0));
                HidCandidate {
                    path: info.path().to_string_lossy().into_owned(),
                    interface: info.interface_number(),
                    usage_page: info.usage_page(),
                    usage: info.usage(),
                    output_len,
                    feature_len,
                }
            })
            .collect();
        let Some(chosen) = choose_candidate(&candidates, &cfg) else {
            return false;
        };
        let path = chosen.path.clone();
        let writable = chosen.is_writable();
        match self.open_path(&path) {
            Ok(()) => {
                if !writable {
                    self.last_error = Some(
                        "Opened a collection with no writable output/feature report. Select a VENDOR collection with a feature report."
                            .into(),
                    );
                }
                true
            }
            Err(err) => {
                self.last_error = Some(err.clone());
                self.note("Open failed", Vec::new(), false, err);
                false
            }
        }
    }

    fn read_feature(&mut self, label: &str, request: Vec<u8>) -> Result<Vec<u8>, DriverError> {
        if self.dry_run {
            return Err(DriverError::Hid("dry-run is enabled".into()));
        }
        if self.device.is_none() {
            self.try_open_configured();
        }
        let result = {
            let Some(device) = self.device.as_ref() else {
                return Err(DriverError::NotConnected);
            };
            let sent = device.send_feature_report(&request);
            if let Err(err) = sent {
                Err(format!("HID feature write failed: {err}"))
            } else {
                std::thread::sleep(std::time::Duration::from_millis(40));
                let mut buf = vec![0u8; self.cfg.report_len.max(65)];
                buf[0] = self.cfg.report_id;
                match device.get_feature_report(&mut buf) {
                    Ok(n) if n > 1 => Ok(buf[1..n].to_vec()),
                    Ok(n) => Err(format!("HID feature read returned {n} bytes")),
                    Err(err) => Err(format!("HID feature read failed: {err}")),
                }
            }
        };
        match result {
            Ok(data) => {
                self.note(
                    label,
                    request,
                    true,
                    format!("read {} data bytes", data.len()),
                );
                Ok(data)
            }
            Err(message) => {
                self.last_error = Some(message.clone());
                self.note(label, request, false, message.clone());
                Err(DriverError::Hid(message))
            }
        }
    }

    /// Probe: read the current LEDPARAM block (`0x87`).
    pub fn probe_lighting(&mut self) -> Result<Vec<u8>, DriverError> {
        let request = build_get_led_report(&self.cfg);
        self.read_feature("GET_LEDPARAM", request)
    }

    /// Read one whole 512-byte keymap layer as 8 raw pages.
    pub fn read_keymap_layer(&mut self, profile: u8, func: bool) -> Result<[u8; 512], DriverError> {
        let mut out = [0u8; 512];
        for page in 0u8..8 {
            let request = build_get_keymap_page(&self.cfg, profile, page, func);
            let reply = self.read_feature("GET keymap page", request)?;
            std::thread::sleep(std::time::Duration::from_millis(20));
            let count = reply.len().min(64);
            out[page as usize * 64..page as usize * 64 + count].copy_from_slice(&reply[..count]);
        }
        Ok(out)
    }

    /// Probe: read all 128 keymap slots of a profile (8 pages).
    pub fn dump_keymap(&mut self, profile: u8, func: bool) -> Result<Vec<[u8; 4]>, DriverError> {
        let raw = self.read_keymap_layer(profile, func)?;
        let mut slots = vec![[0u8; 4]; 128];
        for (slot, entry) in slots.iter_mut().enumerate() {
            entry.copy_from_slice(&raw[slot * 4..slot * 4 + 4]);
        }
        Ok(slots)
    }

    fn write_packet(&mut self, label: &str, report: Vec<u8>) -> Result<(), DriverError> {
        if self.dry_run {
            self.note(label, report, true, "dry-run (not written)");
            return Ok(());
        }
        if self.device.is_none() {
            self.try_open_configured();
        }
        let Some(device) = self.device.as_ref() else {
            self.note(label, report, false, "device not open");
            return Err(DriverError::NotConnected);
        };
        let transport = self.cfg.transport;
        let result = match transport {
            ReportTransport::FeatureReport => {
                device.send_feature_report(&report).map(|_| report.len())
            }
            ReportTransport::OutputReport => device.write(&report),
        };
        if let Err(err) = &result {
            let text = err.to_string();
            let denied = text.contains("0x00000005")
                || text.contains("Access is denied")
                || text.contains("Toegang geweigerd");
            if denied
                && transport == ReportTransport::OutputReport
                && device.send_feature_report(&report).is_ok()
            {
                self.writes += 1;
                self.note(
                    label,
                    report,
                    true,
                    "output report denied; sent as feature report",
                );
                return Ok(());
            }
        }
        match result {
            Ok(written) => {
                self.writes += 1;
                self.note(label, report, true, format!("wrote {written} bytes"));
                Ok(())
            }
            Err(err) => {
                let text = err.to_string();
                let denied = text.contains("0x00000005")
                    || text.contains("Access is denied")
                    || text.contains("Toegang geweigerd");
                let invalid_function = text.contains("0x00000001");
                let no_reports = matches!(self.opened_caps, Some((0, 0)));
                let message = if denied || invalid_function || no_reports {
                    let usage = self
                        .opened_usage
                        .map(|(page, usage)| format!(" (collection usage {page:04X}/{usage:04X})"))
                        .unwrap_or_default();
                    let reports = self
                        .opened_caps
                        .map(|(out, feature)| format!(" (output {out}, feature {feature} bytes)"))
                        .unwrap_or_default();
                    format!(
                        "HID write failed: {err}{usage}{reports}. Select a VENDOR collection that has a writable report (the LK67 control collection is usage 0xFFFF/0002 with a 65-byte feature report) and keep Transport on 'Feature report'."
                    )
                } else {
                    format!("HID write failed: {err}")
                };
                self.last_error = Some(message.clone());
                self.device = None;
                self.status = DeviceStatus::Disconnected;
                self.note(label, report, false, message.clone());
                Err(DriverError::Hid(message))
            }
        }
    }
}

impl Lk67Driver {
    /// Restore the stock keymap entries in every firmware profile slot.
    ///
    /// Used when moving from the onboard remap path to software remapping: a
    /// key written earlier would otherwise be remapped by the firmware and then
    /// again by the low-level hook. Reads first and only writes slots that do not
    /// match stock, but each write still needs the yc500 settle time.
    pub fn restore_stock_keymap(&mut self, layout: &Layout) -> Result<usize, DriverError> {
        let normal_default = layout.default_normal();
        let func_default = layout.default_func();
        let profiles = self.cfg.profile_slots.clamp(1, 4);
        let mut written = 0;
        for profile in 0..profiles {
            for func_layer in [false, true] {
                let slots = self.dump_keymap(profile, func_layer)?;
                let defaults = if func_layer {
                    &func_default
                } else {
                    &normal_default
                };
                for (index, default) in defaults.iter().enumerate() {
                    let slot = key_slot(index);
                    let want = action_entry(default);
                    if slots.get(slot as usize).copied() == Some(want) {
                        continue;
                    }
                    let report = build_keymap_report(&self.cfg, profile, func_layer, slot, default);
                    self.write_packet(
                        if func_layer {
                            "FN stock restore"
                        } else {
                            "Stock keymap restore"
                        },
                        report,
                    )?;
                    written += 1;
                    std::thread::sleep(std::time::Duration::from_millis(550));
                }
            }
        }
        if written > 0 {
            std::thread::sleep(std::time::Duration::from_millis(1200));
        }
        Ok(written)
    }
}

impl KeyboardDriver for Lk67Driver {
    fn status(&self) -> &DeviceStatus {
        &self.status
    }

    fn is_dry_run(&self) -> bool {
        self.dry_run
    }

    fn set_dry_run(&mut self, dry_run: bool) {
        if self.dry_run == dry_run {
            return;
        }
        self.dry_run = dry_run;
        if dry_run {
            self.device = None;
            self.opened_usage = None;
            self.opened_caps = None;
            self.status = DeviceStatus::Simulation;
            self.note("Mode", Vec::new(), true, "dry-run enabled");
        } else {
            self.status = DeviceStatus::Disconnected;
            self.rescan();
        }
    }

    fn rescan(&mut self) {
        self.device = None;
        self.opened_usage = None;
        self.opened_caps = None;
        self.last_error = None;
        if self.dry_run {
            self.status = DeviceStatus::Simulation;
            return;
        }
        if let Err(err) = self.ensure_api() {
            self.status = DeviceStatus::Disconnected;
            self.last_error = Some(err.to_string());
            return;
        }
        let count = self
            .api
            .as_ref()
            .map(|a| a.device_list().count())
            .unwrap_or(0);
        if self.try_open_configured() {
            self.note(
                "Scan",
                Vec::new(),
                true,
                format!(
                    "{count} HID devices, opened {:04X}:{:04X}",
                    self.cfg.vid, self.cfg.pid
                ),
            );
        } else {
            self.status = DeviceStatus::Disconnected;
            self.note(
                "Scan",
                Vec::new(),
                false,
                format!(
                    "{count} HID devices; no match for {:04X}:{:04X}",
                    self.cfg.vid, self.cfg.pid
                ),
            );
        }
    }

    fn send_report(&mut self, label: &str, report: &[u8]) -> Result<(), DriverError> {
        self.write_packet(label, report.to_vec())
    }

    fn apply_lighting(&mut self, lighting: &Lighting) -> Result<usize, DriverError> {
        // One LEDPARAM packet. Per-key pictures are flash-backed on this board,
        // so they are only written by `upload_custom_layout`, never on every
        // colour tweak or profile switch.
        let effect = build_effect_report(&self.cfg, lighting);
        self.write_packet("Lighting mode", effect)?;
        Ok(1)
    }

    fn upload_custom_layout(&mut self, lighting: &Lighting) -> Result<usize, DriverError> {
        // Measured on the yc500 family: pages need ~100 ms between them and the
        // board needs a settle before the mode switch, or the config endpoint
        // stalls until the keyboard is replugged.
        let mut sent = 0;
        for report in build_key_color_reports(&self.cfg, &lighting.custom.keys) {
            self.write_packet("Per-key picture page", report)?;
            sent += 1;
            if !self.dry_run {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        if !self.dry_run {
            std::thread::sleep(std::time::Duration::from_millis(2000));
        }
        self.write_packet(
            "Per-key picture mode",
            build_effect_report(&self.cfg, lighting),
        )?;
        Ok(sent + 1)
    }

    fn apply_keymap(
        &mut self,
        layout: &Layout,
        normal: &[Action],
        func: &[Action],
        profile: u8,
    ) -> Result<usize, DriverError> {
        let changed = changed_keys(layout, normal, func);
        if changed.is_empty() {
            return Ok(0);
        }
        // The LK67 accepts the vendor "simple" single-slot writes and needs a
        // settle delay after each. The active profile is selected by the app.
        let normal_default = layout.default_normal();
        let func_default = layout.default_func();
        let mut sent = 0;
        for (func_layer, index, action) in changed {
            // Macros, combos and app commands stay in software. Write the stock
            // entry so the physical key still reaches the low-level hook.
            let value = if hardware_action(&action) {
                action
            } else if func_layer {
                func_default[index].clone()
            } else {
                normal_default[index].clone()
            };
            let slot = key_slot(index);
            let report = build_keymap_report(&self.cfg, profile, func_layer, slot, &value);
            self.write_packet(
                if func_layer {
                    "FN keymap slot"
                } else {
                    "Keymap slot"
                },
                report,
            )?;
            sent += 1;
            if !self.dry_run {
                std::thread::sleep(std::time::Duration::from_millis(550));
            }
        }
        if !self.dry_run {
            std::thread::sleep(std::time::Duration::from_millis(1200));
        }
        Ok(sent)
    }

    fn stream_music_bands(&mut self, heights: &[u8; MUSIC_HEIGHTS]) -> Result<(), DriverError> {
        let report = build_music_report(&self.cfg, heights);
        self.write_packet("Music bands", report).map(|_| ())
    }

    fn stream_screen_color(&mut self, rgb: [u8; 3]) -> Result<(), DriverError> {
        let report = build_screen_color_report(&self.cfg, rgb);
        self.write_packet("Screen colour", report).map(|_| ())
    }

    fn select_profile(&mut self, slot: u8) -> Result<(), DriverError> {
        let report = build_profile_report(&self.cfg, slot);
        self.write_packet("Select profile slot", report)
    }

    fn poll(&mut self) -> Vec<Vec<u8>> {
        let mut reports = Vec::new();
        // Firmware input-report parsing is a placeholder: the LK67 may not even share
        // its key matrix over HID. When it does, decode it here.
        if let Some(device) = self.device.as_ref() {
            let mut buffer = [0u8; 64];
            for _ in 0..8 {
                match device.read_timeout(&mut buffer, 0) {
                    Ok(0) => break,
                    Ok(n) => reports.push(buffer[..n].to_vec()),
                    Err(_) => break,
                }
            }
        }
        reports
    }

    fn log(&self) -> &VecDeque<PacketLogEntry> {
        &self.log
    }

    fn clear_log(&mut self) {
        self.log.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lk67_collections() -> Vec<HidCandidate> {
        vec![
            HidCandidate {
                path: "kbd".into(),
                interface: 1,
                usage_page: 0x0001,
                usage: 0x0006,
                output_len: 0,
                feature_len: 0,
            },
            HidCandidate {
                path: "consumer".into(),
                interface: 1,
                usage_page: 0x000C,
                usage: 0x0001,
                output_len: 0,
                feature_len: 0,
            },
            HidCandidate {
                path: "vendor-1".into(),
                interface: 1,
                usage_page: 0xFFFF,
                usage: 0x0001,
                output_len: 0,
                feature_len: 0,
            },
            HidCandidate {
                path: "vendor-2".into(),
                interface: 2,
                usage_page: 0xFFFF,
                usage: 0x0002,
                output_len: 0,
                feature_len: 65,
            },
        ]
    }

    #[test]
    fn prefers_dedicated_vendor_collection() {
        let collections = lk67_collections();
        let mut cfg = DriverConfig::default();
        cfg.interface = 1;
        let chosen = choose_candidate(&collections, &cfg).expect("a candidate");
        assert_eq!(chosen.path, "vendor-2");
    }

    #[test]
    fn explicit_usage_selects_that_collection() {
        let mut collections = lk67_collections();
        if let Some(vendor) = collections.iter_mut().find(|c| c.path == "vendor-1") {
            vendor.feature_len = 65;
        }
        let mut cfg = DriverConfig::default();
        cfg.interface = 1;
        cfg.usage_page = 0xFFFF;
        cfg.usage = 0x0001;
        let chosen = choose_candidate(&collections, &cfg).expect("a candidate");
        assert_eq!(chosen.path, "vendor-1");
    }

    #[test]
    fn non_vendor_usage_falls_back_to_vendor() {
        let collections = lk67_collections();
        let mut cfg = DriverConfig::default();
        cfg.interface = 1;
        cfg.usage_page = 0x000C;
        cfg.usage = 0x0001;
        let chosen = choose_candidate(&collections, &cfg).expect("a candidate");
        assert_eq!(chosen.path, "vendor-2");
        assert!(chosen.is_vendor());
    }

    #[test]
    fn old_config_defaults_to_feature_reports() {
        let cfg: DriverConfig = serde_json::from_str(r#"{"vid":12625,"pid":16405,"interface":1}"#)
            .expect("legacy config");
        assert_eq!(cfg.transport, ReportTransport::FeatureReport);
    }

    #[test]
    fn input_only_vendor_usage_is_skipped() {
        let collections = lk67_collections();
        let mut cfg = DriverConfig::default();
        cfg.interface = 1;
        cfg.usage_page = 0xFFFF;
        cfg.usage = 0x0001;
        let chosen = choose_candidate(&collections, &cfg).expect("a candidate");
        assert_eq!(chosen.path, "vendor-2");
        assert!(chosen.feature_len > 0);
    }

    #[test]
    fn falls_back_to_first_collection() {
        let collections = vec![HidCandidate {
            path: "only".into(),
            interface: 0,
            usage_page: 0x0001,
            usage: 0x0006,
            output_len: 0,
            feature_len: 0,
        }];
        let chosen = choose_candidate(&collections, &DriverConfig::default()).expect("a candidate");
        assert_eq!(chosen.path, "only");
    }
}
