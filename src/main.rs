#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod driver;
mod input;
mod model;
mod rgb;
mod tray;
mod ui;

fn main() -> eframe::Result {
    if std::env::args().any(|arg| arg == "--probe") {
        probe();
        return Ok(());
    }
    if std::env::args().any(|arg| arg == "--knob-probe") {
        knob_probe();
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("LK67 Studio")
            .with_inner_size([1320.0, 880.0])
            .with_min_inner_size([1080.0, 720.0])
            .with_icon(icon()),
        ..Default::default()
    };
    eframe::run_native(
        "LK67 Studio",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}

/// Watch every consumer/vendor collection of the keyboard and log input reports
/// for five minutes. Used to identify what the rotary knob actually sends.
fn knob_probe() {
    use hidapi::HidApi;
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    let (cfg, _) = model::store::Config::load();
    let api = match HidApi::new() {
        Ok(api) => api,
        Err(err) => {
            eprintln!("HID init failed: {err}");
            return;
        }
    };
    let path = model::store::config_dir().join("knobprobe.log");
    let log = Arc::new(Mutex::new(std::fs::File::create(&path).ok()));
    let mut readers = 0;
    for info in api.device_list().filter(|info| {
        info.vendor_id() == cfg.driver.vid
            && info.product_id() == cfg.driver.pid
            && (info.usage_page() == 0x0C || info.usage_page() >= 0xFF00)
    }) {
        let label = format!(
            "if={} usage={:04X}/{:04X}",
            info.interface_number(),
            info.usage_page(),
            info.usage()
        );
        match api.open_path(info.path()) {
            Ok(device) => {
                let _ = device.set_blocking_mode(true);
                readers += 1;
                let log = log.clone();
                let label = label.clone();
                std::thread::spawn(move || {
                    let mut buf = [0u8; 128];
                    loop {
                        match device.read_timeout(&mut buf, 250) {
                            Ok(0) => {}
                            Ok(n) => {
                                let line =
                                    format!("{label}  {}", driver::lk67::hex_dump(&buf[..n]));
                                eprintln!("{line}");
                                if let Ok(mut guard) = log.lock()
                                    && let Some(file) = guard.as_mut()
                                {
                                    let _ = writeln!(file, "{line}");
                                    let _ = file.flush();
                                }
                            }
                            Err(err) => {
                                eprintln!("{label} read error: {err}");
                                break;
                            }
                        }
                    }
                });
            }
            Err(err) => eprintln!("{label} open failed: {err}"),
        }
    }
    eprintln!(
        "watching {readers} collection(s) for 300s; log: {}",
        path.display()
    );
    std::thread::sleep(std::time::Duration::from_secs(300));
    std::process::exit(0);
}

/// Open the configured HID device once and print which collection was selected.
fn probe() {
    use driver::KeyboardDriver;
    let (cfg, _) = model::store::Config::load();
    let mut driver = driver::Lk67Driver::new(cfg.driver.clone(), false);
    match driver.status() {
        driver::DeviceStatus::Connected {
            product,
            interface,
            path,
            ..
        } => {
            println!("connected: {product} (interface {interface})");
            println!("path: {path}");
        }
        other => println!("status: {}", other.label()),
    }
    if let Some(err) = driver.last_error() {
        println!("error: {err}");
    }
    match driver.probe_lighting() {
        Ok(reply) => {
            println!(
                "GET_LEDPARAM reply ({} bytes): {}",
                reply.len(),
                driver::lk67::hex_dump(&reply[..reply.len().min(16)])
            );
            if reply.first() == Some(&driver::lk67::GET_LEDPARAM) {
                println!(
                    "  mode={} speed={} brightness={} flags=0x{:02X} rgb={:02X}{:02X}{:02X}",
                    reply.get(1).copied().unwrap_or(0),
                    reply.get(2).copied().unwrap_or(0),
                    reply.get(3).copied().unwrap_or(0),
                    reply.get(4).copied().unwrap_or(0),
                    reply.get(5).copied().unwrap_or(0),
                    reply.get(6).copied().unwrap_or(0),
                    reply.get(7).copied().unwrap_or(0)
                );
            }
        }
        Err(err) => println!("GET_LEDPARAM failed: {err}"),
    }
    if std::env::args().any(|arg| arg == "--test-keymap") {
        eprintln!("waiting 300 ms before write");
        std::thread::sleep(std::time::Duration::from_millis(300));
        let probe_action = model::action::Action::Key {
            key: model::keycode::KeyCode(0x41), // A
        };
        let report = driver::lk67::build_keymap_report(&cfg.driver, 0, false, 1, &probe_action);
        eprintln!("writing slot 1 = A");
        match driver.send_report("Test keymap slot 1 = A", &report) {
            Ok(()) => eprintln!("wrote Esc slot = A"),
            Err(err) => eprintln!("slot write failed: {err}"),
        }
        std::thread::sleep(std::time::Duration::from_millis(1200));
        eprintln!("reading back");
        match driver.dump_keymap(0, false) {
            Ok(slots) => println!("Esc slot after write: {:02X?}", slots[1]),
            Err(err) => println!("keymap readback failed: {err}"),
        }
        let restore = driver::lk67::build_keymap_report(
            &cfg.driver,
            0,
            false,
            1,
            &model::action::Action::Key {
                key: model::keycode::KeyCode(0x1B),
            },
        );
        eprintln!("restoring Esc");
        match driver.send_report("Restore Esc", &restore) {
            Ok(()) => eprintln!("restore Esc sent"),
            Err(err) => eprintln!("restore failed: {err}"),
        }
        std::thread::sleep(std::time::Duration::from_millis(1200));
        match driver.dump_keymap(0, false) {
            Ok(slots) => eprintln!("Esc slot after restore: {:02X?}", slots[1]),
            Err(err) => eprintln!("restore readback failed: {err}"),
        }
    }
    if std::env::args().any(|arg| arg == "--test-apply-keymap") {
        use model::action::Action;
        use model::keycode::KeyCode;
        let mut normal = cfg.profile().normal.clone();
        let func = cfg.profile().func.clone();
        normal[0] = Action::Key {
            key: KeyCode(0x41), // A
        };
        eprintln!("app path: writing Esc = A");
        match driver.apply_keymap(model::layout::layout(), &normal, &func, 0) {
            Ok(count) => eprintln!("apply_keymap wrote {count} slot(s)"),
            Err(err) => eprintln!("apply_keymap failed: {err}"),
        }
        match driver.dump_keymap(0, false) {
            Ok(slots) => println!("Esc slot after apply_keymap: {:02X?}", slots[1]),
            Err(err) => println!("readback failed: {err}"),
        }
        eprintln!("app path: restoring Esc");
        let restore = model::action::Action::Key { key: KeyCode(0x1B) };
        let report = driver::lk67::build_keymap_report(&cfg.driver, 0, false, 1, &restore);
        match driver.send_report("Restore Esc", &report) {
            Ok(()) => eprintln!("restore sent"),
            Err(err) => eprintln!("restore failed: {err}"),
        }
        std::thread::sleep(std::time::Duration::from_millis(1200));
        match driver.dump_keymap(0, false) {
            Ok(slots) => println!("Esc slot after restore: {:02X?}", slots[1]),
            Err(err) => println!("restore readback failed: {err}"),
        }
    }
    if std::env::args().any(|arg| arg == "--test-lighting") {
        use model::lighting::LightingMode;
        let mut lighting = cfg.profile().lighting.clone();
        lighting.mode = LightingMode::Hardware;
        lighting.effect = 1;
        lighting.brightness = 100;
        lighting.colors[0] = [255, 0, 0];
        match driver.apply_lighting(&lighting) {
            Ok(count) => println!("wrote static red lighting ({count} packets)"),
            Err(err) => println!("lighting write failed: {err}"),
        }
        std::thread::sleep(std::time::Duration::from_millis(120));
        match driver.probe_lighting() {
            Ok(reply) => println!(
                "GET_LEDPARAM after write: {}",
                driver::lk67::hex_dump(&reply[..reply.len().min(16)])
            ),
            Err(err) => println!("readback failed: {err}"),
        }
    }
    if std::env::args().any(|arg| arg == "--test-picture") {
        use model::lighting::LightingMode;
        // Distinctive pattern: every key green, Esc red. If the board renders
        // it, mode 13 + the paced 0x0C upload are right for this firmware.
        let mut lighting = cfg.profile().lighting.clone();
        lighting.mode = LightingMode::Custom;
        lighting.brightness = 100;
        lighting.custom.keys = vec![[0, 120, 0]; model::layout::KEY_COUNT];
        lighting.custom.keys[0] = [255, 0, 0];
        eprintln!("uploading per-key picture (7 pages, paced)...");
        match driver.upload_custom_layout(&lighting) {
            Ok(count) => println!("picture sent ({count} packets)"),
            Err(err) => println!("picture upload failed: {err}"),
        }
        match driver.probe_lighting() {
            Ok(reply) => println!(
                "GET_LEDPARAM after picture: {}",
                driver::lk67::hex_dump(&reply[..reply.len().min(16)])
            ),
            Err(err) => println!("readback failed: {err}"),
        }
    }
    if std::env::args().any(|arg| arg == "--clear-picture") {
        use model::lighting::LightingMode;
        let mut lighting = cfg.profile().lighting.clone();
        lighting.mode = LightingMode::Custom;
        lighting.custom.keys = vec![[0, 0, 0]; model::layout::KEY_COUNT];
        match driver.upload_custom_layout(&lighting) {
            Ok(count) => println!("blank picture written ({count} packets)"),
            Err(err) => println!("picture clear failed: {err}"),
        }
        lighting.mode = LightingMode::Hardware;
        match driver.apply_lighting(&lighting) {
            Ok(_) => println!("hardware effect restored"),
            Err(err) => println!("effect restore failed: {err}"),
        }
    }
    if std::env::args().any(|arg| arg == "--test-music") {
        use model::lighting::LightingMode;
        let mut lighting = cfg.profile().lighting.clone();
        lighting.mode = LightingMode::Music;
        lighting.brightness = 100;
        match driver.apply_lighting(&lighting) {
            Ok(count) => println!("music mode written ({count} packets)"),
            Err(err) => println!("music mode failed: {err}"),
        }
        for step in 0..12u8 {
            let mut heights = [0u8; driver::lk67::MUSIC_HEIGHTS];
            for (index, height) in heights.iter_mut().enumerate() {
                let wave = ((index as f32 / 6.0) + step as f32 * 0.6).sin() * 0.5 + 0.5;
                *height = (wave * 6.0) as u8;
            }
            if let Err(err) = driver.stream_music_bands(&heights) {
                println!("band stream failed: {err}");
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(60));
        }
        println!("12 band frames sent");
        match driver.probe_lighting() {
            Ok(reply) => println!(
                "GET_LEDPARAM after music: {}",
                driver::lk67::hex_dump(&reply[..reply.len().min(16)])
            ),
            Err(err) => println!("readback failed: {err}"),
        }
    }
    if std::env::args().any(|arg| arg == "--test-screen") {
        use model::lighting::LightingMode;
        let mut lighting = cfg.profile().lighting.clone();
        lighting.mode = LightingMode::Screen;
        lighting.brightness = 100;
        match driver.apply_lighting(&lighting) {
            Ok(count) => println!("screen mode written ({count} packets)"),
            Err(err) => println!("screen mode failed: {err}"),
        }
        for step in 0..12u32 {
            let value = (step * 20).min(255) as u8;
            if let Err(err) = driver.stream_screen_color([value, 40, 255 - value]) {
                println!("screen colour stream failed: {err}");
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(60));
        }
        println!("12 screen colour frames sent");
        match driver.probe_lighting() {
            Ok(reply) => println!(
                "GET_LEDPARAM after screen: {}",
                driver::lk67::hex_dump(&reply[..reply.len().min(16)])
            ),
            Err(err) => println!("readback failed: {err}"),
        }
    }
    if std::env::args().any(|arg| arg == "--dump-keymap") {
        for func in [false, true] {
            match driver.dump_keymap(0, func) {
                Ok(slots) => {
                    println!("{} keymap slots:", if func { "Fn" } else { "Base" });
                    for (slot, entry) in slots.iter().enumerate() {
                        if entry.iter().any(|b| *b != 0) {
                            println!(
                                "  {slot:3}: {:02X} {:02X} {:02X} {:02X}",
                                entry[0], entry[1], entry[2], entry[3]
                            );
                        }
                    }
                }
                Err(err) => println!("keymap dump failed: {err}"),
            }
        }
    }
}

/// Window icon: the same pixels as the tray icon.
fn icon() -> egui::IconData {
    let (rgba, width, height) = tray::icon_rgba();
    egui::IconData {
        rgba,
        width,
        height,
    }
}
