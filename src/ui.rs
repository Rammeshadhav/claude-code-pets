//! The floating pet window.
//!
//! thought bubble:  ... = working   red clock = needs your approval/input
//!                  green check = finished   zzz = idle
//! left-drag: move   left-click: acknowledge "done"   right-click: menu
//!
//! Efficiency: session files are watched with inotify (gio::FileMonitor), so
//! state changes show up instantly without polling; text is rendered once into
//! cached surfaces; the frame rate adapts to the state.

use crate::custom::{self, Meta};
use crate::pets::{self, all_species, PX};
use crate::state::{self, now, pet_dir, read_sessions, sess_dir, status_phrase, Config, Session};
use crate::usage::{self, Window};
use gtk::cairo::{self, Context, Format, ImageSurface, Operator};
use gtk::gio::prelude::*;
use gtk::gdk::prelude::GdkContextExt;
use gtk::gdk_pixbuf::Pixbuf;
use gtk::prelude::*;
use gtk::{gdk, gio, glib, pango};
use std::cell::RefCell;
use std::f64::consts::{PI, TAU};
use std::fs::{File, OpenOptions};
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

const W: f64 = 190.0; // drawing canvas; the window is this times the scale
const H: f64 = 180.0;
const DEFAULT_SCALE: f64 = 0.75;
const SIZES: [(&str, f64); 3] = [("Small", 0.6), ("Medium", 0.75), ("Large", 1.0)];
const TICKS_PER_SEC: f64 = 1000.0 / 120.0; // animation speeds are tuned in these ticks
const FALLBACK_POLL_S: u32 = 5; // catches closed terminals; inotify handles the rest
const FONT: &str = "Ubuntu Sans, Lato, Open Sans, Noto Sans, Sans";

// session list shown under the pet when several Claude sessions are open
const LIST_W: f64 = 270.0;
const LIST_TOP: f64 = H - 26.0;
const ROW_H: f64 = 21.0;
const LIST_PAD: f64 = 6.0;
const MAX_ROWS: usize = 6;

// usage box: plan limits recorded by `claude-pet statusline`
const USAGE_ROW: f64 = 19.0;
const USAGE_PAD: f64 = 6.0;

/// What the usage meter is called, and its colour, per pet.
fn usage_theme(species: &str) -> (&'static str, Rgb) {
    match species {
        "crimson" => ("Chakra", (0.95, 0.25, 0.25)),
        "ripple" => ("Chakra", (0.68, 0.5, 0.98)),
        "spiral" => ("Chakra", (0.35, 0.68, 1.0)),
        "flash" => ("Chakra", (1.0, 0.85, 0.3)),
        "lavender" => ("Chakra", (0.78, 0.68, 0.98)),
        "miti" => ("Battery", (0.36, 0.72, 1.0)),
        "cat" => ("Energy", (0.98, 0.7, 0.36)),
        "crab" => ("Energy", (0.95, 0.5, 0.4)),
        "ghost" => ("Energy", (0.82, 0.78, 0.98)),
        _ => ("Energy", (0.55, 0.88, 0.45)),
    }
}

fn usage_color(accent: Rgb, left: f64) -> Rgb {
    if left < 15.0 {
        (1.0, 0.32, 0.32)
    } else if left < 40.0 {
        (1.0, 0.75, 0.25)
    } else {
        accent
    }
}

type Rgb = (f64, f64, f64);

/// (status dot / accent colour, text colour)
fn state_colors(state: &str) -> (Rgb, Rgb) {
    match state {
        "working" => ((0.36, 0.62, 1.00), (0.62, 0.78, 1.00)),
        "waiting" => ((1.00, 0.33, 0.33), (1.00, 0.62, 0.60)),
        "done" => ((0.25, 0.82, 0.45), (0.58, 0.92, 0.66)),
        _ => ((0.62, 0.62, 0.70), (0.78, 0.78, 0.84)),
    }
}

fn hex(rgb: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", (rgb.0 * 255.0) as u8, (rgb.1 * 255.0) as u8, (rgb.2 * 255.0) as u8)
}

fn esc(s: &str) -> String {
    glib::markup_escape_text(s).to_string()
}

fn layout(cr: &Context, markup: &str, size: f64, weight: pango::Weight) -> pango::Layout {
    let lay = pangocairo::functions::create_layout(cr);
    let mut desc = pango::FontDescription::from_string(FONT);
    desc.set_absolute_size(size * pango::SCALE as f64);
    desc.set_weight(weight);
    lay.set_font_description(Some(&desc));
    lay.set_markup(markup);
    lay
}

fn extents(lay: &pango::Layout) -> (f64, f64) {
    let (_, logical) = lay.pixel_extents();
    (logical.width() as f64, logical.height() as f64)
}

fn show(cr: &Context, lay: &pango::Layout) {
    pangocairo::functions::show_layout(cr, lay);
}

fn rounded_rect(cr: &Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -PI / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, PI / 2.0);
    cr.arc(x + r, y + h - r, r, PI / 2.0, PI);
    cr.arc(x + r, y + r, r, PI, 3.0 * PI / 2.0);
    cr.close_path();
}

/// A clickable session row, in canvas coordinates.
#[derive(Clone)]
struct Row {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    path: PathBuf,
}

/// Pre-rendered text, re-used until `key` changes.
struct Layer {
    key: String,
    surf: ImageSurface,
    oy: f64,
    px: f64,        // label: pill x position
    rows: Vec<Row>, // list: row rectangles
}

struct Pet {
    window: gtk::Window,
    area: gtk::DrawingArea,
    cfg: Config,
    species: String,
    scale: f64,
    t0: Instant,
    t: f64,
    next_blink: f64,
    blink_until: f64,
    poll_pending: bool,
    save_pending: bool,
    sessions: Vec<Session>,
    state: String,
    detail: String,
    name: String,
    count: usize,
    list_mode: bool,
    rows: Vec<Row>,
    cw: f64, // canvas size (unscaled)
    ch: f64,
    last_active: f64,
    press: Option<(f64, f64)>,
    label_layer: Option<Layer>,
    list_layer: Option<Layer>,
    usage_layer: Option<Layer>,
    /// the loaded image pet, if the species is one
    custom: Option<(String, Pixbuf, Meta)>,
    usage: Vec<Window>,
    usage_top: f64,
    menu: Option<gtk::Menu>,
    _monitor: Option<gio::FileMonitor>,
}

