//! Pet artwork. Vector pets are balls of radius `r` drawn at (cx, cy); pixel
//! pets are 12x10 sprites. `t` is the animation tick (about 8.3 per second).

use gtk::cairo::{Context, LinearGradient, LineCap, Matrix, RadialGradient};
use std::f64::consts::{PI, TAU};

pub const SPECIES: &[&str] = &["blob", "cat", "crab", "ghost", "crimson", "ripple", "spiral", "robot", "flash", "lavender"];

/// All pets, as owned strings (for menus and validation).
pub fn all_species() -> Vec<String> {
    SPECIES.iter().map(|s| s.to_string()).collect()
}

pub fn is_vector(name: &str) -> bool {
    matches!(name, "crimson" | "ripple" | "spiral" | "robot" | "flash" | "lavender")
}

type Rgb = (f64, f64, f64);

/// Draw a vector pet. Returns false if `name` isn't one.
#[allow(clippy::too_many_arguments)]
pub fn draw_vector(name: &str, cr: &Context, cx: f64, cy: f64, r: f64, t: f64, state: &str, asleep: bool) -> bool {
    match name {
        "crimson" => crimson_eye(cr, cx, cy, r, t, state, asleep),
        "ripple" => ripple_eye(cr, cx, cy, r, t, state, asleep),
        "spiral" => spiral_orb(cr, cx, cy, r, t, state, asleep),
        "robot" => robot(cr, cx, cy, r, t, state, asleep),
        "flash" => crate::chibi::flash(cr, cx, cy, r, t, state, asleep),
        "lavender" => crate::chibi::lavender(cr, cx, cy, r, t, state, asleep),
        _ => return false,
    }
    true
}

// ---- helpers -----------------------------------------------------------------

// cairo drawing calls only fail if the context is already in an error state,
// in which case there is nothing useful to do; the results are ignored.
fn fill(cr: &Context) {
    let _ = cr.fill();
}
fn stroke(cr: &Context) {
    let _ = cr.stroke();
}
fn save(cr: &Context) {
    let _ = cr.save();
}
fn restore(cr: &Context) {
    let _ = cr.restore();
}

fn pol(r: f64, deg: f64) -> (f64, f64) {
    let a = deg.to_radians();
    (r * a.cos(), r * a.sin())
}

fn speed(state: &str, asleep: bool, idle: f64, working: f64, waiting: f64, done: f64) -> f64 {
    if asleep {
        return 0.0;
    }
    match state {
        "working" => working,
        "waiting" => waiting,
        "done" => done,
        _ => idle,
    }
}

fn pulse(t: f64, rate: f64) -> f64 {
    0.5 + 0.5 * (t * rate).sin()
}

fn glow(cr: &Context, cx: f64, cy: f64, r: f64, rgb: Rgb, alpha: f64) {
    if alpha <= 0.0 {
        return;
    }
    let g = RadialGradient::new(cx, cy, r * 0.2, cx, cy, r);
    g.add_color_stop_rgba(0.0, rgb.0, rgb.1, rgb.2, alpha);
    g.add_color_stop_rgba(1.0, rgb.0, rgb.1, rgb.2, 0.0);
    let _ = cr.set_source(&g);
    cr.arc(cx, cy, r, 0.0, TAU);
    fill(cr);
}

/// Darken the rim so a flat disc reads as a ball.
fn sphere_shade(cr: &Context, cx: f64, cy: f64, r: f64, dark: f64) {
    let g = RadialGradient::new(cx - r * 0.3, cy - r * 0.35, r * 0.1, cx, cy, r);
    g.add_color_stop_rgba(0.0, 1.0, 1.0, 1.0, 0.0);
    g.add_color_stop_rgba(0.7, 0.0, 0.0, 0.0, 0.0);
    g.add_color_stop_rgba(1.0, 0.0, 0.0, 0.0, dark);
    let _ = cr.set_source(&g);
    cr.arc(cx, cy, r, 0.0, TAU);
    fill(cr);
}

fn dim(cr: &Context, cx: f64, cy: f64, r: f64, alpha: f64) {
    cr.arc(cx, cy, r, 0.0, TAU);
    cr.set_source_rgba(0.05, 0.03, 0.08, alpha);
    fill(cr);
}

