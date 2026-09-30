//! Chibi characters: Flash (a blond ninja in a white lightning cloak) and
//! Lavender (a shy girl with long indigo hair). Original designs.
//!
//! Drawn in "k units": the origin is the pet centre and 1 unit = R/44 px. The
//! head is centred at (0, -20), the torso spans y -2..18, feet rest at y ~37.

use gtk::cairo::{Context, LineCap, LineJoin};
use std::f64::consts::{PI, TAU};

type Rgb = (f64, f64, f64);

const OUTLINE: Rgb = (0.16, 0.12, 0.16);
const SKIN: Rgb = (1.0, 0.87, 0.75);

// cairo calls only fail on an already-broken context; nothing useful to do then.
fn rgb(cr: &Context, c: Rgb) {
    cr.set_source_rgb(c.0, c.1, c.2);
}

/// A closed path: 2 numbers = line to a point, 6 numbers = Bézier curve.
fn path(cr: &Context, pts: &[&[f64]]) {
    cr.move_to(pts[0][0], pts[0][1]);
    for p in &pts[1..] {
        if p.len() == 6 {
            cr.curve_to(p[0], p[1], p[2], p[3], p[4], p[5]);
        } else {
            cr.line_to(p[0], p[1]);
        }
    }
    cr.close_path();
}

fn fill(cr: &Context, c: Rgb, outline: bool) {
    rgb(cr, c);
    if outline {
        let _ = cr.fill_preserve();
        rgb(cr, OUTLINE);
        cr.set_line_width(1.1);
        cr.set_line_join(LineJoin::Round);
        let _ = cr.stroke();
    } else {
        let _ = cr.fill();
    }
}

fn shape(cr: &Context, pts: &[&[f64]], c: Rgb) {
    path(cr, pts);
    fill(cr, c, true);
}

fn ellipse_path(cr: &Context, x: f64, y: f64, rx: f64, ry: f64) {
    let _ = cr.save();
    cr.translate(x, y);
    cr.scale(rx, ry);
    cr.arc(0.0, 0.0, 1.0, 0.0, TAU);
    let _ = cr.restore();
}

fn ellipse(cr: &Context, x: f64, y: f64, rx: f64, ry: f64, c: Rgb, outline: bool) {
    ellipse_path(cr, x, y, rx, ry);
    fill(cr, c, outline);
}

fn limb(cr: &Context, x1: f64, y1: f64, x2: f64, y2: f64, w: f64, c: Rgb) {
    cr.set_line_cap(LineCap::Round);
    for (color, width) in [(OUTLINE, w + 2.2), (c, w)] {
        cr.move_to(x1, y1);
        cr.line_to(x2, y2);
        rgb(cr, color);
        cr.set_line_width(width);
        let _ = cr.stroke();
    }
}

/// An arm from the shoulder; angle 0 = straight down, positive = outward.
/// Returns the hand position.
fn arm(cr: &Context, side: f64, angle: f64, sleeve: Rgb, cuff: Option<Rgb>, length: f64) -> (f64, f64) {
    let sx = side * 10.5;
    let a = angle * side;
    let (hx, hy) = (sx + a.sin() * length, a.cos() * length);
    limb(cr, sx, 0.0, hx, hy, 6.5, sleeve);
    if let Some(cuff) = cuff {
        limb(cr, sx + (hx - sx) * 0.8, hy * 0.8, hx, hy, 6.8, cuff);
    }
    ellipse(cr, hx, hy + 1.0, 3.0, 3.0, SKIN, true);
    (hx, hy + 1.0)
}

fn legs(cr: &Context, pants: Rgb, wrap: Rgb, shoe: Rgb) {
    for side in [-1.0, 1.0] {
        let x = side * 5.2;
        limb(cr, x, 16.0, x, 30.0, 7.2, pants);
        limb(cr, x, 29.0, x, 33.5, 6.4, wrap);
        // sandal, toes flaring slightly outward
        let (right, left) = if side > 0.0 { (x + 5.0, x - 4.4) } else { (x + 4.4, x - 5.0) };
        shape(cr, &[&[x - 4.2, 33.0], &[x + 4.2, 33.0], &[right, 37.5], &[left, 37.5]], shoe);
    }
}

