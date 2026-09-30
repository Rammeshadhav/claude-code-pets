"""Vector (resolution-independent) pets drawn with cairo.

Each pet is a function (cr, cx, cy, R, t, state, asleep) that draws a ball of
radius R centred on (cx, cy). `t` is the animation tick, `state` is one of
idle | working | waiting | done.
"""
import math

import cairo

TAU = 2 * math.pi


def pol(r, deg):
    a = math.radians(deg)
    return r * math.cos(a), r * math.sin(a)


def speed(state, asleep, idle=0.012, working=0.14, waiting=0.22, done=0.035):
    if asleep:
        return 0.0
    return {"working": working, "waiting": waiting, "done": done}.get(state, idle)


def glow(cr, cx, cy, r, rgb, alpha):
    g = cairo.RadialGradient(cx, cy, r * 0.2, cx, cy, r)
    g.add_color_stop_rgba(0, *rgb, alpha)
    g.add_color_stop_rgba(1, *rgb, 0)
    cr.set_source(g)
    cr.arc(cx, cy, r, 0, TAU)
    cr.fill()


def pulse(t, rate):
    return 0.5 + 0.5 * math.sin(t * rate)


def sphere_shade(cr, cx, cy, R, dark=0.45):
    """Darken the rim so a flat disc reads as a ball."""
    g = cairo.RadialGradient(cx - R * 0.3, cy - R * 0.35, R * 0.1, cx, cy, R)
    g.add_color_stop_rgba(0, 1, 1, 1, 0.0)
    g.add_color_stop_rgba(0.7, 0, 0, 0, 0.0)
    g.add_color_stop_rgba(1, 0, 0, 0, dark)
    cr.set_source(g)
    cr.arc(cx, cy, R, 0, TAU)
    cr.fill()


def dim(cr, cx, cy, R, alpha=0.45):
    cr.arc(cx, cy, R, 0, TAU)
    cr.set_source_rgba(0.05, 0.03, 0.08, alpha)
    cr.fill()


def tomoe(cr, cx, cy, orbit, theta, h, rgb=(0.03, 0.0, 0.01)):
    """Comma-shaped tomoe on an orbit; the tail trails the spin."""
    cr.save()
    cr.translate(cx + orbit * math.cos(theta), cy + orbit * math.sin(theta))
    cr.rotate(theta)
    cr.set_source_rgb(*rgb)
    cr.arc(0, 0, h, 0, TAU)
    cr.fill()
    cr.move_to(h, 0)
    cr.curve_to(h * 1.1, -h * 1.4, h * 0.2, -h * 2.6, -h * 0.9, -h * 3.0)
    cr.curve_to(-h * 0.1, -h * 2.0, -h * 0.6, -h * 0.9, -h, 0)
    cr.close_path()
    cr.fill()
    cr.restore()


# ---- crimson eye -----------------------------------------------------------
def crimson_eye(cr, cx, cy, R, t, state, asleep):
    ang = t * speed(state, asleep)
    a = {"waiting": 0.5, "working": 0.28}.get(state, 0.15) * (0.6 + 0.4 * pulse(t, 0.3))
    glow(cr, cx, cy, R * 1.45, (0.95, 0.05, 0.05), 0 if asleep else a)

    g = cairo.RadialGradient(cx - R * 0.25, cy - R * 0.3, R * 0.1, cx, cy, R)
    g.add_color_stop_rgb(0, 1.0, 0.24, 0.2)
    g.add_color_stop_rgb(0.55, 0.86, 0.03, 0.05)
    g.add_color_stop_rgb(1, 0.42, 0.0, 0.02)
    cr.arc(cx, cy, R, 0, TAU)
    cr.set_source(g)
    cr.fill()

    if state != "waiting" or asleep:
        scythe_pattern(cr, cx, cy, R, ang)
    else:  # your turn: the classic three tomoe on the perimeter
        cr.arc(cx, cy, R * 0.66, 0, TAU)
        cr.set_source_rgba(0.1, 0.0, 0.0, 0.85)
        cr.set_line_width(R * 0.025)
        cr.stroke()
        cr.arc(cx, cy, R * 0.2, 0, TAU)
        cr.set_source_rgb(0.03, 0.0, 0.01)
        cr.fill()
        for k in range(3):
            tomoe(cr, cx, cy, R * 0.66, ang + k * TAU / 3, R * 0.15)

    sphere_shade(cr, cx, cy, R)
    cr.arc(cx, cy, R, 0, TAU)
    cr.set_source_rgb(0.06, 0.0, 0.0)
    cr.set_line_width(R * 0.07)
    cr.stroke()
    if asleep:
        dim(cr, cx, cy, R)