/// Comma-shaped tomoe on an orbit; the tail trails the spin.
fn tomoe(cr: &Context, cx: f64, cy: f64, orbit: f64, theta: f64, h: f64, rgb: Rgb) {
    save(cr);
    cr.translate(cx + orbit * theta.cos(), cy + orbit * theta.sin());
    cr.rotate(theta);
    cr.set_source_rgb(rgb.0, rgb.1, rgb.2);
    cr.arc(0.0, 0.0, h, 0.0, TAU);
    fill(cr);
    cr.move_to(h, 0.0);
    cr.curve_to(h * 1.1, -h * 1.4, h * 0.2, -h * 2.6, -h * 0.9, -h * 3.0);
    cr.curve_to(-h * 0.1, -h * 2.0, -h * 0.6, -h * 0.9, -h, 0.0);
    cr.close_path();
    fill(cr);
    restore(cr);
}

fn state_glow(state: &str, t: f64) -> f64 {
    let base = match state {
        "waiting" => 0.5,
        "working" => 0.28,
        _ => 0.15,
    };
    base * (0.6 + 0.4 * pulse(t, 0.3))
}

const INK: Rgb = (0.03, 0.0, 0.01);

// ---- crimson eye --------------------------------------------------------------

fn crimson_eye(cr: &Context, cx: f64, cy: f64, r: f64, t: f64, state: &str, asleep: bool) {
    let ang = t * speed(state, asleep, 0.012, 0.14, 0.22, 0.035);
    glow(cr, cx, cy, r * 1.45, (0.95, 0.05, 0.05), if asleep { 0.0 } else { state_glow(state, t) });

    let g = RadialGradient::new(cx - r * 0.25, cy - r * 0.3, r * 0.1, cx, cy, r);
    g.add_color_stop_rgb(0.0, 1.0, 0.24, 0.2);
    g.add_color_stop_rgb(0.55, 0.86, 0.03, 0.05);
    g.add_color_stop_rgb(1.0, 0.42, 0.0, 0.02);
    cr.arc(cx, cy, r, 0.0, TAU);
    let _ = cr.set_source(&g);
    fill(cr);

    if state != "waiting" || asleep {
        scythe_pattern(cr, cx, cy, r, ang);
    } else {
        // your turn: the classic three tomoe on the perimeter
        cr.arc(cx, cy, r * 0.66, 0.0, TAU);
        cr.set_source_rgba(0.1, 0.0, 0.0, 0.85);
        cr.set_line_width(r * 0.025);
        stroke(cr);
        cr.arc(cx, cy, r * 0.2, 0.0, TAU);
        cr.set_source_rgb(INK.0, INK.1, INK.2);
        fill(cr);
        for k in 0..3 {
            tomoe(cr, cx, cy, r * 0.66, ang + k as f64 * TAU / 3.0, r * 0.15, INK);
        }
    }

    sphere_shade(cr, cx, cy, r, 0.45);
    cr.arc(cx, cy, r, 0.0, TAU);
    cr.set_source_rgb(0.06, 0.0, 0.0);
    cr.set_line_width(r * 0.07);
    stroke(cr);
    if asleep {
        dim(cr, cx, cy, r, 0.45);
    }
}

/// Three curved scythe blades from a black core with a red pupil; a thin line
/// runs alongside each blade's outer (convex) edge.
fn scythe_pattern(cr: &Context, cx: f64, cy: f64, r: f64, ang: f64) {
    save(cr);
    cr.translate(cx, cy);
    cr.scale(r, r); // draw in units of the iris radius
    cr.rotate(ang);
    cr.set_source_rgb(0.04, 0.0, 0.01);
    for k in 0..3 {
        save(cr);
        cr.rotate(k as f64 * TAU / 3.0);
        cr.move_to(0.15, -0.13);
        cr.curve_to(0.45, -0.08, 0.75, -0.1, 0.95, -0.38);
        cr.curve_to(0.93, 0.0, 0.7, 0.4, 0.1, 0.2);
        cr.close_path();
        fill(cr);
        cr.move_to(0.975, -0.2);
        cr.curve_to(0.98, 0.12, 0.74, 0.47, 0.3, 0.44);
        cr.set_line_width(0.028);
        cr.set_line_cap(LineCap::Round);
        stroke(cr);
        restore(cr);
    }
    cr.arc(0.0, 0.0, 0.3, 0.0, TAU);
    fill(cr);
    cr.arc(0.0, 0.0, 0.11, 0.0, TAU);
    cr.set_source_rgb(0.72, 0.04, 0.05);
    fill(cr);
    restore(cr);
}