/// `pale`: soft lilac eyes with a small, gentle pupil.
fn eyes(cr: &Context, t: f64, state: &str, asleep: bool, iris: Rgb, pale: bool) {
    let blink = t.rem_euclid(37.0) < 1.6;
    for side in [-1.0, 1.0] {
        let (ex, ey) = (side * 7.6, -17.5);
        if asleep || blink {
            cr.move_to(ex - 3.6, ey + 0.6);
            cr.curve_to(ex - 1.5, ey + 2.8, ex + 1.5, ey + 2.8, ex + 3.6, ey + 0.6);
            rgb(cr, OUTLINE);
            cr.set_line_width(1.2);
            let _ = cr.stroke();
            continue;
        }
        if state == "done" {
            // happy ^ ^
            cr.move_to(ex - 3.4, ey + 1.5);
            cr.curve_to(ex - 1.5, ey - 2.8, ex + 1.5, ey - 2.8, ex + 3.4, ey + 1.5);
            rgb(cr, OUTLINE);
            cr.set_line_width(1.5);
            let _ = cr.stroke();
            continue;
        }
        ellipse(cr, ex, ey, 3.5, 4.4, (1.0, 1.0, 1.0), false);
        if pale {
            ellipse(cr, ex, ey + 0.4, 2.8, 3.5, iris, false);
            ellipse_path(cr, ex, ey + 0.4, 2.8, 3.5);
            cr.set_source_rgba(0.55, 0.5, 0.7, 0.8);
            cr.set_line_width(0.5);
            let _ = cr.stroke();
            ellipse(cr, ex, ey + 0.9, 1.0, 1.4, (0.55, 0.45, 0.72), false);
        } else {
            ellipse(cr, ex, ey + 0.4, 2.7, 3.5, iris, false);
            cr.set_source_rgb(0.05, 0.05, 0.08);
            ellipse_path(cr, ex, ey + 0.8, 1.2, 1.7);
            let _ = cr.fill();
        }
        ellipse(cr, ex - 1.0, ey - 1.3, 0.9, 0.9, (1.0, 1.0, 1.0), false);
        // upper lash line
        cr.move_to(ex - 3.9, ey - 1.8);
        cr.curve_to(ex - 2.0, ey - 5.0, ex + 2.0, ey - 5.0, ex + 3.9, ey - 2.2);
        rgb(cr, OUTLINE);
        cr.set_line_width(1.4);
        let _ = cr.stroke();
    }
}

fn mouth(cr: &Context, state: &str, asleep: bool) {
    rgb(cr, OUTLINE);
    cr.set_line_width(0.9);
    cr.set_line_cap(LineCap::Round);
    match state {
        "done" if !asleep => {
            // open grin
            cr.move_to(-3.2, -8.5);
            cr.curve_to(-2.5, -5.0, 2.5, -5.0, 3.2, -8.5);
            cr.close_path();
            cr.set_source_rgb(0.7, 0.2, 0.25);
            let _ = cr.fill_preserve();
            rgb(cr, OUTLINE);
            let _ = cr.stroke();
        }
        "waiting" if !asleep => ellipse(cr, 0.0, -7.8, 1.3, 1.6, (0.6, 0.2, 0.25), true), // "oh!"
        _ => {
            cr.move_to(-2.2, -8.4);
            cr.curve_to(-1.0, -7.0, 1.0, -7.0, 2.2, -8.4);
            let _ = cr.stroke();
        }
    }
}

fn face(cr: &Context, t: f64, state: &str, asleep: bool, iris: Rgb, pale: bool) {
    ellipse(cr, -19.2, -17.0, 2.4, 3.4, SKIN, true); // ears
    ellipse(cr, 19.2, -17.0, 2.4, 3.4, SKIN, true);
    shape(
        cr,
        &[
            &[-19.5, -24.0],
            &[-19.5, -30.0, -10.0, -38.0, 0.0, -38.0],
            &[10.0, -38.0, 19.5, -30.0, 19.5, -24.0],
            &[19.5, -12.0, 11.0, -2.5, 0.0, -2.5],
            &[-11.0, -2.5, -19.5, -12.0, -19.5, -24.0],
        ],
        SKIN,
    );
    eyes(cr, t, state, asleep, iris, pale);
    mouth(cr, state, asleep);
}