def scythe_pattern(cr, cx, cy, R, ang):
    """Three curved scythe blades from a black core with a red pupil; a thin
    line runs alongside each blade's outer (convex) edge."""
    ink = (0.04, 0.0, 0.01)
    cr.save()
    cr.translate(cx, cy)
    cr.scale(R, R)  # draw in units of the iris radius
    cr.rotate(ang)
    cr.set_source_rgb(*ink)
    for k in range(3):
        cr.save()
        cr.rotate(k * TAU / 3)
        # blade: straight-ish leading edge out to a sharp tip, bulging trailing edge back
        cr.move_to(0.15, -0.13)
        cr.curve_to(0.45, -0.08, 0.75, -0.1, 0.95, -0.38)
        cr.curve_to(0.93, 0.0, 0.7, 0.4, 0.1, 0.2)
        cr.close_path()
        cr.fill()
        # thin line following the bulge, from near the tip toward the rim
        cr.move_to(0.975, -0.2)
        cr.curve_to(0.98, 0.12, 0.74, 0.47, 0.3, 0.44)
        cr.set_line_width(0.028)
        cr.set_line_cap(cairo.LINE_CAP_ROUND)
        cr.stroke()
        cr.restore()
    cr.arc(0, 0, 0.3, 0, TAU)
    cr.fill()
    cr.arc(0, 0, 0.11, 0, TAU)
    cr.set_source_rgb(0.72, 0.04, 0.05)
    cr.fill()
    cr.restore()


# ---- ripple eye ------------------------------------------------------------
def ripple_eye(cr, cx, cy, R, t, state, asleep):
    alert = state == "waiting" and not asleep  # turns red, ripples race outward
    a = {"waiting": 0.5, "working": 0.28}.get(state, 0.15) * (0.6 + 0.4 * pulse(t, 0.3))
    glow_rgb = (0.95, 0.05, 0.1) if alert else (0.6, 0.35, 0.95)
    glow(cr, cx, cy, R * 1.45, glow_rgb, 0 if asleep else a)

    g = cairo.RadialGradient(cx - R * 0.25, cy - R * 0.3, R * 0.1, cx, cy, R)
    if alert:
        g.add_color_stop_rgb(0, 1.0, 0.26, 0.24)
        g.add_color_stop_rgb(1, 0.55, 0.0, 0.05)
        ring_rgb = (0.12, 0.0, 0.02)
    else:
        g.add_color_stop_rgb(0, 0.90, 0.84, 1.0)
        g.add_color_stop_rgb(1, 0.60, 0.48, 0.85)
        ring_rgb = (0.20, 0.10, 0.32)
    cr.arc(cx, cy, R, 0, TAU)
    cr.set_source(g)
    cr.fill()

    # ripples drift outward while working
    drift = (t * speed(state, asleep, 0.004, 0.03, 0.07, 0.01)) % 1.0
    cr.set_source_rgb(*ring_rgb)
    cr.set_line_width(R * 0.035)
    for i in range(5):
        r = R * (0.22 + 0.17 * (i + drift))
        if r < R * 0.95:
            cr.arc(cx, cy, r, 0, TAU)
            cr.stroke()
    cr.arc(cx, cy, R * 0.13, 0, TAU)
    cr.fill()

    if alert:  # a bright ring pulsing out from the centre
        ph = (t * 0.12) % 1.0
        cr.arc(cx, cy, R * (0.15 + 0.8 * ph), 0, TAU)
        cr.set_source_rgba(1.0, 0.85, 0.8, 0.8 * (1 - ph))
        cr.set_line_width(R * 0.05)
        cr.stroke()

    sphere_shade(cr, cx, cy, R, 0.35)
    cr.arc(cx, cy, R, 0, TAU)
    cr.set_source_rgb(*ring_rgb)
    cr.set_line_width(R * 0.05)
    cr.stroke()
    if asleep:
        dim(cr, cx, cy, R)


