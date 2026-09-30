//! Image pets: your own artwork as a pet (`claude-pet add NAME IMAGE`).
//!
//! The image is prepared once: shrunk, its plain background removed (flood
//! fill from the edges, with softened edges) or, if the background is busy,
//! kept whole to be shown as a rounded portrait card. The result is saved to
//! ~/.claude/pet/custom/NAME.png with NAME.json alongside. Images stay on the
//! user's machine; nothing is bundled with claude-pet.

use crate::state::pet_dir;
use gtk::gdk_pixbuf::{Colorspace, InterpType, Pixbuf};
use gtk::glib;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

/// Prepared images are at most this tall (px): crisp at every pet size.
const MAX_H: i32 = 480;
/// How different (sum of RGB differences) a pixel may be from the background
/// and still be cleared.
const TOLERANCE: i32 = 60;
/// A background counts as plain when this share of the border is within
/// PLAIN_TOLERANCE of its average colour (a white studio backdrop scores ~95%,
/// a painted scene ~40%).
const PLAIN_SHARE: f64 = 0.9;
const PLAIN_TOLERANCE: i32 = 30;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Meta {
    /// "cutout" (transparent background) or "card" (whole picture)
    pub mode: String,
    /// Colour for the aura and usage meter, picked from the artwork.
    pub accent: [f64; 3],
    /// Name of the usage meter ("Energy", "Chakra", ...).
    pub meter: String,
}