type Shared = Rc<RefCell<Pet>>;

fn debug() -> bool {
    std::env::var_os("CLAUDE_PET_DEBUG").is_some()
}

/// Hold the single-instance lock for the life of the process.
fn acquire_lock() -> Option<File> {
    let f = OpenOptions::new().create(true).truncate(false).write(true).open(pet_dir().join("pet.lock")).ok()?;
    // SAFETY: flock on a file descriptor we own.
    (unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0).then_some(f)
}

pub fn run() -> i32 {
    // Wayland/GNOME ignores keep-above and window positioning for native
    // clients; XWayland honors both.
    if std::env::var_os("DISPLAY").is_some() && std::env::var_os("CLAUDE_PET_NATIVE").is_none() {
        std::env::set_var("GDK_BACKEND", "x11");
    }
    let _ = std::fs::create_dir_all(sess_dir());
    let Some(_lock) = acquire_lock() else {
        eprintln!("pet is already running");
        return 0;
    };
    let _ = std::fs::write(pet_dir().join("pet.pid"), std::process::id().to_string());
    if gtk::init().is_err() {
        eprintln!("cannot open a display");
        return 1;
    }
    let pet = build();
    poll(&pet);
    schedule_frame(&pet);
    gtk::main();
    0
}

fn build() -> Shared {
    let cfg = Config::load();
    let species = cfg
        .species
        .clone()
        .filter(|s| all_species().contains(s))
        .unwrap_or_else(|| "blob".into());
    let scale = cfg.scale.filter(|s| (0.2..=4.0).contains(s)).unwrap_or(DEFAULT_SCALE);

    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("Claude Pet");
    window.set_decorated(false);
    window.set_resizable(false);
    window.set_keep_above(true);
    window.set_skip_taskbar_hint(true);
    window.set_skip_pager_hint(true);
    window.set_accept_focus(false);
    window.set_type_hint(gdk::WindowTypeHint::Utility);
    window.stick();
    window.set_app_paintable(true);
    if let Some(visual) = WidgetExt::screen(&window).and_then(|s| s.rgba_visual()) {
        window.set_visual(Some(&visual));
    }
    let area = gtk::DrawingArea::new();
    window.add(&area);

    let pet = Rc::new(RefCell::new(Pet {
        window: window.clone(),
        area: area.clone(),
        cfg,
        species,
        scale,
        t0: Instant::now(),
        t: 0.0,
        next_blink: 37.0,
        blink_until: 0.0,
        poll_pending: false,
        save_pending: false,
        sessions: Vec::new(),
        state: "idle".into(),
        detail: String::new(),
        name: String::new(),
        count: 0,
        list_mode: false,
        rows: Vec::new(),
        cw: W,
        ch: H,
        last_active: now(),
        press: None,
        label_layer: None,
        list_layer: None,
        usage_layer: None,
        custom: None,
        usage: Vec::new(),
        usage_top: H,
        menu: None,
        _monitor: None,
    }));
    pet.borrow().apply_size();

    {
        let p = pet.clone();
        area.connect_draw(move |_, cr| {
            if let Ok(mut s) = p.try_borrow_mut() {
                s.draw(cr);
            }
            glib::Propagation::Stop
        });
    }

    window.add_events(
        gdk::EventMask::BUTTON_PRESS_MASK | gdk::EventMask::BUTTON_RELEASE_MASK | gdk::EventMask::POINTER_MOTION_MASK,
    );
    {
        let p = pet.clone();
        window.connect_button_press_event(move |_, ev| match ev.button() {
            3 => {
                show_menu(&p, ev);
                glib::Propagation::Stop
            }
            1 => {
                p.borrow_mut().press = Some(ev.root());
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        });
    }
    {
        let p = pet.clone();
        window.connect_motion_notify_event(move |w, ev| {
            let start = p.borrow().press;
            if let Some((x0, y0)) = start {
                if ev.state().contains(gdk::ModifierType::BUTTON1_MASK) {
                    let (x, y) = ev.root();
                    if (x - x0).abs() + (y - y0).abs() > 4.0 {
                        p.borrow_mut().press = None;
                        w.begin_move_drag(1, x as i32, y as i32, ev.time());
                    }
                }
            }
            glib::Propagation::Proceed
        });
    }
    {
        let p = pet.clone();
        window.connect_button_release_event(move |_, ev| {
            let pressed = p.borrow_mut().press.take();
            if ev.button() == 1 && pressed.is_some() {
                let (x, y) = ev.position();
                let row = {
                    let s = p.borrow();
                    let (x, y) = (x / s.scale, y / s.scale);
                    s.rows
                        .iter()
                        .find(|r| x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h)
                        .map(|r| r.path.clone())
                };
                state::acknowledge(row.as_deref());
                poll(&p);
            }
            glib::Propagation::Proceed
        });
    }
    {
        let p = pet.clone();
        window.connect_configure_event(move |_, ev| {
            let (x, y) = ev.position();
            let pos = [x as f64, y as f64];
            let mut s = p.borrow_mut();
            if s.cfg.pos != Some(pos) {
                s.cfg.pos = Some(pos);
                if !s.save_pending {
                    s.save_pending = true;
                    let p2 = p.clone();
                    glib::timeout_add_local_once(Duration::from_millis(800), move || {
                        let mut s = p2.borrow_mut();
                        s.save_pending = false;
                        s.cfg.save();
                    });
                }
            }
            false
        });
    }
    window.connect_destroy(|_| gtk::main_quit());

    // position: remembered, else bottom-right of the work area
    let pos = pet.borrow().cfg.pos;
    if let Some([x, y]) = pos {
        window.move_(x as i32, y as i32);
    } else if let Some(mon) = gdk::Display::default().and_then(|d| d.monitor(0)) {
        let g = mon.workarea();
        let (w, h) = pet.borrow().win_size();
        window.move_(g.x() + g.width() - w - 24, g.y() + g.height() - h - 24);
    }

    // react to hook writes immediately instead of polling
    let monitor = gio::File::for_path(sess_dir())
        .monitor_directory(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE)
        .ok();
    if let Some(m) = &monitor {
        let p = pet.clone();
        m.connect_changed(move |_, _, _, _| {
            // a hook write fires several events; coalesce them into one read
            let schedule = {
                let mut s = p.borrow_mut();
                !std::mem::replace(&mut s.poll_pending, true)
            };
            if schedule {
                let p2 = p.clone();
                glib::idle_add_local_once(move || poll(&p2));
            }
        });
    }
    pet.borrow_mut()._monitor = monitor;
    {
        let p = pet.clone();
        glib::timeout_add_seconds_local(FALLBACK_POLL_S, move || {
            poll(&p);
            glib::ControlFlow::Continue
        });
    }

    window.show_all();
    pet
}

fn poll(p: &Shared) {
    let sessions = read_sessions();
    let mut s = p.borrow_mut();
    s.poll_pending = false;
    s.count = sessions.iter().filter(|x| matches!(x.state.as_str(), "working" | "waiting")).count();
    match sessions.first() {
        Some(top) => {
            s.state = top.state.clone();
            s.detail = top.detail.clone();
            s.name = top.display_name().to_string();
        }
        None => {
            s.state = "idle".into();
            s.detail.clear();
            s.name.clear();
        }
    }
    s.sessions = sessions;
    s.usage = usage::read();
    if s.state != "idle" {
        s.last_active = now();
    }
    s.update_layout();
    if debug() {
        eprintln!("{:.3} poll: {} {} ({} sessions)", now(), s.state, s.detail, s.sessions.len());
    }
    s.area.queue_draw();
}

fn schedule_frame(p: &Shared) {
    let fps: u64 = {
        let s = p.borrow();
        if s.asleep() {
            2
        } else if matches!(s.state.as_str(), "working" | "waiting") {
            24
        } else {
            12
        }
    };
    let p2 = p.clone();
    glib::timeout_add_local_once(Duration::from_millis(1000 / fps), move || {
        {
            // time-based animation: speed is the same whatever the frame rate
            let mut s = p2.borrow_mut();
            s.t = s.t0.elapsed().as_secs_f64() * TICKS_PER_SEC;
            if s.t >= s.next_blink {
                s.next_blink = s.t + 37.0;
                if s.state != "working" {
                    s.blink_until = s.t + 2.0;
                }
            }
            s.area.queue_draw();
        }
        schedule_frame(&p2);
    });
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

fn show_menu(p: &Shared, ev: &gdk::EventButton) {
    let (species, scale, show_list) = {
        let s = p.borrow();
        (s.species.clone(), s.scale, s.cfg.show_list.unwrap_or(true))
    };
    let menu = gtk::Menu::new();
    for name in all_species() {
        let item = gtk::CheckMenuItem::with_label(&capitalize(&name));
        item.set_draw_as_radio(true);
        item.set_active(name == species);
        let p = p.clone();
        item.connect_activate(move |_| {
            let mut s = p.borrow_mut();
            s.species = name.clone();
            s.cfg.species = Some(name.clone());
            s.cfg.save();
            s.area.queue_draw();
        });
        menu.append(&item);
    }
    menu.append(&gtk::SeparatorMenuItem::new());

    let size = gtk::MenuItem::with_label("Size");
    let sub = gtk::Menu::new();
    for (label, value) in SIZES {
        let item = gtk::CheckMenuItem::with_label(label);
        item.set_draw_as_radio(true);
        item.set_active((value - scale).abs() < 0.01);
        let p = p.clone();
        item.connect_activate(move |_| {
            let mut s = p.borrow_mut();
            s.scale = value;
            s.cfg.scale = Some(value);
            s.cfg.save();
            s.label_layer = None;
            s.list_layer = None;
            s.usage_layer = None;
            s.apply_size();
        });
        sub.append(&item);
    }
    size.set_submenu(Some(&sub));
    menu.append(&size);

    let show = gtk::CheckMenuItem::with_label("Show all sessions");
    show.set_active(show_list);
    {
        let p = p.clone();
        show.connect_toggled(move |item| {
            let mut s = p.borrow_mut();
            s.cfg.show_list = Some(item.is_active());
            s.cfg.save();
            s.update_layout();
        });
    }
    menu.append(&show);

    let usage_item = gtk::CheckMenuItem::with_label("Show usage limits");
    usage_item.set_active(p.borrow().show_usage());
    {
        let p = p.clone();
        usage_item.connect_toggled(move |item| {
            let mut s = p.borrow_mut();
            s.cfg.show_usage = Some(item.is_active());
            s.cfg.save();
            s.update_layout();
        });
    }
    menu.append(&usage_item);

    let clear = gtk::MenuItem::with_label("Clear finished");
    {
        let p = p.clone();
        clear.connect_activate(move |_| {
            state::acknowledge(None);
            poll(&p);
        });
    }
    menu.append(&clear);
    let quit = gtk::MenuItem::with_label("Put pet away");
    quit.connect_activate(|_| gtk::main_quit());
    menu.append(&quit);

    menu.show_all();
    menu.popup_at_pointer(Some(&**ev));
    p.borrow_mut().menu = Some(menu); // keep it alive while shown
}

impl Pet {
    fn asleep(&self) -> bool {
        self.state == "idle" && now() - self.last_active > state::SLEEP_AFTER
    }

    fn win_size(&self) -> (i32, i32) {
        ((self.cw * self.scale).round() as i32, (self.ch * self.scale).round() as i32)
    }

    /// Resize the window to the canvas, keeping the pet where it was and on screen.
    fn apply_size(&self) {
        let (w, h) = self.win_size();
        if let (Some(gw), true) = (self.window.window(), self.window.is_visible()) {
            let (mut x, mut y) = self.window.position();
            let (old_w, _) = self.window.size();
            x += (old_w - w) / 2; // keep the pet horizontally in place
            if let Some(mon) = gdk::Display::default().and_then(|d| d.monitor_at_window(&gw)) {
                let g = mon.workarea();
                x = x.min(g.x() + g.width() - w).max(g.x());
                y = y.min(g.y() + g.height() - h).max(g.y());
            }
            self.window.move_(x, y);
        }
        self.area.set_size_request(w, h);
        self.window.resize(w, h);
    }

    fn show_usage(&self) -> bool {
        self.cfg.show_usage.unwrap_or(true)
    }

    fn update_layout(&mut self) {
        self.list_mode = self.cfg.show_list.unwrap_or(true) && self.sessions.len() >= 2;
        let mut size = if self.list_mode {
            let rows = self.sessions.len().min(MAX_ROWS) as f64;
            let more = if self.sessions.len() > MAX_ROWS { 16.0 } else { 0.0 };
            (LIST_W, LIST_TOP + LIST_PAD * 2.0 + rows * ROW_H + more + 4.0)
        } else {
            (W, H)
        };
        if self.show_usage() {
            self.usage_top = size.1 + 2.0;
            let rows = self.usage.len().max(1) as f64; // one hint row until data arrives
            size = (LIST_W, self.usage_top + USAGE_PAD * 2.0 + rows * USAGE_ROW + 2.0);
        }
        if size != (self.cw, self.ch) {
            (self.cw, self.ch) = size;
            self.apply_size();
        }
    }

    // ---- drawing -----------------------------------------------------------

    fn draw(&mut self, cr: &Context) {
        cr.set_operator(Operator::Clear);
        let _ = cr.paint();
        cr.set_operator(Operator::Over);
        cr.scale(self.scale, self.scale);
        let _ = cr.save();
        cr.translate((self.cw - W) / 2.0, 0.0); // pet stays centred when the list widens the window
        self.draw_pet(cr);
        let _ = cr.restore();
        if self.list_mode {
            self.draw_list(cr, LIST_TOP);
        } else {
            self.rows.clear();
        }
        if self.show_usage() {
            self.draw_usage(cr, self.usage_top);
        }
    }

    fn draw_pet(&mut self, cr: &Context) {
        let asleep = self.asleep();
        let t = self.t;
        let bob = match self.state.as_str() {
            "working" => -(t * 0.45).sin().abs() * 6.0,
            "waiting" => -(t * 0.25).sin().abs() * 14.0,
            "done" => -(t * 0.3).sin().abs() * 4.0,
            _ => (t * 0.08).sin() * 2.0,
        };

        if self.custom_pet().is_some() {
            self.draw_custom(cr, bob, asleep);
        } else if pets::is_vector(&self.species) {
            let r = 44.0;
            let (cx, cy) = (W / 2.0 - 14.0, H - r - 36.0 + bob * 0.5);
            shadow(cr, cx, H - 28.0, r * 0.85, 0.22);
            pets::draw_vector(&self.species, cr, cx, cy, r, t, &self.state, asleep);
            self.draw_bubble(cr, cx + r * 0.8, cy - r * 0.85, asleep);
        } else {
            let sprite = pets::sprite(&self.species);
            let (sw, sh) = (sprite[0].len() as f64 * PX, sprite.len() as f64 * PX);
            let (ox, oy) = ((W - sw) / 2.0 - 18.0, H - sh - 34.0 + bob);
            shadow(cr, ox + sw / 2.0, H - 30.0, sw / 2.4 * (1.0 + bob / 60.0), 0.25);
            pets::draw_sprite(cr, &self.species, ox, oy, asleep || t < self.blink_until);
            self.draw_bubble(cr, ox + sw - 6.0, oy - 6.0, asleep);
        }
        if !self.list_mode {
            self.draw_label(cr);
        }
    }

    fn draw_bubble(&self, cr: &Context, x: f64, y: f64, asleep: bool) {
        let r = 20.0;
        let (cx, cy) = ((x + r).min(W - r - 4.0), (y - r).max(r + 4.0));
        let t = self.t;
        // tail puffs
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.95);
        cr.arc(x + 2.0, y + 2.0, 3.0, 0.0, TAU);
        let _ = cr.fill();
        cr.arc(x + 8.0, y - 6.0, 5.0, 0.0, TAU);
        let _ = cr.fill();
        // bubble
        cr.arc(cx, cy, r, 0.0, TAU);
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.97);
        let _ = cr.fill_preserve();
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.25);
        cr.set_line_width(1.5);
        let _ = cr.stroke();

        match self.state.as_str() {
            "working" => {
                for i in 0..3 {
                    let lift = (t * 0.35 - i as f64 * 0.8).sin().max(0.0) * 4.0;
                    cr.arc(cx - 9.0 + i as f64 * 9.0, cy - lift, 3.2, 0.0, TAU);
                    cr.set_source_rgb(0.35, 0.35, 0.40);
                    let _ = cr.fill();
                }
            }
            "waiting" => {
                let pulse = 0.85 + 0.15 * (t * 0.5).sin();
                cr.arc(cx, cy, 12.0 * pulse, 0.0, TAU);
                cr.set_source_rgb(0.90, 0.22, 0.22);
                let _ = cr.fill();
                cr.set_source_rgb(1.0, 1.0, 1.0);
                cr.set_line_width(2.2);
                cr.set_line_cap(cairo::LineCap::Round);
                cr.move_to(cx, cy);
                cr.line_to(cx, cy - 7.0);
                cr.move_to(cx, cy);
                cr.line_to(cx + 5.0, cy + 2.0);
                let _ = cr.stroke();
            }
            "done" => {
                cr.arc(cx, cy, 12.0, 0.0, TAU);
                cr.set_source_rgb(0.20, 0.72, 0.35);
                let _ = cr.fill();
                cr.set_source_rgb(1.0, 1.0, 1.0);
                cr.set_line_width(2.8);
                cr.set_line_cap(cairo::LineCap::Round);
                cr.set_line_join(cairo::LineJoin::Round);
                cr.move_to(cx - 6.0, cy);
                cr.line_to(cx - 2.0, cy + 5.0);
                cr.line_to(cx + 6.0, cy - 5.0);
                let _ = cr.stroke();
            }
            _ if asleep => {
                cr.set_source_rgb(0.40, 0.40, 0.55);
                for (i, size) in [9.0, 12.0, 15.0].into_iter().enumerate() {
                    let lay = layout(cr, "z", size, pango::Weight::Heavy);
                    let dy = (t * 0.1 + i as f64).sin() * 1.5;
                    cr.move_to(cx - 13.0 + i as f64 * 7.0, cy - 4.0 - i as f64 * 6.0 + dy);
                    show(cr, &lay);
                }
            }
            _ => {
                // little music note, drawn so it never depends on fonts
                cr.set_source_rgb(0.40, 0.40, 0.55);
                let dy = (t * 0.12).sin() * 2.0;
                let _ = cr.save();
                cr.translate(cx - 2.0, cy + dy);
                let _ = cr.save();
                cr.translate(-3.0, 6.0);
                cr.scale(1.3, 1.0);
                cr.arc(0.0, 0.0, 3.4, 0.0, TAU);
                let _ = cr.restore();
                let _ = cr.fill();
                cr.rectangle(0.8, -8.0, 1.8, 14.0);
                let _ = cr.fill();
                cr.move_to(2.6, -8.0);
                cr.curve_to(6.0, -6.0, 8.0, -3.0, 6.0, 1.0);
                cr.set_line_width(1.8);
                let _ = cr.stroke();
                let _ = cr.restore();
            }
        }

        if self.count > 1 {
            let (bx, by) = (cx + r - 4.0, cy - r + 4.0);
            cr.arc(bx, by, 8.0, 0.0, TAU);
            cr.set_source_rgb(0.25, 0.45, 0.95);
            let _ = cr.fill();
            cr.set_source_rgb(1.0, 1.0, 1.0);
            let lay = layout(cr, &self.count.to_string(), 10.0, pango::Weight::Bold);
            let (lw, lh) = extents(&lay);
            cr.move_to(bx - lw / 2.0, by - lh / 2.0);
            show(cr, &lay);
        }
    }

    // ---- cached text layers ------------------------------------------------
    // Pango layout is the costly part of a frame, so text is rendered once into
    // an offscreen surface and re-used until the text itself changes. Only the
    // cheap animated shapes (status dots, glow) are drawn every frame.

    fn render_layer(&self, key: String, top: f64, height: f64, render: impl FnOnce(&Context) -> (f64, Vec<Row>)) -> Option<Layer> {
        let s = self.scale;
        let oy = (top * s).round();
        let surf = ImageSurface::create(
            Format::ARgb32,
            ((self.cw * s) as i32 + 1).max(1),
            ((height * s) as i32 + 2).max(1),
        )
        .ok()?;
        let (px, rows) = {
            let c = Context::new(&surf).ok()?;
            c.translate(0.0, -oy);
            c.scale(s, s);
            render(&c)
        };
        surf.flush();
        Some(Layer { key, surf, oy, px, rows })
    }

    fn blit(cr: &Context, layer: &Layer) {
        let _ = cr.save();
        cr.identity_matrix(); // device pixels: keeps text crisp
        let _ = cr.set_source_surface(&layer.surf, 0.0, layer.oy);
        let _ = cr.paint();
        let _ = cr.restore();
    }

    fn status_dot(&self, cr: &Context, x: f64, y: f64, state: &str, r: f64) {
        let (dot, _) = state_colors(state);
        if matches!(state, "working" | "waiting") {
            let rate = if state == "waiting" { 0.5 } else { 0.25 };
            let halo = 0.5 + 0.5 * (self.t * rate).sin();
            cr.arc(x, y, r + 2.4 * halo, 0.0, TAU);
            cr.set_source_rgba(dot.0, dot.1, dot.2, 0.35 * (1.0 - halo) + 0.1);
            let _ = cr.fill();
        }
        cr.arc(x, y, r, 0.0, TAU);
        cr.set_source_rgb(dot.0, dot.1, dot.2);
        let _ = cr.fill();
    }

    /// Glassy pill: pulsing status dot, session name, coloured status phrase.
    fn draw_label(&mut self, cr: &Context) {
        if self.state == "idle" && self.name.is_empty() {
            return;
        }
        let asleep = self.asleep();
        let phrase = status_phrase(&self.state, &self.detail, asleep, Some(self.t));
        let widest = status_phrase(&self.state, &self.detail, asleep, None);
        let (ph, py, ox) = (22.0, H - 22.0 - 3.0, (self.cw - W) / 2.0);
        let key = format!("{}|{}|{}|{}|{}", self.scale, self.name, phrase, widest, self.state);

        if self.label_layer.as_ref().map_or(true, |l| l.key != key) {
            let (_, text_rgb) = state_colors(&self.state);
            let name = if self.name.is_empty() { "claude" } else { &self.name };
            self.label_layer = self.render_layer(key, py - 2.0, ph + 6.0, |c| {
                let max_text = W - 38.0;
                let status = layout(
                    c,
                    &format!(
                        "<span foreground='#ffffff' fgalpha='40%'>  ·  </span><span weight='medium' foreground='{}'>{}</span>",
                        hex(text_rgb),
                        esc(&phrase)
                    ),
                    11.5,
                    pango::Weight::Normal,
                );
                let (status_w, _) = extents(&layout(c, &format!("  ·  {}", esc(&widest)), 11.5, pango::Weight::Medium));
                let name_lay = layout(c, &format!("<span weight='bold' foreground='#ffffff'>{}</span>", esc(name)), 11.5, pango::Weight::Normal);
                name_lay.set_ellipsize(pango::EllipsizeMode::End);
                name_lay.set_width(((max_text - status_w).max(30.0) * pango::SCALE as f64) as i32);
                let (name_w, text_h) = extents(&name_lay);
                let pw = name_w + status_w + 30.0;
                let px = ox + (W - pw) / 2.0;
                rounded_rect(c, px, py + 1.5, pw, ph, ph / 2.0);
                c.set_source_rgba(0.0, 0.0, 0.0, 0.25);
                let _ = c.fill();
                rounded_rect(c, px, py, pw, ph, ph / 2.0);
                c.set_source_rgba(0.09, 0.09, 0.12, 0.88);
                let _ = c.fill_preserve();
                c.set_source_rgba(1.0, 1.0, 1.0, 0.12);
                c.set_line_width(1.0);
                let _ = c.stroke();
                let ty = py + (ph - text_h) / 2.0;
                c.move_to(px + 21.0, ty);
                show(c, &name_lay);
                c.move_to(px + 21.0 + name_w, ty);
                show(c, &status);
                (px, Vec::new())
            });
        }
        if let Some(layer) = &self.label_layer {
            Self::blit(cr, layer);
            // draw_pet is translated by ox; the layer is in window coordinates
            self.status_dot(cr, layer.px + 12.0 - ox, py + ph / 2.0, &self.state, 3.5);
        }
    }

    /// One row per Claude session: status dot, session name, folder, status.
    fn draw_list(&mut self, cr: &Context, top: f64) {
        let rows: Vec<Session> = self.sessions.iter().take(MAX_ROWS).cloned().collect();
        let extra = self.sessions.len() - rows.len();
        let (x0, w) = (6.0, self.cw - 12.0);
        let h = LIST_PAD * 2.0 + rows.len() as f64 * ROW_H + if extra > 0 { 16.0 } else { 0.0 };
        let right = x0 + w - 9.0;
        let phrases: Vec<String> = rows.iter().map(|s| status_phrase(&s.state, &s.detail, false, Some(self.t))).collect();
        let mut key = format!("{}|{}|{}", self.scale, self.cw, extra);
        for (s, p) in rows.iter().zip(&phrases) {
            key += &format!("|{}|{}|{}|{}|{}", s.display_name(), s.project, s.state, p, s.path.display());
        }

        if self.list_layer.as_ref().map_or(true, |l| l.key != key) {
            self.list_layer = self.render_layer(key, top - 1.0, h + 4.0, |c| {
                rounded_rect(c, x0, top + 1.5, w, h, 10.0);
                c.set_source_rgba(0.0, 0.0, 0.0, 0.25);
                let _ = c.fill();
                rounded_rect(c, x0, top, w, h, 10.0);
                c.set_source_rgba(0.09, 0.09, 0.12, 0.9);
                let _ = c.fill_preserve();
                c.set_source_rgba(1.0, 1.0, 1.0, 0.12);
                c.set_line_width(1.0);
                let _ = c.stroke();

                let mut rects = Vec::new();
                let mut y = top + LIST_PAD;
                for (s, phrase) in rows.iter().zip(&phrases) {
                    let (_, text_rgb) = state_colors(&s.state);
                    let mid = y + ROW_H / 2.0;
                    // status, right-aligned in a fixed slot so it doesn't jitter
                    let widest = status_phrase(&s.state, &s.detail, false, None);
                    let (slot, _) = extents(&layout(c, &esc(&widest), 11.0, pango::Weight::Medium));
                    let status = layout(
                        c,
                        &format!("<span weight='medium' foreground='{}'>{}</span>", hex(text_rgb), esc(phrase)),
                        11.0,
                        pango::Weight::Normal,
                    );
                    // session name, then its folder in grey if there's room
                    let room = right - slot - 8.0 - (x0 + 21.0);
                    let name = layout(
                        c,
                        &format!("<span weight='bold' foreground='#ffffff'>{}</span>", esc(s.display_name())),
                        11.0,
                        pango::Weight::Normal,
                    );
                    name.set_ellipsize(pango::EllipsizeMode::End);
                    name.set_width(((room * 0.75) * pango::SCALE as f64) as i32);
                    let (name_w, text_h) = extents(&name);
                    let ty = mid - text_h / 2.0;
                    c.move_to(x0 + 21.0, ty);
                    show(c, &name);
                    let folder_w = room - name_w - 6.0;
                    if !s.project.is_empty() && folder_w > 24.0 {
                        let folder = layout(
                            c,
                            &format!("<span foreground='#ffffff' fgalpha='45%'>{}</span>", esc(&s.project)),
                            10.0,
                            pango::Weight::Normal,
                        );
                        folder.set_ellipsize(pango::EllipsizeMode::End);
                        folder.set_width((folder_w * pango::SCALE as f64) as i32);
                        let (_, fh) = extents(&folder);
                        c.move_to(x0 + 21.0 + name_w + 6.0, mid - fh / 2.0);
                        show(c, &folder);
                    }
                    c.move_to(right - slot, ty);
                    show(c, &status);
                    rects.push(Row { x: x0, y, w, h: ROW_H, path: s.path.clone() });
                    y += ROW_H;
                }
                if extra > 0 {
                    let more = layout(c, &format!("<span foreground='#ffffff' fgalpha='50%'>+{extra} more</span>"), 10.0, pango::Weight::Normal);
                    let (mw, _) = extents(&more);
                    c.move_to(x0 + (w - mw) / 2.0, y);
                    show(c, &more);
                }
                (0.0, rects)
            });
        }

        let Some(layer) = &self.list_layer else { return };
        Self::blit(cr, layer);
        self.rows = layer.rows.clone();
        // animated parts on top: pulsing highlight for rows that need you, status dots
        for (row, s) in self.rows.iter().zip(&rows) {
            if s.state == "waiting" {
                let glow = 0.5 + 0.5 * (self.t * 0.5).sin();
                rounded_rect(cr, row.x + 3.0, row.y + 1.0, row.w - 6.0, row.h - 2.0, 6.0);
                cr.set_source_rgba(1.0, 0.3, 0.3, 0.08 + 0.08 * glow);
                let _ = cr.fill();
            }
            self.status_dot(cr, row.x + 12.0, row.y + row.h / 2.0, &s.state, 3.2);
        }
    }

    /// The current species as an image pet, loaded on first use.
    fn custom_pet(&mut self) -> Option<&(String, Pixbuf, Meta)> {
        if self.custom.as_ref().map(|c| c.0 != self.species).unwrap_or(true) {
            self.custom = custom::load(&self.species).map(|(pb, meta)| (self.species.clone(), pb, meta));
        }
        self.custom.as_ref()
    }

    /// Your own artwork, untouched; the state shows in its aura, motion and bubble.
    fn draw_custom(&mut self, cr: &Context, bob: f64, asleep: bool) {
        let Some((_, pb, meta)) = self.custom_pet().cloned() else { return };
        let (t, state) = (self.t, self.state.clone());
        let card = meta.mode == "card";
        let (iw, ih) = (pb.width() as f64, pb.height() as f64);
        let mut s = (if card { 112.0 } else { 124.0 }) / ih;
        if iw * s > 150.0 {
            s = 150.0 / iw;
        }
        let (w, h) = (iw * s, ih * s);
        let waiting = state == "waiting" && !asleep;
        let shake = if waiting { (t * 1.3).sin() * 2.5 } else { 0.0 };
        let breathe = if state == "idle" && !asleep { 1.0 + 0.012 * (t * 0.08).sin() } else { 1.0 };
        let cx = W / 2.0 - 14.0 + shake;
        let (x, y) = (cx - w / 2.0, H - 30.0 - h + bob);

        shadow(cr, W / 2.0 - 14.0, H - 28.0, w * 0.42, 0.22);

        // aura in the state's colour (the pet's own colour when idle)
        if !asleep {
            let (rgb, alpha) = match state.as_str() {
                "working" => ((0.36, 0.62, 1.0), 0.28 + 0.12 * (t * 0.3).sin()),
                "waiting" => ((1.0, 0.3, 0.3), 0.45 + 0.2 * (t * 0.6).sin()),
                "done" => ((0.3, 0.9, 0.5), 0.4),
                _ => ((meta.accent[0], meta.accent[1], meta.accent[2]), 0.16),
            };
            let (gx, gy, gr) = (cx, y + h * 0.5, w.max(h) * 0.62);
            let g = cairo::RadialGradient::new(gx, gy, gr * 0.15, gx, gy, gr);
            g.add_color_stop_rgba(0.0, rgb.0, rgb.1, rgb.2, alpha);
            g.add_color_stop_rgba(1.0, rgb.0, rgb.1, rgb.2, 0.0);
            let _ = cr.set_source(&g);
            let _ = cr.save();
            cr.translate(gx, gy);
            cr.scale(1.0, h / w.max(h) * 1.05);
            cr.arc(0.0, 0.0, gr, 0.0, TAU);
            let _ = cr.restore();
            let _ = cr.fill();
        }

        // the artwork, breathing gently around its feet
        let _ = cr.save();
        cr.translate(x + w / 2.0, y + h);
        cr.scale(breathe, breathe);
        cr.translate(-w / 2.0, -h);
        if card {
            rounded_rect(cr, 0.0, 0.0, w, h, 10.0);
            cr.clip();
        }
        cr.scale(s, s);
        cr.set_source_pixbuf(&pb, 0.0, 0.0);
        cr.source().set_filter(cairo::Filter::Good);
        let _ = cr.paint_with_alpha(if asleep { 0.5 } else { 1.0 });
        let _ = cr.restore();
        if card {
            let (bc, ba) = match state.as_str() {
                "waiting" if !asleep => ((1.0, 0.35, 0.35), 0.6 + 0.4 * (t * 0.6).sin()),
                "working" if !asleep => ((0.45, 0.7, 1.0), 0.9),
                "done" if !asleep => ((0.35, 0.9, 0.5), 0.9),
                _ => ((1.0, 1.0, 1.0), 0.85),
            };
            rounded_rect(cr, x, y, w, h, 10.0);
            cr.set_source_rgba(bc.0, bc.1, bc.2, ba);
            cr.set_line_width(2.5);
            let _ = cr.stroke();
        }

        // finished: a few twinkling stars
        if state == "done" && !asleep {
            for k in 0..3 {
                let ph = (t * 0.08 + k as f64 / 3.0).rem_euclid(1.0);
                let (sx, sy) = (x + w * [0.1, 0.9, 0.2][k] , y + h * [0.15, 0.3, 0.55][k] - ph * 8.0);
                twinkle(cr, sx, sy, 3.5 * (1.0 - ph) + 1.0, 1.0 - ph);
            }
        }
        self.draw_bubble(cr, x + w - 6.0, y + 10.0, asleep);
    }

    /// Plan usage as themed meters: what's left in the 5-hour and weekly windows.
    fn draw_usage(&mut self, cr: &Context, top: f64) {
        let (term, accent) = match self.custom_pet() {
            Some((_, _, meta)) => {
                let term: &'static str = match meta.meter.as_str() {
                    "Chakra" => "Chakra",
                    "Battery" => "Battery",
                    "HP" => "HP",
                    "Mana" => "Mana",
                    _ => "Energy",
                };
                (term, (meta.accent[0], meta.accent[1], meta.accent[2]))
            }
            None => usage_theme(&self.species),
        };
        let rows = self.usage.clone();
        let (x0, w) = (6.0, self.cw - 12.0);
        let h = USAGE_PAD * 2.0 + rows.len().max(1) as f64 * USAGE_ROW;
        let (bar_x, bar_w) = (x0 + 94.0, 56.0);
        let texts: Vec<(String, String)> = rows
            .iter()
            .map(|r| (format!("{:.0}%", r.left), if r.resets_at > 0.0 { usage::until(r.resets_at) } else { String::new() }))
            .collect();
        let mut key = format!("{}|{}|{}|{}", self.scale, self.cw, top, term);
        for (r, (pct, reset)) in rows.iter().zip(&texts) {
            key += &format!("|{}|{pct}|{reset}", r.label);
        }

        if self.usage_layer.as_ref().map_or(true, |l| l.key != key) {
            self.usage_layer = self.render_layer(key, top - 1.0, h + 4.0, |c| {
                rounded_rect(c, x0, top + 1.5, w, h, 10.0);
                c.set_source_rgba(0.0, 0.0, 0.0, 0.25);
                let _ = c.fill();
                rounded_rect(c, x0, top, w, h, 10.0);
                c.set_source_rgba(0.09, 0.09, 0.12, 0.9);
                let _ = c.fill_preserve();
                c.set_source_rgba(1.0, 1.0, 1.0, 0.12);
                c.set_line_width(1.0);
                let _ = c.stroke();
                if rows.is_empty() {
                    let hint = layout(
                        c,
                        "<span foreground='#ffffff' fgalpha='55%'>usage shows after Claude's next reply</span>",
                        10.0,
                        pango::Weight::Normal,
                    );
                    let (hw, hh) = extents(&hint);
                    c.move_to(x0 + (w - hw) / 2.0, top + (h - hh) / 2.0);
                    show(c, &hint);
                    return (0.0, Vec::new());
                }
                let mut y = top + USAGE_PAD;
                for (r, (pct, reset)) in rows.iter().zip(&texts) {
                    let mid = y + USAGE_ROW / 2.0;
                    let name = layout(
                        c,
                        &format!(
                            "<span weight='bold' foreground='#ffffff'>{}</span><span foreground='#ffffff' fgalpha='55%'> · {}</span>",
                            esc(term),
                            r.label
                        ),
                        10.5,
                        pango::Weight::Normal,
                    );
                    c.move_to(x0 + 24.0, mid - extents(&name).1 / 2.0);
                    show(c, &name);
                    let col = hex(usage_color(accent, r.left));
                    let p = layout(c, &format!("<span weight='bold' foreground='{col}'>{pct}</span>"), 10.5, pango::Weight::Normal);
                    c.move_to(bar_x + bar_w + 6.0, mid - extents(&p).1 / 2.0);
                    show(c, &p);
                    if !reset.is_empty() {
                        let rl = layout(c, &format!("<span foreground='#ffffff' fgalpha='60%'>{}</span>", esc(reset)), 10.0, pango::Weight::Normal);
                        let (rw, rh) = extents(&rl);
                        c.move_to(x0 + w - 9.0 - rw, mid - rh / 2.0);
                        show(c, &rl);
                        refill_icon(c, x0 + w - 9.0 - rw - 7.0, mid);
                    }
                    y += USAGE_ROW;
                }
                (0.0, Vec::new())
            });
        }
        if let Some(layer) = &self.usage_layer {
            Self::blit(cr, layer);
        }
        // live parts: icons and meters (the meter pulses when nearly empty)
        let mut y = top + USAGE_PAD;
        for r in &rows {
            let mid = y + USAGE_ROW / 2.0;
            if r.label == "5h" {
                bolt_icon(cr, x0 + 13.0, mid, accent);
            } else {
                calendar_icon(cr, x0 + 13.0, mid, accent);
            }
            self.meter(cr, bar_x, mid - 4.0, bar_w, 8.0, r.left, usage_color(accent, r.left));
            y += USAGE_ROW;
        }
    }

    /// A segmented, glossy health-bar style meter showing what's left.
    #[allow(clippy::too_many_arguments)]
    fn meter(&self, cr: &Context, x: f64, y: f64, w: f64, h: f64, left: f64, c: Rgb) {
        rounded_rect(cr, x, y, w, h, h / 2.0);
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.12);
        let _ = cr.fill();
        let fw = w * left / 100.0;
        if fw > 0.5 {
            let alpha = if left < 15.0 { 0.55 + 0.45 * (0.5 + 0.5 * (self.t * 0.6).sin()) } else { 1.0 };
            let _ = cr.save();
            rounded_rect(cr, x, y, w, h, h / 2.0);
            cr.clip();
            cr.rectangle(x, y, fw, h);
            cr.set_source_rgba(c.0, c.1, c.2, alpha);
            let _ = cr.fill();
            cr.rectangle(x, y, fw, h * 0.45); // gloss
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.28 * alpha);
            let _ = cr.fill();
            let _ = cr.restore();
        }
        cr.set_source_rgba(0.09, 0.09, 0.12, 0.9); // segment ticks, like an HP bar
        for i in 1..10 {
            cr.rectangle(x + w * i as f64 / 10.0 - 0.5, y, 1.0, h);
        }
        let _ = cr.fill();
    }
}