# ---- spiral orb ------------------------------------------------------------
def spiral_orb(cr, cx, cy, R, t, state, asleep):
    shuriken = state == "waiting" and not asleep  # grows spinning wind blades
    spd = speed(state, asleep, 0.05, 0.28, 0.35, 0.1)
    if asleep:
        R *= 0.7
    flick = 0.85 + 0.15 * math.sin(t * 1.7)

    glow(cr, cx, cy, R * (2.1 if shuriken else 1.6), (0.3, 0.65, 1.0), 0.18 if asleep else 0.55 * flick)

    if shuriken:
        cr.save()
        cr.translate(cx, cy)
        cr.rotate(t * 0.45)
        for k in range(4):
            cr.save()
            cr.rotate(k * TAU / 4)
            cr.move_to(*pol(R * 0.6, -18))
            cr.curve_to(*pol(R * 1.2, -12), *pol(R * 1.6, 5), *pol(R * 1.75, 22))
            cr.curve_to(*pol(R * 1.4, 12), *pol(R * 1.05, 14), *pol(R * 0.6, 22))
            cr.close_path()
            g = cairo.LinearGradient(R * 0.6, 0, R * 1.75, 0)
            g.add_color_stop_rgba(0, 0.85, 0.95, 1.0, 0.9)
            g.add_color_stop_rgba(1, 0.55, 0.8, 1.0, 0.25)
            cr.set_source(g)
            cr.fill()
            cr.restore()
        cr.restore()

    g = cairo.RadialGradient(cx, cy, 0, cx, cy, R)
    g.add_color_stop_rgba(0, 1, 1, 1, 1)
    g.add_color_stop_rgba(0.25, 0.75, 0.93, 1.0, 0.95)
    g.add_color_stop_rgba(0.7, 0.2, 0.55, 1.0, 0.85)
    g.add_color_stop_rgba(1, 0.1, 0.3, 0.95, 0.35)
    cr.arc(cx, cy, R, 0, TAU)
    cr.set_source(g)
    cr.fill()

    # swirling chakra streaks; inner layers spin faster
    cr.set_line_cap(cairo.LINE_CAP_ROUND)
    for i in range(14):
        r = R * (0.25 + 0.7 * ((i * 0.37) % 1.0))
        start = i * 1.9 + t * spd * (1.6 - r / R)
        span = 0.9 + (i % 3) * 0.35
        cr.arc(cx, cy, r, start, start + span)
        cr.set_source_rgba(1, 1, 1, 0.35 + 0.4 * ((i * 0.53) % 1.0))
        cr.set_line_width(R * (0.03 + 0.03 * (i % 2)))
        cr.stroke()
    # tilted outer rings of wind
    for k, tilt in enumerate((0.35, -0.5)):
        cr.save()
        cr.translate(cx, cy)
        cr.rotate(tilt + t * spd * 0.3 * (1 if k else -1))
        cr.scale(1, 0.32)
        cr.arc(0, 0, R * 1.08, 0, TAU)
        cr.restore()
        cr.set_source_rgba(0.8, 0.93, 1.0, 0.55)
        cr.set_line_width(R * 0.035)
        cr.stroke()
    glow(cr, cx, cy, R * 0.35, (1, 1, 1), 0.9)


# ---- shared shapes -----------------------------------------------------------
def star(cr, x, y, r, rot):
    for i in range(10):
        rr = r if i % 2 == 0 else r * 0.45
        a = rot + i * math.pi / 5 - math.pi / 2
        (cr.move_to if i == 0 else cr.line_to)(x + rr * math.cos(a), y + rr * math.sin(a))
    cr.close_path()


# ---- Robot --------------------------------------------------------------------
# An isometric view of the rover. World axes: x = length (front is +x), y = width,
# z = up, in units of `s`. Visible faces are +x (front), +y (left side) and top.
COS30 = math.cos(math.pi / 6)
ROBOT_L, ROBOT_W = 1.2, 1.0            # chassis length, width
ROBOT_ZB, ROBOT_ZT = 0.12, 0.6         # chassis bottom, top
ROBOT_RW, ROBOT_TW = 0.36, 0.17        # wheel radius, tyre width
ROBOT_WX = 0.38                       # wheel x positions (±)
ROBOT_WY = ROBOT_W / 2 + 0.03          # inner face of the wheels (±)

STATE_LED = {"working": (0.36, 0.62, 1.0), "waiting": (1.0, 0.25, 0.25),
             "done": (0.25, 0.85, 0.45), "idle": (0.85, 0.85, 0.9)}


def _plane(cr, ox, oy, ux, uy, vx, vy):
    """Map a face's 2-D (u, v) coordinates onto the screen."""
    cr.transform(cairo.Matrix(ux, uy, vx, vy, ox, oy))