pub fn dir() -> PathBuf {
    pet_dir().join("custom")
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 32 && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Names of all image pets, sorted.
pub fn names() -> Vec<String> {
    let mut out: Vec<String> = fs::read_dir(dir())
        .map(|d| {
            d.flatten()
                .filter_map(|e| {
                    let p = e.path();
                    (p.extension()? == "json").then(|| p.file_stem()?.to_str().map(String::from))?
                })
                .filter(|n| dir().join(format!("{n}.png")).exists())
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

pub fn exists(name: &str) -> bool {
    valid_name(name) && dir().join(format!("{name}.json")).exists() && dir().join(format!("{name}.png")).exists()
}

pub fn load(name: &str) -> Option<(Pixbuf, Meta)> {
    if !exists(name) {
        return None;
    }
    let meta: Meta = serde_json::from_str(&fs::read_to_string(dir().join(format!("{name}.json"))).ok()?).ok()?;
    let pb = Pixbuf::from_file(dir().join(format!("{name}.png"))).ok()?;
    Some((pb, meta))
}

pub fn remove(name: &str) -> bool {
    let a = fs::remove_file(dir().join(format!("{name}.png"))).is_ok();
    let b = fs::remove_file(dir().join(format!("{name}.json"))).is_ok();
    a || b
}

/// Prepare `image` and save it as pet `name`. Returns the mode used.
pub fn add(name: &str, image: &Path, meter: &str) -> Result<Meta, String> {
    let src = Pixbuf::from_file(image).map_err(|e| format!("cannot read {}: {e}", image.display()))?;
    let (pb, mode) = prepare(&src)?;
    let meta = Meta { mode: mode.into(), accent: accent(&pb), meter: meter.into() };
    fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
    pb.savev(dir().join(format!("{name}.png")), "png", &[]).map_err(|e| e.to_string())?;
    let json = serde_json::to_string(&meta).map_err(|e| e.to_string())?;
    fs::write(dir().join(format!("{name}.json")), json).map_err(|e| e.to_string())?;
    Ok(meta)
}

/// RGBA pixels we can edit, plus their geometry.
struct Pixels {
    data: Vec<u8>,
    w: usize,
    h: usize,
}

impl Pixels {
    fn from(pb: &Pixbuf) -> Result<Self, String> {
        let pb = pb.add_alpha(false, 0, 0, 0).map_err(|e| e.to_string())?;
        let (w, h, rs) = (pb.width() as usize, pb.height() as usize, pb.rowstride() as usize);
        let bytes = pb.read_pixel_bytes();
        let mut data = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            data.extend_from_slice(&bytes[y * rs..y * rs + w * 4]);
        }
        Ok(Pixels { data, w, h })
    }

    fn px(&self, x: usize, y: usize) -> [u8; 4] {
        let i = (y * self.w + x) * 4;
        [self.data[i], self.data[i + 1], self.data[i + 2], self.data[i + 3]]
    }

    fn set_alpha(&mut self, x: usize, y: usize, a: u8) {
        self.data[(y * self.w + x) * 4 + 3] = a;
    }

    fn border(&self) -> Vec<(usize, usize)> {
        let mut b = Vec::new();
        for x in 0..self.w {
            b.push((x, 0));
            b.push((x, self.h - 1));
        }
        for y in 1..self.h - 1 {
            b.push((0, y));
            b.push((self.w - 1, y));
        }
        b
    }

    fn into_pixbuf(self) -> Pixbuf {
        let rs = (self.w * 4) as i32;
        Pixbuf::from_bytes(&glib::Bytes::from_owned(self.data), Colorspace::Rgb, true, 8, self.w as i32, self.h as i32, rs)
    }
}

fn dist(p: [u8; 4], bg: [i32; 3]) -> i32 {
    (p[0] as i32 - bg[0]).abs() + (p[1] as i32 - bg[1]).abs() + (p[2] as i32 - bg[2]).abs()
}

/// Shrink, then remove a plain background (or decide on a card), then trim.
fn prepare(src: &Pixbuf) -> Result<(Pixbuf, &'static str), String> {
    let (w, h) = (src.width(), src.height());
    if w < 8 || h < 8 {
        return Err("image is too small".into());
    }
    let src = if h > MAX_H {
        let nw = ((w as f64) * MAX_H as f64 / h as f64).round().max(1.0) as i32;
        src.scale_simple(nw, MAX_H, InterpType::Hyper).ok_or("could not scale the image")?
    } else {
        src.clone()
    };
    let mut p = Pixels::from(&src)?;
    let border = p.border();
    let n = border.len() as f64;

    let transparent = border.iter().filter(|&&(x, y)| p.px(x, y)[3] < 30).count() as f64 / n;
    if transparent < 0.6 {
        // is the background one plain colour?
        let mut sum = [0i64; 3];
        for &(x, y) in &border {
            let c = p.px(x, y);
            for k in 0..3 {
                sum[k] += c[k] as i64;
            }
        }
        let bg = [0, 1, 2].map(|k| (sum[k] as f64 / n) as i32);
        let plain = border.iter().filter(|&&(x, y)| dist(p.px(x, y), bg) < PLAIN_TOLERANCE).count() as f64 / n;
        if plain < PLAIN_SHARE {
            return Ok((src, "card")); // busy background: show the whole picture
        }
        flood_clear(&mut p, &border, bg);
    }
    let trimmed = trim(p);
    Ok((trimmed, "cutout"))
}

/// Clear background pixels connected to the edges, then soften the cut edge.
fn flood_clear(p: &mut Pixels, border: &[(usize, usize)], bg: [i32; 3]) {
    let (w, h) = (p.w, p.h);
    let mut cleared = vec![false; w * h];
    let mut queue: VecDeque<(usize, usize)> = VecDeque::new();
    for &(x, y) in border {
        if dist(p.px(x, y), bg) < TOLERANCE && !cleared[y * w + x] {
            cleared[y * w + x] = true;
            queue.push_back((x, y));
        }
    }
    while let Some((x, y)) = queue.pop_front() {
        p.set_alpha(x, y, 0);
        let mut visit = |nx: usize, ny: usize| {
            let i = ny * w + nx;
            if !cleared[i] && dist(p.px(nx, ny), bg) < TOLERANCE {
                cleared[i] = true;
                queue.push_back((nx, ny));
            }
        };
        if x > 0 {
            visit(x - 1, y);
        }
        if x + 1 < w {
            visit(x + 1, y);
        }
        if y > 0 {
            visit(x, y - 1);
        }
        if y + 1 < h {
            visit(x, y + 1);
        }
    }
    // anti-aliased edge: pixels next to the cleared area fade by how close they are to the background
    for y in 0..h {
        for x in 0..w {
            if cleared[y * w + x] {
                continue;
            }
            let near = (x > 0 && cleared[y * w + x - 1])
                || (x + 1 < w && cleared[y * w + x + 1])
                || (y > 0 && cleared[(y - 1) * w + x])
                || (y + 1 < h && cleared[(y + 1) * w + x]);
            if near {
                let d = dist(p.px(x, y), bg);
                let keep = ((d - TOLERANCE) as f64 / 90.0).clamp(0.35, 1.0);
                let a = p.px(x, y)[3];
                p.set_alpha(x, y, (a as f64 * keep) as u8);
            }
        }
    }
}

/// Crop to the visible pixels, with a small margin.
fn trim(p: Pixels) -> Pixbuf {
    let (mut x0, mut y0, mut x1, mut y1) = (p.w, p.h, 0, 0);
    for y in 0..p.h {
        for x in 0..p.w {
            if p.px(x, y)[3] > 16 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    let pb = p.into_pixbuf();
    if x1 < x0 || y1 < y0 {
        return pb;
    }
    let (x0, y0) = (x0.saturating_sub(2), y0.saturating_sub(2));
    let (x1, y1) = ((x1 + 2).min(pb.width() as usize - 1), (y1 + 2).min(pb.height() as usize - 1));
    pb.new_subpixbuf(x0 as i32, y0 as i32, (x1 - x0 + 1) as i32, (y1 - y0 + 1) as i32).copy().unwrap_or(pb)
}

/// The artwork's most characteristic colour: an average weighted toward
/// saturated pixels, brightened so it reads on the dark usage box.
fn accent(pb: &Pixbuf) -> [f64; 3] {
    let Ok(p) = Pixels::from(pb) else { return [0.55, 0.85, 0.45] };
    let (mut acc, mut total) = ([0f64; 3], 0f64);
    for y in (0..p.h).step_by(2) {
        for x in (0..p.w).step_by(2) {
            let c = p.px(x, y);
            if c[3] < 200 {
                continue;
            }
            let (mx, mn) = (c[0].max(c[1]).max(c[2]) as f64, c[0].min(c[1]).min(c[2]) as f64);
            let sat = if mx > 0.0 { (mx - mn) / mx } else { 0.0 };
            let weight = sat * sat * (mx / 255.0);
            for k in 0..3 {
                acc[k] += c[k] as f64 * weight;
            }
            total += weight;
        }
    }
    if total < 1e-6 {
        return [0.55, 0.85, 0.45];
    }
    let c = acc.map(|v| v / total / 255.0);
    let peak = c[0].max(c[1]).max(c[2]).max(1e-6);
    c.map(|v| (v / peak * 0.95).clamp(0.2, 1.0)) // brighten to full strength
}