// ---- ripple eye ---------------------------------------------------------------

fn ripple_eye(cr: &Context, cx: f64, cy: f64, r: f64, t: f64, state: &str, asleep: bool) {
    let alert = state == "waiting" && !asleep; // turns red, ripples race outward
    let glow_rgb = if alert { (0.95, 0.05, 0.1) } else { (0.6, 0.35, 0.95) };
    glow(cr, cx, cy, r * 1.45, glow_rgb, if asleep { 0.0 } else { state_glow(state, t) });

    let g = RadialGradient::new(cx - r * 0.25, cy - r * 0.3, r * 0.1, cx, cy, r);
    let ring: Rgb = if alert {
        g.add_color_stop_rgb(0.0, 1.0, 0.26, 0.24);
        g.add_color_stop_rgb(1.0, 0.55, 0.0, 0.05);
        (0.12, 0.0, 0.02)
    } else {
        g.add_color_stop_rgb(0.0, 0.90, 0.84, 1.0);
        g.add_color_stop_rgb(1.0, 0.60, 0.48, 0.85);
        (0.20, 0.10, 0.32)
    };
    cr.arc(cx, cy, r, 0.0, TAU);
    let _ = cr.set_source(&g);
    fill(cr);

    // ripples drift outward while working
    let drift = (t * speed(state, asleep, 0.004, 0.03, 0.07, 0.01)).rem_euclid(1.0);
    cr.set_source_rgb(ring.0, ring.1, ring.2);
    cr.set_line_width(r * 0.035);
    for i in 0..5 {
        let rr = r * (0.22 + 0.17 * (i as f64 + drift));
        if rr < r * 0.95 {
            cr.arc(cx, cy, rr, 0.0, TAU);
            stroke(cr);
        }
    }
    cr.arc(cx, cy, r * 0.13, 0.0, TAU);
    fill(cr);

    if alert {
        // a bright ring pulsing out from the centre
        let ph = (t * 0.12).rem_euclid(1.0);
        cr.arc(cx, cy, r * (0.15 + 0.8 * ph), 0.0, TAU);
        cr.set_source_rgba(1.0, 0.85, 0.8, 0.8 * (1.0 - ph));
        cr.set_line_width(r * 0.05);
        stroke(cr);
    }

    sphere_shade(cr, cx, cy, r, 0.35);
    cr.arc(cx, cy, r, 0.0, TAU);
    cr.set_source_rgb(ring.0, ring.1, ring.2);
    cr.set_line_width(r * 0.05);
    stroke(cr);
    if asleep {
        dim(cr, cx, cy, r, 0.45);
    }
}

// ---- spiral orb ---------------------------------------------------------------