def robot(cr, cx, cy, R, t, state, asleep):
    s = R * 0.95
    oy0 = cy + 0.5 * s  # wheels rest on the pet's shadow

    def P(x, y, z):
        return cx + (x - y) * COS30 * s, oy0 + (x + y) * 0.5 * s - z * s

    # rocking while it needs you; wheels spin while working
    rock = math.sin(t * 0.5) * 0.06 if state == "waiting" and not asleep else 0.0
    spin = t * speed(state, asleep, 0.0, 0.35, 0.0, 0.0) + (math.sin(t * 0.5) * 0.6 if rock else 0.0)
    cr.save()
    gx, gy = P(0, 0, 0)
    cr.translate(gx, gy)
    cr.rotate(rock)
    cr.translate(-gx, -gy)
    if asleep:
        cr.push_group()

    def wheel(x, y_inner, side):
        """A tyre as a stack of discs from the inner to the outer face."""
        for i in range(7):
            f = i / 6
            y = y_inner + side * ROBOT_TW * f
            cr.save()
            ox, oy = P(x, y, ROBOT_RW)
            _plane(cr, ox, oy, COS30 * s, 0.5 * s, 0, -s)  # the wheel's (x, z) plane
            cr.arc(0, 0, ROBOT_RW, 0, TAU)
            if i < 6:
                shade = 0.52 + 0.14 * f
                cr.set_source_rgb(shade, shade, shade + 0.01)
                cr.fill()
            else:  # outer face: tyre, rim groove, hub, spin marks
                cr.set_source_rgb(0.80, 0.80, 0.82)
                cr.fill()
                cr.set_line_width(0.012)
                cr.set_source_rgb(0.45, 0.45, 0.47)
                cr.arc(0, 0, ROBOT_RW * 0.8, 0, TAU)
                cr.stroke()
                cr.arc(0, 0, ROBOT_RW * 0.62, 0, TAU)
                cr.set_source_rgb(0.68, 0.68, 0.70)
                cr.fill()
                cr.set_source_rgb(0.40, 0.40, 0.43)
                cr.set_line_width(0.03)
                cr.set_line_cap(cairo.LINE_CAP_ROUND)
                for k in range(3):
                    a = spin + k * TAU / 3
                    cr.move_to(ROBOT_RW * 0.66 * math.cos(a), ROBOT_RW * 0.66 * math.sin(a))
                    cr.line_to(ROBOT_RW * 0.76 * math.cos(a), ROBOT_RW * 0.76 * math.sin(a))
                cr.stroke()
            cr.restore()

    # far wheels (behind the chassis)
    for x in (-ROBOT_WX, ROBOT_WX):
        wheel(x, -ROBOT_WY - ROBOT_TW, 1)

    L2, W2 = ROBOT_L / 2, ROBOT_W / 2
    bev = 0.07  # chamfer on the top edges

    def poly(pts, rgb):
        cr.move_to(*P(*pts[0]))
        for p in pts[1:]:
            cr.line_to(*P(*p))
        cr.close_path()
        cr.set_source_rgb(*rgb)
        cr.fill_preserve()
        cr.set_source_rgba(0, 0, 0, 0.55)
        cr.set_line_width(max(0.8, s * 0.012))
        cr.stroke()

    # chassis: left side, front, chamfers, top
    zt = ROBOT_ZT - bev
    poly([(-L2, W2, ROBOT_ZB), (L2, W2, ROBOT_ZB), (L2, W2, zt), (-L2, W2, zt)], (0.20, 0.20, 0.22))
    poly([(L2, W2, ROBOT_ZB), (L2, -W2, ROBOT_ZB), (L2, -W2, zt), (L2, W2, zt)], (0.25, 0.25, 0.27))
    poly([(-L2, W2, zt), (L2, W2, zt), (L2 - bev, W2 - bev, ROBOT_ZT), (-L2 + bev, W2 - bev, ROBOT_ZT)], (0.29, 0.29, 0.31))
    poly([(L2, W2, zt), (L2, -W2, zt), (L2 - bev, -W2 + bev, ROBOT_ZT), (L2 - bev, W2 - bev, ROBOT_ZT)], (0.33, 0.33, 0.35))
    poly([(-L2 + bev, -W2 + bev, ROBOT_ZT), (L2 - bev, -W2 + bev, ROBOT_ZT),
          (L2 - bev, W2 - bev, ROBOT_ZT), (-L2 + bev, W2 - bev, ROBOT_ZT)], (0.36, 0.36, 0.38))

    # top plate, drawn in the top plane: u = x, v = y
    cr.save()
    ox, oy = P(0, 0, ROBOT_ZT)
    _plane(cr, ox, oy, COS30 * s, 0.5 * s, -COS30 * s, 0.5 * s)
    pl, pw, pr = L2 - 0.16, W2 - 0.14, 0.12
    cr.new_sub_path()
    cr.arc(pl - pr, -pw + pr, pr, -math.pi / 2, 0)
    cr.arc(pl - pr, pw - pr, pr, 0, math.pi / 2)
    cr.arc(-pl + pr, pw - pr, pr, math.pi / 2, math.pi)
    cr.arc(-pl + pr, -pw + pr, pr, math.pi, 3 * math.pi / 2)
    cr.close_path()
    cr.set_source_rgb(0.40, 0.40, 0.42)
    cr.fill_preserve()
    cr.set_source_rgba(0, 0, 0, 0.6)
    cr.set_line_width(0.012)
    cr.stroke()
    # rows of vent dots along the long edges
    cr.set_source_rgba(0.08, 0.08, 0.09, 0.9)
    for yy in (-pw + 0.09, pw - 0.09):
        for i in range(8):
            cr.arc(-pl + 0.12 + i * (2 * pl - 0.24) / 7, yy, 0.013, 0, TAU)
            cr.fill()
    # hinge latches at the back, two round ports
    cr.set_source_rgb(0.62, 0.62, 0.64)
    for yy in (-0.22, 0.08):
        cr.rectangle(-pl - 0.02, yy, 0.12, 0.13)
        cr.fill()
    for px, py in ((-pl + 0.3, 0.3), (pl - 0.26, -0.26)):
        cr.arc(px, py, 0.07, 0, TAU)
        cr.set_source_rgb(0.14, 0.14, 0.15)
        cr.fill()
        cr.arc(px, py, 0.07, 0, TAU)
        cr.set_source_rgb(0.62, 0.62, 0.64)
        cr.set_line_width(0.018)
        cr.stroke()
    cr.restore()

    # front face: name plate and the status light (u = -y, v = -z)
    cr.save()
    ox, oy = P(L2, W2, zt)
    _plane(cr, ox, oy, COS30 * s, -0.5 * s, 0, s)
    led = STATE_LED.get(state, STATE_LED["idle"])
    if asleep:
        on = 0.0
    elif state == "waiting":
        on = 1.0 if (t * 0.5) % 2 < 1.2 else 0.15  # blink
    elif state == "working":
        on = 0.6 + 0.4 * pulse(t, 0.4)
    elif state == "done":
        on = 1.0
    else:
        on = 0.35 + 0.15 * pulse(t, 0.1)
    lx, ly = ROBOT_W / 2, 0.3
    if on > 0.2:
        g = cairo.RadialGradient(lx, ly, 0, lx, ly, 0.3)
        g.add_color_stop_rgba(0, *led, 0.7 * on)
        g.add_color_stop_rgba(1, *led, 0)
        cr.set_source(g)
        cr.arc(lx, ly, 0.3, 0, TAU)
        cr.fill()
    cr.rectangle(lx - 0.16, ly - 0.025, 0.32, 0.05)
    cr.set_source_rgb(*[0.15 + (c - 0.15) * on for c in led])
    cr.fill()
    cr.restore()

    # near wheels (in front of the chassis)
    for x in (-ROBOT_WX, ROBOT_WX):
        wheel(x, ROBOT_WY, 1)

    if asleep:
        cr.pop_group_to_source()
        cr.paint_with_alpha(0.6)
    cr.restore()