fn blush(cr: &Context, amount: f64) {
    for side in [-1.0, 1.0] {
        ellipse_path(cr, side * 11.5, -11.2, 3.4, 1.7);
        cr.set_source_rgba(1.0, 0.4, 0.5, amount);
        let _ = cr.fill();
    }
}

fn headband(cr: &Context) {
    let y = -27.5;
    shape(cr, &[&[-19.3, y - 2.0], &[19.3, y - 2.0], &[19.6, y + 3.0], &[-19.6, y + 3.0]], (0.18, 0.28, 0.5));
    shape(cr, &[&[-8.0, y - 3.0], &[8.0, y - 3.0], &[8.0, y + 4.0], &[-8.0, y + 4.0]], (0.78, 0.8, 0.84));
    // lightning-bolt emblem
    cr.set_source_rgb(0.35, 0.37, 0.42);
    cr.move_to(1.2, y - 2.2);
    cr.line_to(-1.6, y + 0.9);
    cr.line_to(0.0, y + 0.9);
    cr.line_to(-1.0, y + 3.2);
    cr.line_to(1.9, y - 0.2);
    cr.line_to(0.3, y - 0.2);
    cr.close_path();
    let _ = cr.fill();
}

fn sparkle(cr: &Context, x: f64, y: f64, r: f64, c: Rgb, a: f64) {
    cr.set_source_rgba(c.0, c.1, c.2, a);
    cr.move_to(x, y - r);
    for i in 1..8 {
        let rr = if i % 2 == 0 { r } else { r * 0.3 };
        let ang = -PI / 2.0 + i as f64 * PI / 4.0;
        cr.line_to(x + rr * ang.cos(), y + rr * ang.sin());
    }
    cr.close_path();
    let _ = cr.fill();
}

fn begin(cr: &Context, cx: f64, cy: f64, r: f64, asleep: bool) {
    let _ = cr.save();
    let k = r / 44.0 * 1.18;
    cr.translate(cx, cy + 14.0 * k); // feet rest on the pet's shadow
    cr.scale(k, k);
    if asleep {
        cr.push_group();
    }
}

fn end(cr: &Context, asleep: bool) {
    if asleep {
        let _ = cr.pop_group_to_source();
        let _ = cr.paint_with_alpha(0.65);
    }
    let _ = cr.restore();
}

// ---- Flash -----------------------------------------------------------------------