fn spiral_orb(cr: &Context, cx: f64, cy: f64, r: f64, t: f64, state: &str, asleep: bool) {
    let shuriken = state == "waiting" && !asleep; // grows spinning wind blades
    let spd = speed(state, asleep, 0.05, 0.28, 0.35, 0.1);
    let r = if asleep { r * 0.7 } else { r };
    let flick = 0.85 + 0.15 * (t * 1.7).sin();
    let glow_r = r * if shuriken { 2.1 } else { 1.6 };
    glow(cr, cx, cy, glow_r, (0.3, 0.65, 1.0), if asleep { 0.18 } else { 0.55 * flick });

    if shuriken {
        save(cr);
        cr.translate(cx, cy);
        cr.rotate(t * 0.45);
        for k in 0..4 {
            save(cr);
            cr.rotate(k as f64 * TAU / 4.0);
            let (x0, y0) = pol(r * 0.6, -18.0);
            let (x1, y1) = pol(r * 1.2, -12.0);
            let (x2, y2) = pol(r * 1.6, 5.0);
            let (x3, y3) = pol(r * 1.75, 22.0);
            let (x4, y4) = pol(r * 1.4, 12.0);
            let (x5, y5) = pol(r * 1.05, 14.0);
            let (x6, y6) = pol(r * 0.6, 22.0);
            cr.move_to(x0, y0);
            cr.curve_to(x1, y1, x2, y2, x3, y3);
            cr.curve_to(x4, y4, x5, y5, x6, y6);
            cr.close_path();
            let g = LinearGradient::new(r * 0.6, 0.0, r * 1.75, 0.0);
            g.add_color_stop_rgba(0.0, 0.85, 0.95, 1.0, 0.9);
            g.add_color_stop_rgba(1.0, 0.55, 0.8, 1.0, 0.25);
            let _ = cr.set_source(&g);
            fill(cr);
            restore(cr);
        }
        restore(cr);
    }

    let g = RadialGradient::new(cx, cy, 0.0, cx, cy, r);
    g.add_color_stop_rgba(0.0, 1.0, 1.0, 1.0, 1.0);
    g.add_color_stop_rgba(0.25, 0.75, 0.93, 1.0, 0.95);
    g.add_color_stop_rgba(0.7, 0.2, 0.55, 1.0, 0.85);
    g.add_color_stop_rgba(1.0, 0.1, 0.3, 0.95, 0.35);
    cr.arc(cx, cy, r, 0.0, TAU);
    let _ = cr.set_source(&g);
    fill(cr);

    // swirling chakra streaks; inner layers spin faster
    cr.set_line_cap(LineCap::Round);
    for i in 0..14 {
        let fi = i as f64;
        let rr = r * (0.25 + 0.7 * (fi * 0.37).rem_euclid(1.0));
        let start = fi * 1.9 + t * spd * (1.6 - rr / r);
        let span = 0.9 + (i % 3) as f64 * 0.35;
        cr.new_path();
        cr.arc(cx, cy, rr, start, start + span);
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.35 + 0.4 * (fi * 0.53).rem_euclid(1.0));
        cr.set_line_width(r * (0.03 + 0.03 * (i % 2) as f64));
        stroke(cr);
    }
    // tilted outer rings of wind
    for (k, tilt) in [0.35_f64, -0.5].into_iter().enumerate() {
        save(cr);
        cr.translate(cx, cy);
        let dir = if k == 1 { 1.0 } else { -1.0 };
        cr.rotate(tilt + t * spd * 0.3 * dir);
        cr.scale(1.0, 0.32);
        cr.arc(0.0, 0.0, r * 1.08, 0.0, TAU);
        restore(cr);
        cr.set_source_rgba(0.8, 0.93, 1.0, 0.55);
        cr.set_line_width(r * 0.035);
        stroke(cr);
    }
    glow(cr, cx, cy, r * 0.35, (1.0, 1.0, 1.0), 0.9);
}

// ---- Robot ---------------------------------------------------------------------
// An isometric view of the rover. World axes: x = length (front is +x), y = width,
// z = up, in units of `s`. Visible faces are +x (front), +y (left side) and top.

const COS30: f64 = 0.866_025_403_784_438_6;
const ROBOT_L: f64 = 1.2; // chassis length
const ROBOT_W: f64 = 1.0; // chassis width
const ROBOT_ZB: f64 = 0.12; // chassis bottom
const ROBOT_ZT: f64 = 0.6; // chassis top
const ROBOT_RW: f64 = 0.36; // wheel radius
const ROBOT_TW: f64 = 0.17; // tyre width
const ROBOT_WX: f64 = 0.38; // wheel x positions (±)
const ROBOT_WY: f64 = ROBOT_W / 2.0 + 0.03; // inner face of the wheels (±)

struct Iso {
    cx: f64,
    oy: f64,
    s: f64,
}

impl Iso {
    fn p(&self, x: f64, y: f64, z: f64) -> (f64, f64) {
        (self.cx + (x - y) * COS30 * self.s, self.oy + (x + y) * 0.5 * self.s - z * self.s)
    }

    /// Map a face's 2-D (u, v) coordinates onto the screen.
    fn plane(&self, cr: &Context, origin: (f64, f64), u: (f64, f64), v: (f64, f64)) {
        cr.transform(Matrix::new(u.0, u.1, v.0, v.1, origin.0, origin.1));
    }