# ---- chibi characters ---------------------------------------------------------
# Drawn in "k units": the origin is the pet centre and 1 unit = R/44 px. The head
# is centred at (0, -20), the torso spans y -2..18, feet rest at y ~37.
OUTLINE = (0.16, 0.12, 0.16)
SKIN = (1.0, 0.87, 0.75)


def _path(cr, pts):
    cr.move_to(*pts[0])
    for p in pts[1:]:
        if len(p) == 6:
            cr.curve_to(*p)
        else:
            cr.line_to(*p)
    cr.close_path()


def _fill(cr, rgb, outline=True):
    cr.set_source_rgb(*rgb)
    if outline:
        cr.fill_preserve()
        cr.set_source_rgb(*OUTLINE)
        cr.set_line_width(1.1)
        cr.set_line_join(cairo.LINE_JOIN_ROUND)
        cr.stroke()
    else:
        cr.fill()


def _shape(cr, pts, rgb, outline=True):
    _path(cr, pts)
    _fill(cr, rgb, outline)


def _ellipse(cr, x, y, rx, ry, rgb, outline=True):
    cr.save()
    cr.translate(x, y)
    cr.scale(rx, ry)
    cr.arc(0, 0, 1, 0, TAU)
    cr.restore()
    _fill(cr, rgb, outline)


def _limb(cr, x1, y1, x2, y2, w, rgb):
    cr.set_line_cap(cairo.LINE_CAP_ROUND)
    for color, width in ((OUTLINE, w + 2.2), (rgb, w)):
        cr.move_to(x1, y1)
        cr.line_to(x2, y2)
        cr.set_source_rgb(*color)
        cr.set_line_width(width)
        cr.stroke()


def _arm(cr, side, angle, sleeve, cuff=None, length=15):
    """An arm from the shoulder; angle 0 = straight down, positive = outward."""
    sx = side * 10.5
    a = angle * side
    hx, hy = sx + math.sin(a) * length, math.cos(a) * length
    _limb(cr, sx, 0, hx, hy, 6.5, sleeve)
    if cuff:
        _limb(cr, sx + (hx - sx) * 0.8, hy * 0.8, hx, hy, 6.8, cuff)
    _ellipse(cr, hx, hy + 1, 3.0, 3.0, SKIN)
    return hx, hy + 1