pub fn flash(cr: &Context, cx: f64, cy: f64, r: f64, t: f64, state: &str, asleep: bool) {
    let (hair, hair_dk) = ((0.99, 0.85, 0.3), (0.85, 0.62, 0.12));
    let (blue, vest, cloak, bolt) = ((0.14, 0.3, 0.5), (0.42, 0.48, 0.56), (0.96, 0.96, 0.97), (0.25, 0.5, 0.95));
    let waiting = state == "waiting" && !asleep;
    begin(cr, cx, cy, r, asleep);
    if waiting {
        // lightning sparks crackle around him
        for i in 0..5 {
            let ph = (t * 0.15 + i as f64 / 5.0).rem_euclid(1.0);
            let a = i as f64 * 1.3 + t * 0.05;
            sparkle(cr, a.cos() * 30.0, -10.0 + a.sin() * 26.0, 4.0 + 3.0 * (1.0 - ph), (1.0, 0.9, 0.3), 1.0 - ph);
        }
    }
    // spiky crown (behind the face)
    shape(
        cr,
        &[
            &[-21.0, -16.0], &[-27.0, -22.0], &[-22.0, -27.0], &[-30.0, -33.0], &[-19.0, -36.0],
            &[-22.0, -46.0], &[-11.0, -41.0], &[-6.0, -52.0], &[1.0, -43.0], &[9.0, -51.0],
            &[12.0, -41.0], &[23.0, -46.0], &[21.0, -35.0], &[30.0, -33.0], &[22.0, -26.0],
            &[27.0, -20.0], &[21.0, -16.0],
        ],
        hair,
    );
    // cloak back panel
    shape(cr, &[&[-11.0, -2.0], &[11.0, -2.0], &[19.0, 31.0], &[-19.0, 31.0]], cloak);
    legs(cr, blue, (0.93, 0.93, 0.93), (0.2, 0.2, 0.3));
    // torso: blue shirt, green vest
    shape(cr, &[&[-10.5, -2.0], &[10.5, -2.0], &[11.5, 18.0], &[-11.5, 18.0]], blue);
    shape(cr, &[&[-7.0, -2.0], &[7.0, -2.0], &[7.5, 16.0], &[-7.5, 16.0]], vest);
    // cloak front panels with a blue lightning zigzag at the hem
    for side in [-1.0, 1.0] {
        let pts: [&[f64]; 5] = [
            &[side * 11.0, -3.0],
            &[side * 7.0, -2.0],
            &[side * 9.0, 31.0],
            &[side * 21.0, 31.0],
            &[side * 13.0, 4.0],
        ];
        shape(cr, &pts, cloak);
        let _ = cr.save();
        path(cr, &pts);
        cr.clip();
        cr.move_to(side * 6.0, 32.0);
        cr.line_to(side * 6.0, 27.0);
        for j in 0..5 {
            let x0 = side * (7.0 + j as f64 * 3.2);
            cr.line_to(x0 + side * 1.6, 23.0);
            cr.line_to(x0 + side * 3.2, 27.0);
        }
        cr.line_to(side * 24.0, 32.0);
        cr.close_path();
        rgb(cr, bolt);
        let _ = cr.fill();
        let _ = cr.restore();
    }
    // arms: a throwing knife raised while working, waving while he needs you
    if waiting {
        arm(cr, -1.0, 0.35, blue, None, 15.0);
    } else if state == "working" && !asleep {
        arm(cr, -1.0, 0.3, blue, None, 15.0);
        let (hx, hy) = arm(cr, 1.0, 1.9 + 0.1 * (t * 0.6).sin(), blue, None, 13.0);
        // a simple throwing knife with a ring on the handle
        let steel = (0.55, 0.58, 0.64);
        let _ = cr.save();
        cr.translate(hx, hy);
        cr.rotate(-0.6);
        shape(cr, &[&[1.0, -1.4], &[11.0, 0.0], &[1.0, 1.4]], steel);
        ellipse(cr, -1.5, 0.0, 1.4, 1.4, steel, true);
        let _ = cr.restore();
    } else {
        arm(cr, -1.0, 0.3, blue, None, 15.0);
        arm(cr, 1.0, 0.3, blue, None, 15.0);
    }
    // high blue collar
    shape(
        cr,
        &[&[-9.0, -1.0], &[-12.0, -8.0], &[-5.0, -3.0], &[5.0, -3.0], &[12.0, -8.0], &[9.0, -1.0]],
        (0.3, 0.5, 0.85),
    );
    face(cr, t, state, asleep, (0.25, 0.55, 0.92), false);
    headband(cr);
    // bangs over the headband and long locks framing the face
    shape(
        cr,
        &[
            &[-20.0, -25.0], &[-15.0, -34.0], &[-9.0, -26.0], &[-5.0, -35.0], &[0.0, -27.0],
            &[5.0, -35.0], &[9.0, -26.0], &[15.0, -34.0], &[20.0, -25.0], &[18.0, -31.0],
            &[6.0, -39.0], &[-6.0, -39.0], &[-18.0, -31.0],
        ],
        hair,
    );
    for side in [-1.0, 1.0] {
        shape(
            cr,
            &[
                &[side * 19.5, -26.0],
                &[side * 21.0, -10.0],
                &[side * 17.5, -3.0],
                &[side * 16.5, -14.0],
                &[side * 15.5, -22.0],
            ],
            hair_dk,
        );
    }
    if waiting {
        // right arm raised beside the head, waving; drawn last so it's in front
        arm(cr, 1.0, 2.25 + 0.3 * (t * 0.8).sin(), blue, None, 17.0);
    }
    end(cr, asleep);
}

// ---- Lavender --------------------------------------------------------------------