    fn poly(&self, cr: &Context, pts: &[(f64, f64, f64)], rgb: Rgb) {
        for (i, &(x, y, z)) in pts.iter().enumerate() {
            let (px, py) = self.p(x, y, z);
            if i == 0 {
                cr.move_to(px, py);
            } else {
                cr.line_to(px, py);
            }
        }
        cr.close_path();
        cr.set_source_rgb(rgb.0, rgb.1, rgb.2);
        let _ = cr.fill_preserve();
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.55);
        cr.set_line_width((self.s * 0.012).max(0.8));
        stroke(cr);
    }

    /// A tyre as a stack of discs from the inner to the outer face.
    fn wheel(&self, cr: &Context, x: f64, y_inner: f64, spin: f64) {
        let s = self.s;
        for i in 0..7 {
            let f = i as f64 / 6.0;
            save(cr);
            self.plane(cr, self.p(x, y_inner + ROBOT_TW * f, ROBOT_RW), (COS30 * s, 0.5 * s), (0.0, -s));
            cr.arc(0.0, 0.0, ROBOT_RW, 0.0, TAU);
            if i < 6 {
                let shade = 0.52 + 0.14 * f;
                cr.set_source_rgb(shade, shade, shade + 0.01);
                fill(cr);
            } else {
                // outer face: tyre, rim groove, hub, spin marks
                cr.set_source_rgb(0.80, 0.80, 0.82);
                fill(cr);
                cr.set_line_width(0.012);
                cr.set_source_rgb(0.45, 0.45, 0.47);
                cr.arc(0.0, 0.0, ROBOT_RW * 0.8, 0.0, TAU);
                stroke(cr);
                cr.arc(0.0, 0.0, ROBOT_RW * 0.62, 0.0, TAU);
                cr.set_source_rgb(0.68, 0.68, 0.70);
                fill(cr);
                cr.set_source_rgb(0.40, 0.40, 0.43);
                cr.set_line_width(0.03);
                cr.set_line_cap(LineCap::Round);
                for k in 0..3 {
                    let a = spin + k as f64 * TAU / 3.0;
                    cr.move_to(ROBOT_RW * 0.66 * a.cos(), ROBOT_RW * 0.66 * a.sin());
                    cr.line_to(ROBOT_RW * 0.76 * a.cos(), ROBOT_RW * 0.76 * a.sin());
                }
                stroke(cr);
            }
            restore(cr);
        }
    }
}