def _legs(cr, pants, wrap=(0.93, 0.93, 0.93), shoe=(0.2, 0.2, 0.3)):
    for side in (-1, 1):
        x = side * 5.2
        _limb(cr, x, 16, x, 30, 7.2, pants)
        _limb(cr, x, 29, x, 33.5, 6.4, wrap)
        # sandal, toes flaring slightly outward
        right, left = (x + 5, x - 4.4) if side > 0 else (x + 4.4, x - 5)
        _shape(cr, [(x - 4.2, 33), (x + 4.2, 33), (right, 37.5), (left, 37.5)], shoe)


def _eyes(cr, t, state, asleep, iris, pale=False):
    """`pale`: soft lilac eyes with a small, gentle pupil."""
    blink = (t % 37) < 1.6
    for side in (-1, 1):
        ex, ey = side * 7.6, -17.5
        if asleep or blink:
            cr.move_to(ex - 3.6, ey + 0.6)
            cr.curve_to(ex - 1.5, ey + 2.8, ex + 1.5, ey + 2.8, ex + 3.6, ey + 0.6)
            cr.set_source_rgb(*OUTLINE)
            cr.set_line_width(1.2)
            cr.stroke()
            continue
        if state == "done":  # happy ^ ^
            cr.move_to(ex - 3.4, ey + 1.5)
            cr.curve_to(ex - 1.5, ey - 2.8, ex + 1.5, ey - 2.8, ex + 3.4, ey + 1.5)
            cr.set_source_rgb(*OUTLINE)
            cr.set_line_width(1.5)
            cr.stroke()
            continue
        _ellipse(cr, ex, ey, 3.5, 4.4, (1, 1, 1), outline=False)
        if pale:
            _ellipse(cr, ex, ey + 0.4, 2.8, 3.5, iris, outline=False)
            cr.save()
            cr.translate(ex, ey + 0.4)
            cr.scale(2.8, 3.5)
            cr.arc(0, 0, 1, 0, TAU)
            cr.restore()
            cr.set_source_rgba(0.55, 0.5, 0.7, 0.8)
            cr.set_line_width(0.5)
            cr.stroke()
            _ellipse(cr, ex, ey + 0.9, 1.0, 1.4, (0.55, 0.45, 0.72), outline=False)
        else:
            _ellipse(cr, ex, ey + 0.4, 2.7, 3.5, iris, outline=False)
            cr.set_source_rgb(0.05, 0.05, 0.08)
            cr.save()
            cr.translate(ex, ey + 0.8)
            cr.scale(1.2, 1.7)
            cr.arc(0, 0, 1, 0, TAU)
            cr.restore()
            cr.fill()
        _ellipse(cr, ex - 1.0, ey - 1.3, 0.9, 0.9, (1, 1, 1), outline=False)
        # upper lash line
        cr.move_to(ex - 3.9, ey - 1.8)
        cr.curve_to(ex - 2, ey - 5, ex + 2, ey - 5, ex + 3.9, ey - 2.2)
        cr.set_source_rgb(*OUTLINE)
        cr.set_line_width(1.4)
        cr.stroke()


def _mouth(cr, state, asleep):
    cr.set_source_rgb(*OUTLINE)
    cr.set_line_width(0.9)
    cr.set_line_cap(cairo.LINE_CAP_ROUND)
    if state == "done" and not asleep:  # open grin
        cr.move_to(-3.2, -8.5)
        cr.curve_to(-2.5, -5, 2.5, -5, 3.2, -8.5)
        cr.close_path()
        cr.set_source_rgb(0.7, 0.2, 0.25)
        cr.fill_preserve()
        cr.set_source_rgb(*OUTLINE)
        cr.stroke()
    elif state == "waiting" and not asleep:  # "oh!"
        _ellipse(cr, 0, -7.8, 1.3, 1.6, (0.6, 0.2, 0.25))
    else:
        cr.move_to(-2.2, -8.4)
        cr.curve_to(-1, -7, 1, -7, 2.2, -8.4)
        cr.stroke()


def _face(cr, t, state, asleep, iris, pale=False):
    _ellipse(cr, -19.2, -17, 2.4, 3.4, SKIN)  # ears
    _ellipse(cr, 19.2, -17, 2.4, 3.4, SKIN)
    _shape(cr, [(-19.5, -24), (-19.5, -30, -10, -38, 0, -38), (10, -38, 19.5, -30, 19.5, -24),
                (19.5, -12, 11, -2.5, 0, -2.5), (-11, -2.5, -19.5, -12, -19.5, -24)], SKIN)
    _eyes(cr, t, state, asleep, iris, pale)
    _mouth(cr, state, asleep)


def _blush(cr, amount):
    for side in (-1, 1):
        cr.save()
        cr.translate(side * 11.5, -11.2)
        cr.scale(3.4, 1.7)
        cr.arc(0, 0, 1, 0, TAU)
        cr.restore()
        cr.set_source_rgba(1.0, 0.4, 0.5, amount)
        cr.fill()