fn shadow(cr: &Context, cx: f64, cy: f64, r: f64, flatten: f64) {
    let _ = cr.save();
    cr.translate(cx, cy);
    cr.scale(1.0, flatten);
    cr.arc(0.0, 0.0, r, 0.0, TAU);
    let _ = cr.restore();
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.18);
    let _ = cr.fill();
}

fn bolt_icon(cr: &Context, x: f64, y: f64, c: Rgb) {
    cr.move_to(x + 1.5, y - 6.0);
    cr.line_to(x - 3.5, y + 1.0);
    cr.line_to(x - 0.2, y + 1.0);
    cr.line_to(x - 1.5, y + 6.0);
    cr.line_to(x + 3.5, y - 1.0);
    cr.line_to(x + 0.2, y - 1.0);
    cr.close_path();
    cr.set_source_rgb(c.0, c.1, c.2);
    let _ = cr.fill();
}

fn calendar_icon(cr: &Context, x: f64, y: f64, c: Rgb) {
    rounded_rect(cr, x - 5.0, y - 4.5, 10.0, 9.5, 1.8);
    cr.set_source_rgb(c.0, c.1, c.2);
    cr.set_line_width(1.3);
    let _ = cr.stroke();
    cr.rectangle(x - 5.0, y - 4.5, 10.0, 3.0);
    let _ = cr.fill();
    for dx in [-2.5, 0.0, 2.5] {
        cr.rectangle(x + dx - 0.6, y + 0.5, 1.2, 1.2);
        cr.rectangle(x + dx - 0.6, y + 2.6, 1.2, 1.2);
    }
    let _ = cr.fill();
}

/// A small circular arrow: "refills in".
fn refill_icon(cr: &Context, x: f64, y: f64) {
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.6);
    cr.set_line_width(1.2);
    cr.new_sub_path();
    cr.arc(x, y, 3.2, 0.6, 5.6);
    let _ = cr.stroke();
    let (ax, ay) = (x + 3.2 * 0.6f64.cos(), y + 3.2 * 0.6f64.sin());
    cr.move_to(ax + 1.8, ay - 0.6);
    cr.line_to(ax - 0.3, ay + 1.9);
    cr.line_to(ax - 0.9, ay - 1.2);
    cr.close_path();
    let _ = cr.fill();
}

fn twinkle(cr: &Context, x: f64, y: f64, r: f64, a: f64) {
    cr.set_source_rgba(1.0, 0.9, 0.35, a);
    cr.move_to(x, y - r);
    for i in 1..8 {
        let rr = if i % 2 == 0 { r } else { r * 0.3 };
        let ang = -PI / 2.0 + i as f64 * PI / 4.0;
        cr.line_to(x + rr * ang.cos(), y + rr * ang.sin());
    }
    cr.close_path();
    let _ = cr.fill();
}