fn robot(cr: &Context, cx: f64, cy: f64, r: f64, t: f64, state: &str, asleep: bool) {
    let s = r * 0.95;
    let iso = Iso { cx, oy: cy + 0.5 * s, s }; // wheels rest on the pet's shadow
    let (l2, w2, bev) = (ROBOT_L / 2.0, ROBOT_W / 2.0, 0.07);
    let zt = ROBOT_ZT - bev;

    // rocking while it needs you; wheels spin while working
    let waiting = state == "waiting" && !asleep;
    let rock = if waiting { (t * 0.5).sin() * 0.06 } else { 0.0 };
    let spin = t * speed(state, asleep, 0.0, 0.35, 0.0, 0.0) + if waiting { (t * 0.5).sin() * 0.6 } else { 0.0 };
    save(cr);
    let (gx, gy) = iso.p(0.0, 0.0, 0.0);
    cr.translate(gx, gy);
    cr.rotate(rock);
    cr.translate(-gx, -gy);
    if asleep {
        cr.push_group();
    }

    // far wheels (behind the chassis)
    for x in [-ROBOT_WX, ROBOT_WX] {
        iso.wheel(cr, x, -ROBOT_WY - ROBOT_TW, spin);
    }

    // chassis: left side, front, chamfers, top
    iso.poly(cr, &[(-l2, w2, ROBOT_ZB), (l2, w2, ROBOT_ZB), (l2, w2, zt), (-l2, w2, zt)], (0.20, 0.20, 0.22));
    iso.poly(cr, &[(l2, w2, ROBOT_ZB), (l2, -w2, ROBOT_ZB), (l2, -w2, zt), (l2, w2, zt)], (0.25, 0.25, 0.27));
    iso.poly(
        cr,
        &[(-l2, w2, zt), (l2, w2, zt), (l2 - bev, w2 - bev, ROBOT_ZT), (-l2 + bev, w2 - bev, ROBOT_ZT)],
        (0.29, 0.29, 0.31),
    );
    iso.poly(
        cr,
        &[(l2, w2, zt), (l2, -w2, zt), (l2 - bev, -w2 + bev, ROBOT_ZT), (l2 - bev, w2 - bev, ROBOT_ZT)],
        (0.33, 0.33, 0.35),
    );
    iso.poly(
        cr,
        &[
            (-l2 + bev, -w2 + bev, ROBOT_ZT),
            (l2 - bev, -w2 + bev, ROBOT_ZT),
            (l2 - bev, w2 - bev, ROBOT_ZT),
            (-l2 + bev, w2 - bev, ROBOT_ZT),
        ],
        (0.36, 0.36, 0.38),
    );

    // top plate, drawn in the top plane: u = x, v = y
    save(cr);
    iso.plane(cr, iso.p(0.0, 0.0, ROBOT_ZT), (COS30 * s, 0.5 * s), (-COS30 * s, 0.5 * s));
    let (pl, pw, pr) = (l2 - 0.16, w2 - 0.14, 0.12);
    cr.new_sub_path();
    cr.arc(pl - pr, -pw + pr, pr, -PI / 2.0, 0.0);
    cr.arc(pl - pr, pw - pr, pr, 0.0, PI / 2.0);
    cr.arc(-pl + pr, pw - pr, pr, PI / 2.0, PI);
    cr.arc(-pl + pr, -pw + pr, pr, PI, 3.0 * PI / 2.0);
    cr.close_path();
    cr.set_source_rgb(0.40, 0.40, 0.42);
    let _ = cr.fill_preserve();
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.6);
    cr.set_line_width(0.012);
    stroke(cr);
    // rows of vent dots along the long edges
    cr.set_source_rgba(0.08, 0.08, 0.09, 0.9);
    for yy in [-pw + 0.09, pw - 0.09] {
        for i in 0..8 {
            cr.arc(-pl + 0.12 + i as f64 * (2.0 * pl - 0.24) / 7.0, yy, 0.013, 0.0, TAU);
            fill(cr);
        }
    }
    // hinge latches at the back, two round ports
    cr.set_source_rgb(0.62, 0.62, 0.64);
    for yy in [-0.22, 0.08] {
        cr.rectangle(-pl - 0.02, yy, 0.12, 0.13);
        fill(cr);
    }
    for (px, py) in [(-pl + 0.3, 0.3), (pl - 0.26, -0.26)] {
        cr.arc(px, py, 0.07, 0.0, TAU);
        cr.set_source_rgb(0.14, 0.14, 0.15);
        fill(cr);
        cr.arc(px, py, 0.07, 0.0, TAU);
        cr.set_source_rgb(0.62, 0.62, 0.64);
        cr.set_line_width(0.018);
        stroke(cr);
    }
    restore(cr);

    // front face: the status light (u = -y, v = -z)
    save(cr);
    iso.plane(cr, iso.p(l2, w2, zt), (COS30 * s, -0.5 * s), (0.0, s));
    let led: Rgb = match state {
        "working" => (0.36, 0.62, 1.0),
        "waiting" => (1.0, 0.25, 0.25),
        "done" => (0.25, 0.85, 0.45),
        _ => (0.85, 0.85, 0.9),
    };
    let on = if asleep {
        0.0
    } else {
        match state {
            "waiting" if (t * 0.5).rem_euclid(2.0) < 1.2 => 1.0, // blink
            "waiting" => 0.15,
            "working" => 0.6 + 0.4 * pulse(t, 0.4),
            "done" => 1.0,
            _ => 0.35 + 0.15 * pulse(t, 0.1),
        }
    };
    let (lx, ly) = (ROBOT_W / 2.0, 0.3);
    if on > 0.2 {
        let g = RadialGradient::new(lx, ly, 0.0, lx, ly, 0.3);
        g.add_color_stop_rgba(0.0, led.0, led.1, led.2, 0.7 * on);
        g.add_color_stop_rgba(1.0, led.0, led.1, led.2, 0.0);
        let _ = cr.set_source(&g);
        cr.arc(lx, ly, 0.3, 0.0, TAU);
        fill(cr);
    }
    cr.rectangle(lx - 0.16, ly - 0.025, 0.32, 0.05);
    let dimmed = |c: f64| 0.15 + (c - 0.15) * on;
    cr.set_source_rgb(dimmed(led.0), dimmed(led.1), dimmed(led.2));
    fill(cr);
    restore(cr);

    // near wheels (in front of the chassis)
    for x in [-ROBOT_WX, ROBOT_WX] {
        iso.wheel(cr, x, ROBOT_WY, spin);
    }

    if asleep {
        let _ = cr.pop_group_to_source();
        let _ = cr.paint_with_alpha(0.6);
    }
    restore(cr);
}