def _headband(cr, y=-27.5):
    _shape(cr, [(-19.3, y - 2), (19.3, y - 2), (19.6, y + 3), (-19.6, y + 3)], (0.18, 0.28, 0.5))
    _shape(cr, [(-8, y - 3), (8, y - 3), (8, y + 4), (-8, y + 4)], (0.78, 0.8, 0.84))
    # lightning-bolt emblem
    cr.set_source_rgb(0.35, 0.37, 0.42)
    _path(cr, [(1.2, y - 2.2), (-1.6, y + 0.9), (0, y + 0.9), (-1.0, y + 3.2), (1.9, y - 0.2), (0.3, y - 0.2)])
    cr.fill()


def _sparkle(cr, x, y, r, rgb, a):
    cr.set_source_rgba(*rgb, a)
    cr.move_to(x, y - r)
    for i in range(1, 8):
        rr = r if i % 2 == 0 else r * 0.3
        ang = -math.pi / 2 + i * math.pi / 4
        cr.line_to(x + rr * math.cos(ang), y + rr * math.sin(ang))
    cr.close_path()
    cr.fill()


def _begin(cr, cx, cy, R, asleep):
    cr.save()
    k = R / 44 * 1.18
    cr.translate(cx, cy + 14 * k)  # feet rest on the pet's shadow
    cr.scale(k, k)
    if asleep:
        cr.push_group()


def _end(cr, asleep):
    if asleep:
        cr.pop_group_to_source()
        cr.paint_with_alpha(0.65)
    cr.restore()


# ---- Flash: a blond ninja in a white lightning cloak ----------------------------
def flash(cr, cx, cy, R, t, state, asleep):
    hair, hair_dk = (0.99, 0.85, 0.3), (0.85, 0.62, 0.12)
    blue, vest, cloak, bolt = (0.14, 0.3, 0.5), (0.42, 0.48, 0.56), (0.96, 0.96, 0.97), (0.25, 0.5, 0.95)
    waiting = state == "waiting" and not asleep
    _begin(cr, cx, cy, R, asleep)
    if waiting:  # lightning sparks crackle around him
        for i in range(5):
            ph = (t * 0.15 + i / 5) % 1
            a = i * 1.3 + t * 0.05
            _sparkle(cr, math.cos(a) * 30, -10 + math.sin(a) * 26, 4 + 3 * (1 - ph), (1, 0.9, 0.3), 1 - ph)
    # spiky crown (behind the face)
    _shape(cr, [(-21, -16), (-27, -22), (-22, -27), (-30, -33), (-19, -36), (-22, -46), (-11, -41),
                (-6, -52), (1, -43), (9, -51), (12, -41), (23, -46), (21, -35), (30, -33), (22, -26),
                (27, -20), (21, -16)], hair)
    # cloak back panel
    _shape(cr, [(-11, -2), (11, -2), (19, 31), (-19, 31)], cloak)
    _legs(cr, blue)
    # torso: blue shirt, green vest
    _shape(cr, [(-10.5, -2), (10.5, -2), (11.5, 18), (-11.5, 18)], blue)
    _shape(cr, [(-7, -2), (7, -2), (7.5, 16), (-7.5, 16)], vest)
    # cloak front panels with a blue lightning zigzag at the hem
    for side in (-1, 1):
        pts = [(side * 11, -3), (side * 7, -2), (side * 9, 31), (side * 21, 31), (side * 13, 4)]
        _shape(cr, pts, cloak)
        cr.save()
        _path(cr, pts)
        cr.clip()
        cr.move_to(side * 6, 32)
        cr.line_to(side * 6, 27)
        for j in range(5):
            x0 = side * (7 + j * 3.2)
            cr.line_to(x0 + side * 1.6, 23)
            cr.line_to(x0 + side * 3.2, 27)
        cr.line_to(side * 24, 32)
        cr.close_path()
        cr.set_source_rgb(*bolt)
        cr.fill()
        cr.restore()
    # arms: a throwing knife raised while working, waving while he needs you
    if waiting:
        _arm(cr, -1, 0.35, blue)
    elif state == "working" and not asleep:
        _arm(cr, -1, 0.3, blue)
        hx, hy = _arm(cr, 1, 1.9 + 0.1 * math.sin(t * 0.6), blue, length=13)
        # a simple throwing knife with a ring on the handle
        cr.save()
        cr.translate(hx, hy)
        cr.rotate(-0.6)
        _shape(cr, [(1, -1.4), (11, 0), (1, 1.4)], (0.55, 0.58, 0.64))
        _ellipse(cr, -1.5, 0, 1.4, 1.4, (0.55, 0.58, 0.64))
        cr.restore()
    else:
        _arm(cr, -1, 0.3, blue)
        _arm(cr, 1, 0.3, blue)
    # high blue collar
    _shape(cr, [(-9, -1), (-12, -8), (-5, -3), (5, -3), (12, -8), (9, -1)], (0.3, 0.5, 0.85))
    _face(cr, t, state, asleep, (0.25, 0.55, 0.92))
    _headband(cr)
    # bangs over the headband and long locks framing the face
    _shape(cr, [(-20, -25), (-15, -34), (-9, -26), (-5, -35), (0, -27), (5, -35), (9, -26), (15, -34),
                (20, -25), (18, -31), (6, -39), (-6, -39), (-18, -31)], hair)
    for side in (-1, 1):
        _shape(cr, [(side * 19.5, -26), (side * 21, -10), (side * 17.5, -3), (side * 16.5, -14),
                    (side * 15.5, -22)], hair_dk)
    if waiting:  # right arm raised beside the head, waving; drawn last so it's in front
        _arm(cr, 1, 2.25 + 0.3 * math.sin(t * 0.8), blue, length=17)
    _end(cr, asleep)