pub fn lavender(cr: &Context, cx: f64, cy: f64, r: f64, t: f64, state: &str, asleep: bool) {
    let (hair, hair_hi) = ((0.17, 0.16, 0.36), (0.36, 0.36, 0.66));
    let (jacket, lav, navy) = ((0.92, 0.92, 0.94), (0.72, 0.62, 0.88), (0.26, 0.28, 0.46));
    let waiting = state == "waiting" && !asleep;
    begin(cr, cx, cy, r, asleep);
    // long hair behind, gently swaying
    let sway = (t * 0.15).sin() * 1.5;
    shape(
        cr,
        &[
            &[-20.0, -26.0], &[-24.0, 0.0], &[-22.0 + sway, 14.0], &[-12.0, 10.0], &[12.0, 10.0],
            &[22.0 + sway, 14.0], &[24.0, 0.0], &[20.0, -26.0],
        ],
        hair,
    );
    legs(cr, navy, navy, (0.25, 0.22, 0.3));
    // jacket: cream with lavender panels
    shape(cr, &[&[-11.0, -2.0], &[11.0, -2.0], &[13.0, 19.0], &[-13.0, 19.0]], jacket);
    shape(cr, &[&[-5.0, -2.0], &[5.0, -2.0], &[6.0, 19.0], &[-6.0, 19.0]], lav);
    rgb(cr, OUTLINE);
    cr.set_line_width(0.6);
    cr.move_to(0.0, -1.0);
    cr.line_to(0.0, 19.0);
    let _ = cr.stroke();
    if waiting {
        // shy: index fingers pressed together in front of her chest
        let wig = (t * 0.9).sin() * 0.8;
        for side in [-1.0, 1.0] {
            limb(cr, side * 10.5, 0.0, side * (2.5 + wig), 9.0, 6.5, jacket);
            limb(cr, side * 6.0, 5.5, side * (2.6 + wig), 9.0, 6.9, lav);
            ellipse(cr, side * (1.8 + wig), 9.5, 2.6, 2.6, SKIN, true);
        }
    } else {
        for side in [-1.0, 1.0] {
            arm(cr, side, 0.25, jacket, Some(lav), 15.0);
        }
    }
    // hood collar and a ribbon bow at the neck
    shape(cr, &[&[-12.0, -1.0], &[-10.0, -6.0], &[10.0, -6.0], &[12.0, -1.0], &[0.0, 2.0]], jacket);
    let ribbon = (0.95, 0.55, 0.72);
    shape(cr, &[&[0.0, -2.5], &[-6.0, -5.5], &[-6.0, 0.5]], ribbon);
    shape(cr, &[&[0.0, -2.5], &[6.0, -5.5], &[6.0, 0.5]], ribbon);
    ellipse(cr, 0.0, -2.5, 1.6, 1.6, ribbon, true);
    face(cr, t, state, asleep, (0.9, 0.87, 0.96), true);
    blush(cr, if waiting { 0.75 } else { 0.35 });
    if state == "working" && !asleep {
        // focused: little sparkles twinkle beside her eyes
        for (i, side) in [-1.0, 1.0].into_iter().enumerate() {
            let a = 0.5 + 0.5 * (t * 0.5 + i as f64 * 1.7).sin();
            sparkle(cr, side * 15.5, -23.0, 2.2 + 1.2 * a, (0.8, 0.7, 1.0), 0.5 + 0.5 * a);
        }
    }
    // straight hime-cut bangs and side locks
    shape(
        cr,
        &[
            &[-20.0, -24.0],
            &[-20.0, -32.0, -11.0, -39.0, 0.0, -39.0],
            &[11.0, -39.0, 20.0, -32.0, 20.0, -24.0],
            &[20.0, -23.5],
            &[-20.0, -23.5],
        ],
        hair,
    );
    rgb(cr, hair_hi);
    cr.set_line_width(0.8);
    for x in [-13.0, -6.0, 1.0, 8.0, 14.0] {
        cr.move_to(x, -24.5);
        cr.line_to(x + 0.5, -29.0);
    }
    let _ = cr.stroke();
    path(cr, &[&[-10.0, -35.0], &[0.0, -37.5], &[10.0, -35.0], &[0.0, -36.0]]); // shine
    fill(cr, hair_hi, false);
    for side in [-1.0, 1.0] {
        shape(
            cr,
            &[&[side * 20.0, -26.0], &[side * 21.5, -2.0], &[side * 17.0, -1.0], &[side * 16.5, -23.0]],
            hair,
        );
    }
    end(cr, asleep);
}
