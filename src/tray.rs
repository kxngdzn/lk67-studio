//! System tray icon: show the window, switch profile and quit.
//!
//! The icon itself is created on the main thread (Windows needs that thread's
//! event loop), while a small listener thread turns menu and tray events into
//! egui viewport commands. That way "Show" and "Quit" still work while the
//! window is hidden and no frame is being drawn.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};

use egui::{Context, ViewportCommand};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub struct Tray {
    /// Dropping this removes the icon; it must stay alive for the app's life.
    _icon: TrayIcon,
    profiles: Receiver<usize>,
}

impl Tray {
    /// Profile slots picked from the tray menu since the last frame.
    pub fn drain_profiles(&self) -> Vec<usize> {
        self.profiles.try_iter().collect()
    }
}

pub fn build(ctx: &Context, quitting: Arc<AtomicBool>) -> Option<Tray> {
    let (rgba, width, height) = icon_rgba();
    let icon = Icon::from_rgba(rgba, width, height).ok()?;

    let menu = Menu::new();
    let show = MenuItem::new("Show LK67 Studio", true, None);
    let quit = MenuItem::new("Quit", true, None);
    let mut profile_items: Vec<MenuItem> = (1..=4)
        .map(|slot| MenuItem::new(format!("Profile {slot}"), true, None))
        .collect();
    menu.append(&show).ok()?;
    menu.append(&PredefinedMenuItem::separator()).ok()?;
    for item in &profile_items {
        menu.append(item).ok()?;
    }
    menu.append(&PredefinedMenuItem::separator()).ok()?;
    menu.append(&quit).ok()?;

    let tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("LK67 Studio")
        .with_icon(icon)
        .build()
        .ok()?;

    let show_id = show.id().clone();
    let quit_id = quit.id().clone();
    let profile_ids: Vec<_> = profile_items
        .iter_mut()
        .map(|item| item.id().clone())
        .collect();
    let (tx, rx) = channel();
    let ctx = ctx.clone();
    std::thread::Builder::new()
        .name("lk67-tray".into())
        .spawn(move || {
            loop {
                while let Ok(event) = MenuEvent::receiver().try_recv() {
                    if event.id == show_id {
                        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                        ctx.send_viewport_cmd(ViewportCommand::Focus);
                    } else if event.id == quit_id {
                        quitting.store(true, Ordering::Relaxed);
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    } else if let Some(slot) = profile_ids.iter().position(|id| *id == event.id) {
                        let _ = tx.send(slot);
                        ctx.request_repaint();
                    }
                }
                while let Ok(event) = TrayIconEvent::receiver().try_recv() {
                    if let TrayIconEvent::DoubleClick {
                        button: MouseButton::Left,
                        ..
                    } = event
                    {
                        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                        ctx.send_viewport_cmd(ViewportCommand::Focus);
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(120));
            }
        })
        .ok()?;

    Some(Tray {
        _icon: tray_icon,
        profiles: rx,
    })
}

/// Window and tray icon pixels: a dark keycap with an RGB accent bar.
pub fn icon_rgba() -> (Vec<u8>, u32, u32) {
    let size = 32usize;
    let mut rgba = vec![0u8; size * size * 4];
    let accent = [52u8, 206, 170];
    for y in 0..size {
        for x in 0..size {
            let index = (y * size + x) * 4;
            let border = x < 2 || y < 2 || x >= size - 2 || y >= size - 2;
            let (r, g, b) = if border {
                (8, 12, 18)
            } else if (22..26).contains(&y) {
                (accent[0], accent[1], accent[2])
            } else if (8..20).contains(&y) && (6..26).contains(&x) {
                (32, 40, 54)
            } else {
                (22, 28, 40)
            };
            rgba[index] = r;
            rgba[index + 1] = g;
            rgba[index + 2] = b;
            rgba[index + 3] = 255;
        }
    }
    (rgba, size as u32, size as u32)
}