# ---- Lavender: a shy girl with long indigo hair ---------------------------------
def lavender(cr, cx, cy, R, t, state, asleep):
    hair, hair_hi = (0.17, 0.16, 0.36), (0.36, 0.36, 0.66)
    jacket, lav, navy = (0.92, 0.92, 0.94), (0.72, 0.62, 0.88), (0.26, 0.28, 0.46)
    waiting = state == "waiting" and not asleep
    _begin(cr, cx, cy, R, asleep)
    # long hair behind, gently swaying
    sway = math.sin(t * 0.15) * 1.5
    _shape(cr, [(-20, -26), (-24, 0), (-22 + sway, 14), (-12, 10), (12, 10), (22 + sway, 14),
                (24, 0), (20, -26)], hair)
    _legs(cr, navy, wrap=navy, shoe=(0.25, 0.22, 0.3))
    # jacket: cream with lavender panels
    _shape(cr, [(-11, -2), (11, -2), (13, 19), (-13, 19)], jacket)
    _shape(cr, [(-5, -2), (5, -2), (6, 19), (-6, 19)], lav)
    cr.set_source_rgb(*OUTLINE)
    cr.set_line_width(0.6)
    cr.move_to(0, -1)
    cr.line_to(0, 19)
    cr.stroke()
    if waiting:  # shy: index fingers pressed together in front of her chest
        wig = math.sin(t * 0.9) * 0.8
        for side in (-1, 1):
            _limb(cr, side * 10.5, 0, side * (2.5 + wig), 9, 6.5, jacket)
            _limb(cr, side * 6, 5.5, side * (2.6 + wig), 9, 6.9, lav)
            _ellipse(cr, side * (1.8 + wig), 9.5, 2.6, 2.6, SKIN)
    else:
        for side in (-1, 1):
            _arm(cr, side, 0.25, jacket, cuff=lav)
    # hood collar and a ribbon bow at the neck
    _shape(cr, [(-12, -1), (-10, -6), (10, -6), (12, -1), (0, 2)], jacket)
    ribbon = (0.95, 0.55, 0.72)
    _shape(cr, [(0, -2.5), (-6, -5.5), (-6, 0.5)], ribbon)
    _shape(cr, [(0, -2.5), (6, -5.5), (6, 0.5)], ribbon)
    _ellipse(cr, 0, -2.5, 1.6, 1.6, ribbon)
    _face(cr, t, state, asleep, (0.9, 0.87, 0.96), pale=True)
    _blush(cr, 0.75 if waiting else 0.35)
    if state == "working" and not asleep:  # focused: little sparkles twinkle beside her eyes
        for i, side in enumerate((-1, 1)):
            a = 0.5 + 0.5 * math.sin(t * 0.5 + i * 1.7)
            _sparkle(cr, side * 15.5, -23, 2.2 + 1.2 * a, (0.8, 0.7, 1.0), 0.5 + 0.5 * a)
    # straight hime-cut bangs and side locks
    _shape(cr, [(-20, -24), (-20, -32, -11, -39, 0, -39), (11, -39, 20, -32, 20, -24),
                (20, -23.5), (-20, -23.5)], hair)
    cr.set_source_rgb(*hair_hi)
    cr.set_line_width(0.8)
    for x in (-13, -6, 1, 8, 14):
        cr.move_to(x, -24.5)
        cr.line_to(x + 0.5, -29)
    cr.stroke()
    _shape(cr, [(-10, -35), (0, -37.5), (10, -35), (0, -36)], hair_hi, outline=False)  # shine
    for side in (-1, 1):
        _shape(cr, [(side * 20, -26), (side * 21.5, -2), (side * 17, -1), (side * 16.5, -23)], hair)
    _end(cr, asleep)


PETS = {
    "crimson": crimson_eye,
    "ripple": ripple_eye,
    "spiral": spiral_orb,
    "robot": robot,
    "flash": flash,
    "lavender": lavender,
}