// ---- pixel pets ----------------------------------------------------------------

/// Size of one sprite pixel.
pub const PX: f64 = 7.0;

// D = outline, B = body, E = eye, P = blush/accent, . = transparent
pub fn sprite(name: &str) -> &'static [&'static str] {
    match name {
        "cat" => &[
            ".D........D.", "DBD......DBD", "DBBDDDDDDBBD", "DBBBBBBBBBBD", "DBEBBBBBBEBD",
            "DBEBBPPBBEBD", "DBBBBDDBBBBD", ".DBBBBBBBBD.", ".DBDBBBBDBD.", ".DD.DDDD.DD.",
        ],
        "crab" => &[
            "DD........DD", "DBD......DBD", ".DBD.DD.DBD.", "..DDBBBBDD..", ".DBBEBBEBBD.",
            "DBBBEBBEBBBD", "DBPBBBBBBPBD", ".DBBBBBBBBD.", "..DBD..DBD..", "..DD....DD..",
        ],
        "ghost" => &[
            "...DDDDDD...", "..DBBBBBBD..", ".DBBBBBBBBD.", ".DBEBBBBEBD.", ".DBEBBBBEBD.",
            ".DBPBDDBPBD.", ".DBBBBBBBBD.", ".DBBBBBBBBD.", ".DBBDBBDBBD.", "..D..D..D...",
        ],
        _ => &[
            "....DDDD....", "..DDBBBBDD..", ".DBBBBBBBBD.", "DBBEBBBBEBBD", "DBBEBBBBEBBD",
            "DBPBBBBBBPBD", "DBBBBDDBBBBD", "DBBBBBBBBBBD", ".DBBBBBBBBD.", "..DDDDDDDD..",
        ],
    }
}

/// body, outline, accent
fn palette(name: &str) -> (Rgb, Rgb, Rgb) {
    match name {
        "cat" => ((0.98, 0.70, 0.36), (0.33, 0.18, 0.06), (1.0, 0.55, 0.60)),
        "crab" => ((0.90, 0.47, 0.36), (0.35, 0.11, 0.07), (1.0, 0.78, 0.60)),
        "ghost" => ((0.82, 0.78, 0.98), (0.25, 0.20, 0.42), (1.0, 0.62, 0.78)),
        _ => ((0.55, 0.85, 0.45), (0.13, 0.28, 0.12), (1.0, 0.60, 0.65)),
    }
}

pub fn draw_sprite(cr: &Context, name: &str, ox: f64, oy: f64, eyes_closed: bool) {
    let (body, dark, accent) = palette(name);
    let rows = sprite(name);
    for (y, row) in rows.iter().enumerate() {
        for (x, ch) in row.bytes().enumerate() {
            let (px, py) = (ox + x as f64 * PX, oy + y as f64 * PX);
            let rgb = match ch {
                b'.' => continue,
                b'D' => dark,
                b'P' => accent,
                b'E' if eyes_closed => {
                    cr.set_source_rgb(body.0, body.1, body.2);
                    cr.rectangle(px, py, PX, PX);
                    fill(cr);
                    let below = rows.get(y + 1).is_some_and(|r| r.as_bytes()[x] == b'E');
                    if !below {
                        cr.set_source_rgb(dark.0, dark.1, dark.2);
                        cr.rectangle(px, py + PX / 3.0, PX, PX / 3.0);
                        fill(cr);
                    }
                    continue;
                }
                b'E' => (0.08, 0.08, 0.1),
                _ => body,
            };
            cr.set_source_rgb(rgb.0, rgb.1, rgb.2);
            cr.rectangle(px, py, PX + 0.5, PX + 0.5);
            fill(cr);
        }
    }
}
