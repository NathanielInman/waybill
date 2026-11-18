#!/usr/bin/env python3
"""Writes docs/assets/hero.svg: the README's animated hero.

A 13 second loop on the same desktop as the siblings' heroes, under the
floating Platform bar. An agent in a terminal files a decision with
`waybill ask` and the Waybill lamp turns amber with a 1; a click opens the
Desk drawer out of it, with the decision first and its options as buttons;
a click on an option writes the ruling, the lamp goes dim, and the agent's
next `waybill next` picks the task up with the ruling on it. A click
outside closes the drawer.

The drawer is drawn by `draw_drawer()` in its own coordinates at its real
size (460 px wide, as crates/waybill-board/src/drawer.rs sets it), one
state at a time, so the desktop tour in dot-files can port it. The story
stacks one drawer per state and swaps them.

Pure CSS animation inside the SVG, so GitHub plays it in an <img>. With
reduced motion it is a still of the open drawer with the decision waiting.

    python3 docs/assets/hero.py
"""

import textwrap
from dataclasses import dataclass
from pathlib import Path

W, H = 1200, 660
T = 13  # loop seconds

# Nord, matching the Platform waybar and the siblings' heroes.
BG0, BG1 = "#14171c", "#0d0f12"
PILL, PILL_LINE, PILL_ON = "#24262a", "#3a404d", "#3c4651"
BAR_FG, BRIGHT, BAR_DIM = "#c5c8c6", "#d8dee9", "#616e88"
PANEL, PANEL_LINE, WELL = "#1e2024", "#41464e", "#15171a"
TILE, TILE_LINE, HOVER = "#25282c", "#2f3237", "#33373d"
FG, DIM, IDLE = "#c5c8c6", "#7c838e", "#555b64"
FROST, GO, AMBER, ORANGE, RED, PURPLE = "#88c0d0", "#a3be8c", "#ebcb8b", "#d08770", "#bf616a", "#b48ead"
TEAL, BLUE = "#8fbcbb", "#81a1c1"
FONT = "JetBrains Mono, DejaVu Sans Mono, Menlo, Consolas, monospace"

# The README's alt text says the same.
ALT = (
    "A floating waybar of frosted pills: workspaces, the window title, Tannoy's song, then the lamps, where "
    "Waybill's dot is dim. In a terminal an agent runs waybill ask with two options for a launch discount, and the "
    "dot turns amber with a 1 beside it. A click opens the Waybill drawer out of it: three tasks wait on you, the "
    "decision first with its options as buttons, then a foley recording and a backup disk to swap, over a "
    "sparkline of the week's finished tasks. A click on the first option writes the ruling: the decision leaves "
    "the drawer, the lamp goes dim, and in the terminal the agent's waybill next starts that task with the ruling "
    "on it as a visit. A click outside closes the drawer."
)


def esc(s):
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def tx(x, y, s, size=13, fill=FG, weight=400, anchor="start", spacing=0, attrs=""):
    """A <text> element as a string."""
    ls = f' letter-spacing="{spacing}"' if spacing else ""
    attrs = f" {attrs}" if attrs else ""
    return f'<text x="{x:.1f}" y="{y:.1f}" font-size="{size}" fill="{fill}" font-weight="{weight}" text-anchor="{anchor}"{ls}{attrs}>{esc(s)}</text>'


def text_w(s, size):
    """A monospace advance: 0.6em a character."""
    return len(s) * size * 0.6


def clip(s, size, width):
    """`s`, ending in an ellipsis if it would run past `width`."""
    cols = int(width / (size * 0.6))
    return s if len(s) <= cols else s[: max(1, cols - 1)] + "…"


# ---------------------------------------------------------------- the drawer
# crates/waybill-board: drawer.rs builds it, style.css sizes it. Everything
# below is in the drawer's own coordinates, (0, 0) at the panel's top left.

# What each lamp state looks like in the bar: (color, the text after the
# dot). lamp.rs and bar.format.
LAMPS = {
    "calm": (BAR_DIM, ""),
    "needs": (AMBER, "1"),
}


@dataclass(frozen=True)
class Task:
    title: str
    project: str
    color: str  # the project's dot
    priority: str = "M"
    frees: int = 0


@dataclass(frozen=True)
class Decision:
    title: str
    project: str
    color: str
    priority: str = "M"
    options: tuple = ()


@dataclass(frozen=True)
class Desk:
    lamp: str = "needs"  # a LAMPS key
    counts: str = "13 ready · 4 doing"
    waiting: str = "3 tasks wait on you."
    holding: str = ""  # "1 of them holds up 3 others.", in amber
    decisions: tuple = ()
    tasks: tuple = ()
    more: int = 0
    week: str = "4 finished today, 107 this week."
    said: str = ""  # what the last click did
    spark: tuple = (9, 14, 11, 6, 5, 16, 21, 18, 23, 12, 7, 19, 22, 4)


FOLEY = Task("Record the bell chime foley", "moonlight", FROST, "M")
BACKUP = Task("Swap the failing backup disk", "homelab", TEAL, "H")
DISCOUNT = Decision("Pick the launch discount", "storefront", PURPLE, "M",
                    ("10% for the first week", "No offer at launch"))
# The story's two moments, for the hero here and the dot-files tour. The
# decision is the only thing holding work up, so ruling on it calms the lamp.
ASKED = Desk(decisions=(DISCOUNT,), tasks=(FOLEY, BACKUP))
RULED = Desk(lamp="calm", waiting="2 tasks wait on you.", decisions=(), tasks=(FOLEY, BACKUP),
             said="Ruled: 10% for the first week")

DRAWER_W = 460  # drawer.rs WIDTH
P = 15  # the panel's border and padding
IW = DRAWER_W - 2 * P
GAP = 10  # between the panel's rows
HEADER_H, ROW_H, OPT_H, TILE_H, SAID_H = 28, 46, 26, 56, 14
HEAD_SIZE, HEAD_COLS = 15.5, 44


def _headline(d):
    return textwrap.wrap(f"{d.waiting} {d.holding}".strip(), HEAD_COLS)


def _chip(o, x, y, s, color, fill_op=0.16):
    w = text_w(s, 10.5) + 14
    o.append(f'<rect x="{x:.1f}" y="{y - 11}" width="{w:.1f}" height="15" rx="7.5" fill="{color}" fill-opacity="{fill_op}"/>')
    o.append(tx(x + w / 2, y, s, 10.5, color, 700, "middle"))
    return w


def _row(o, spots, name, y, mark, mark_color, t):
    """A task's row: its mark, title, project and chips."""
    o.append(f'<rect x="{P}" y="{y}" width="{IW}" height="{ROW_H}" rx="10" fill="{TILE}"/>')
    o.append(tx(P + 22, y + 29 if len(mark) == 1 and mark.isdigit() else y + 28, mark,
                18 if mark.isdigit() else 13, mark_color, 700, "middle"))
    x = P + 44
    o.append(tx(x, y + 19, clip(t.title, 13, IW - 56), 13, BRIGHT))
    o.append(f'<circle cx="{x + 3.5}" cy="{y + 33}" r="3.2" fill="{t.color}"/>')
    o.append(tx(x + 12, y + 37, t.project, 11.5, DIM))
    cx = x + 12 + text_w(t.project, 11.5) + 8
    pri = {"H": RED, "M": FG, "L": DIM}[t.priority]
    cx += _chip(o, cx, y + 37, t.priority, pri, 0.18 if t.priority == "H" else 0.08) + 6
    if getattr(t, "frees", 0):
        _chip(o, cx, y + 37, f"frees {t.frees}", AMBER)
    spots[name] = (P, y, IW, ROW_H)


def drawer_height(d):
    h = P + HEADER_H + GAP + len(_headline(d)) * 20 + GAP
    for dec in d.decisions:
        h += ROW_H + 5 + (OPT_H + 5 if dec.options else 0)
    h += len(d.tasks) * (ROW_H + 5)
    if d.more:
        h += 16
    h += GAP - 5 + TILE_H
    if d.said:
        h += GAP + SAID_H
    return h + P


def draw_drawer(d):
    """One state of the drawer. Returns (svg, height, spots): spots name what
    a story can click, as (x, y, w, h) in drawer coordinates: board (the
    Open the board button), decision:<i> and task:<i> (the rows), and
    option:<i> (the first decision's option buttons)."""
    o, spots = [], {}
    h = drawer_height(d)
    o.append(f'<rect width="{DRAWER_W}" height="{h}" rx="16" fill="{PANEL}" stroke="{PANEL_LINE}" filter="url(#shadow)"/>')
    color = AMBER if d.lamp == "needs" else GO
    y = P
    o.append(f'<circle cx="{P + 5}" cy="{y + 14}" r="4.5" fill="{color}"/>')
    o.append(tx(P + 20, y + 18.5, "WAYBILL", 12.5, BRIGHT, 700, spacing=3))
    o.append(tx(P + 108, y + 18.5, d.counts, 11.5, DIM))
    bw = text_w("Open the board", 12) + 22
    o.append(f'<rect x="{P + IW - bw:.1f}" y="{y + 1.5}" width="{bw:.1f}" height="{HEADER_H - 3}" rx="8" fill="{HOVER}" fill-opacity=".55" stroke="{PANEL_LINE}"/>')
    o.append(tx(P + IW - bw / 2, y + 18.5, "Open the board", 12, FG, 400, "middle"))
    spots["board"] = (P + IW - bw, y, bw, HEADER_H)
    y += HEADER_H + GAP
    # The headline: what waits in white, what it holds up in amber.
    at = 0
    for line in _headline(d):
        white = max(0, min(len(line), len(d.waiting) - at))
        o.append(f'<text x="{P + 2}" y="{y + 15}" font-size="{HEAD_SIZE}" font-weight="700">'
                 f'<tspan fill="{BRIGHT}">{esc(line[:white])}</tspan><tspan fill="{AMBER}">{esc(line[white:])}</tspan></text>')
        at += len(line) + 1
        y += 20
    y += GAP
    for i, dec in enumerate(d.decisions):
        _row(o, spots, f"decision:{i}", y, "◆", PURPLE, dec)
        y += ROW_H + 5
        if dec.options:
            x = P + 10
            for k, option in enumerate(dec.options):
                w = text_w(option, 12) + 20
                o.append(f'<rect x="{x:.1f}" y="{y + .5}" width="{w:.1f}" height="{OPT_H - 1}" rx="8" fill="{PURPLE}" fill-opacity=".13" stroke="{PURPLE}" stroke-opacity=".55"/>')
                o.append(tx(x + w / 2, y + 17.5, option, 12, BRIGHT, 400, "middle"))
                if i == 0:
                    spots[f"option:{k}"] = (x, y, w, OPT_H)
                x += w + 6
            y += OPT_H + 5
    for i, t in enumerate(d.tasks):
        mark, mark_color = (str(t.frees), AMBER) if t.frees else ("■", AMBER)
        _row(o, spots, f"task:{i}", y, mark, mark_color, t)
        y += ROW_H + 5
    if d.more:
        o.append(tx(P + 2, y + 10, f"and {d.more} more on the board", 11, DIM))
        y += 16
    y += GAP - 5
    # The week: finished per day, and the count.
    o.append(f'<rect x="{P}" y="{y}" width="{IW}" height="{TILE_H}" rx="12" fill="{TILE}" stroke="{TILE_LINE}"/>')
    sx, sy, sw, sh = P + 14, y + 13, 150, 30
    top = max(d.spark) or 1
    pts = [(sx + sw * i / (len(d.spark) - 1), sy + sh - sh * v / top) for i, v in enumerate(d.spark)]
    path = f"M{pts[0][0]:.1f},{pts[0][1]:.1f}"
    for (px, py), (qx, qy) in zip(pts, pts[1:]):
        mx = (px + qx) / 2
        path += f" C{mx:.1f},{py:.1f} {mx:.1f},{qy:.1f} {qx:.1f},{qy:.1f}"
    o.append(f'<path d="{path} L{pts[-1][0]:.1f},{sy + sh} L{sx},{sy + sh} Z" fill="{GO}" fill-opacity=".16"/>')
    o.append(f'<path d="{path}" fill="none" stroke="{GO}" stroke-width="1.6"/>')
    o.append(f'<circle cx="{pts[-1][0]:.1f}" cy="{pts[-1][1]:.1f}" r="2.4" fill="{GO}"/>')
    for k, line in enumerate(textwrap.wrap(d.week, 30)):
        o.append(tx(sx + sw + 16, y + 24 + k * 15, line, 11.5, FG))
    y += TILE_H
    if d.said:
        o.append(tx(P + 2, y + GAP + 10, d.said, 11.5, GO))
    return "".join(o), h, spots


def center(spot):
    x, y, w, h = spot
    return x + w / 2, y + h / 2


# ---- end of the panel ----
# dot-files' bar hero (docs/bar/apps.py) runs everything above this line
# to draw the panel in its own tour, so keep the panel's code above it and
# the story below it.

# ---------------------------------------------------------------- animation

# The story, in seconds.
ASK = 1.2  # the agent's `waybill ask` lands
CLICK_LAMP, OPEN = 2.6, 2.68
CLICK_OPTION = 4.8
NEXT = 6.6  # the agent runs `waybill next`
CLICK_OUT, CLOSED = 10.4, 10.85
RESET = 12.2
LAG = 0.1  # a click becomes a task command, and the list is read again

out, kfs = [], []


def add(s):
    out.append(s)


def pct(sec):
    return f"{max(0.0, min(100.0, sec / T * 100)):.2f}%"


def frames(pts):
    """A keyframe set from (seconds, declarations) points; returns its name."""
    name = f"k{len(kfs)}"
    rows = " ".join(f"{pct(t)} {{ {d}; }}" for t, d in pts)
    kfs.append(f"@keyframes {name} {{ {rows} }}")
    return name


def anim(prop, pts):
    return frames([(t, f"{prop}: {v}") for t, v in pts])


def fade(spans, ramp=0.16, out_ramp=None):
    """Opacity keyframes visible during each (start, end) span."""
    out_ramp = ramp if out_ramp is None else out_ramp
    pts = []
    for s, e in spans:
        if s <= 0:
            pts.append((0, 1))
        else:
            pts += [(0, 0)] if not pts else []
            pts += [(s, 0), (s + ramp, 1)]
        if e >= T:
            pts.append((T, 1))
        else:
            pts += [(e, 1), (e + out_ramp, 0)]
    if pts[-1][0] < T:
        pts.append((T, 0))
    return anim("opacity", pts)


def style(*names, extra=""):
    return f'class="a" style="animation-name:{",".join(names)};{extra}"'


def group(name, shown=True, extra=""):
    """A group that fades by `name`; `shown` is its state in the still."""
    add(f'<g {style(name, extra=extra)} opacity="{1 if shown else 0}">')


def press(t):
    """A button's flash under a click."""
    return anim("opacity", [(0, 0), (t - 0.01, 0), (t, 1), (t + 0.3, 0), (T, 0)])


# ---------------------------------------------------------------- scene

css = f"""
text {{ white-space: pre; }}
.a {{ animation-duration: {T}s; animation-iteration-count: infinite; animation-timing-function: linear; animation-fill-mode: both; }}
@media (prefers-reduced-motion: reduce) {{
  .a {{ animation: none !important; }}
  #cursor, .ripple {{ display: none; }}
}}
"""


def song_defs():
    """Tannoy's song cover, for the media pill."""
    lanterns = ""
    for x, y, s in [(26, 40, 1.0), (54, 28, 1.25), (78, 46, 0.85)]:
        lanterns += (f'<line x1="{x}" y1="0" x2="{x}" y2="{y - 9 * s:.1f}" stroke="#2e2235" stroke-width="1.2"/>'
                     f'<circle cx="{x}" cy="{y}" r="{18 * s:.1f}" fill="url(#glow)"/>'
                     f'<rect x="{x - 6 * s:.1f}" y="{y - 9 * s:.1f}" width="{12 * s:.1f}" height="{18 * s:.1f}" rx="{5 * s:.1f}" fill="#ebcb8b"/>')
    return ('<linearGradient id="dusk" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#3b2b4a"/><stop offset=".7" stop-color="#b8706a"/><stop offset="1" stop-color="#d08770"/></linearGradient>'
            '<radialGradient id="glow"><stop offset="0" stop-color="#ebcb8b" stop-opacity=".55"/><stop offset="1" stop-color="#ebcb8b" stop-opacity="0"/></radialGradient>'
            '<clipPath id="art"><rect width="100" height="100" rx="7"/></clipPath>'
            f'<symbol id="song" viewBox="0 0 100 100"><g clip-path="url(#art)"><rect width="100" height="100" fill="url(#dusk)"/>{lanterns}'
            '<path d="M0,74 Q22,64 44,72 T100,68 V100 H0 Z" fill="#2e2235"/></g></symbol>')


add(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}" font-family="{FONT}" xml:space="preserve" role="img" aria-label="{esc(ALT)}">')
STYLE_AT = len(out)
add("")  # the <style> goes here once every keyframe exists
add(f"""<defs>
<linearGradient id="wall" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{BG0}"/><stop offset="1" stop-color="{BG1}"/></linearGradient>
<radialGradient id="haze1" cx=".85" cy="0" r=".75"><stop offset="0" stop-color="{FROST}" stop-opacity=".13"/><stop offset="1" stop-color="{FROST}" stop-opacity="0"/></radialGradient>
<radialGradient id="haze2" cx=".05" cy="1" r=".6"><stop offset="0" stop-color="{PURPLE}" stop-opacity=".09"/><stop offset="1" stop-color="{PURPLE}" stop-opacity="0"/></radialGradient>
{song_defs()}
<filter id="shadow" x="-20%" y="-20%" width="140%" height="150%"><feDropShadow dx="0" dy="18" stdDeviation="22" flood-color="#000" flood-opacity=".65"/></filter>
<clipPath id="screen"><rect width="{W}" height="{H}" rx="18"/></clipPath>
</defs>""")
add('<g clip-path="url(#screen)">')
add(f'<rect width="{W}" height="{H}" fill="url(#wall)"/><rect width="{W}" height="{H}" fill="url(#haze1)"/><rect width="{W}" height="{H}" fill="url(#haze2)"/>')


def window(x, y, w, h, title=""):
    add(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="12" fill="#1a1d22" stroke="#2a2e35"/>')
    add(f'<rect x="{x}" y="{y}" width="{w}" height="30" rx="12" fill="#1f2328"/>')
    for k, c in enumerate([RED, AMBER, GO]):
        add(f'<circle cx="{x + 18 + k * 16}" cy="{y + 15}" r="5" fill="{c}" fill-opacity=".55"/>')
    if title:
        add(tx(x + w / 2, y + 19, title, 11, DIM, anchor="middle"))


# The terminal the agent works in.
TX_, TY_, TW_, TH_ = 34, 36, 600, 330
window(TX_, TY_, TW_, TH_, "claude · ~/Sites/storefront")
TL = TX_ + 18


def term(y, s, c=FG, spans=None, weight=400):
    if spans is None:
        add(tx(TL, y, s, 12.5, c, weight))
    else:
        group(fade(spans, 0.05), spans[0][0] <= 0)
        add(tx(TL, y, s, 12.5, c, weight))
        add("</g>")


ty = TY_ + 58
term(ty, "● The launch needs a discount, and that call is yours.", DIM)
ty += 26
term(ty, "$ waybill ask --project storefront \\", FROST, [(0.3, RESET)])
term(ty + 19, "    --option \"10% for the first week\" \\", FROST, [(0.3, RESET)])
term(ty + 38, "    --option \"No offer at launch\" -- \"Pick the launch discount\"", FROST, [(0.3, RESET)])
term(ty + 61, "Asked Me: Pick the launch discount", AMBER, [(ASK, RESET)])
term(ty + 80, "● Filed. I'll take something else until you rule.", DIM, [(ASK + 0.5, RESET)])
term(ty + 110, "$ waybill next --project storefront", FROST, [(NEXT, RESET)])
term(ty + 133, "Started:", GO, [(NEXT + 0.5, RESET)], 700)
term(ty + 152, "Pick the launch discount", FG, [(NEXT + 0.5, RESET)], 700)
term(ty + 171, "  held by   claude in storefront (main)", DIM, [(NEXT + 0.5, RESET)])
term(ty + 190, "  visit     Me: 10% for the first week", GO, [(NEXT + 0.5, RESET)])

# ---------------------------------------------------------------- the bar
# packages/waybar in dot-files: a floating bar 34 px high, 6 px off the
# edges, 6 px between pills. Modules pad 8 px a side inside a group's 4.

BAR_Y, BAR_H = 620, 34
MID = BAR_Y + 21.5  # text baseline
CY = BAR_Y + BAR_H / 2
BAR_SIZE, CW = 13, 7.8  # bar font size and its advance
GW = 11  # a Nerd Font glyph's width in the bar


def pill(x, w, fill=PILL):
    add(f'<rect x="{x:.1f}" y="{BAR_Y + 0.5}" width="{w:.1f}" height="{BAR_H - 1}" rx="10" fill="{fill}" stroke="{PILL_LINE}"/>')


# Left: workspaces and the window title.
x = 6
ws = [("1", True), ("2", False), ("3", False)]
ws_w = 4 + len(ws) * (8 + CW + 8 + 2) + 4
pill(x, ws_w)
bx = x + 4
for num, active in ws:
    w = 8 + CW + 8
    bx += 1
    if active:
        add(f'<rect x="{bx:.1f}" y="{BAR_Y + 4}" width="{w:.1f}" height="{BAR_H - 8}" rx="7" fill="{PILL_ON}"/>')
    add(tx(bx + 8, MID, num, BAR_SIZE, BRIGHT if active else BAR_FG, attrs='fill-opacity=".7"'))
    bx += w + 1
x += ws_w + 6
TITLE = "claude · storefront"
title_w = 12 + len(TITLE) * CW + 12
pill(x, title_w)
add(tx(x + 12, MID, TITLE, BAR_SIZE, BRIGHT))
LEFT_END = x + title_w

# Right, packed from the edge: the tray, the clock, the desk, audio, the
# lamps (whose width follows Waybill's count), then the media pill.
x = W - 6
tray_w = 10 + 3 * 14 + 2 * 8 + 10
x -= tray_w
pill(x, tray_w)
for k, c in enumerate([BLUE, PURPLE, "#5e81ac"]):
    add(f'<rect x="{x + 10 + k * 22}" y="{CY - 7}" width="14" height="14" rx="4" fill="{c}" fill-opacity=".8"/>')
CLOCK = "Wed Nov 18  14:28"
clock_w = 12 + len(CLOCK) * CW + 12
x -= 6 + clock_w
pill(x, clock_w)
add(tx(x + 12, MID, CLOCK, BAR_SIZE, PURPLE))
audio_w = 4 + 16 + GW + CW + 3 * CW + 4
x -= 6 + audio_w
pill(x, audio_w)
sp = x + 4 + 8 + 7
add(f'<path d="M{sp - 6},{CY - 2.5} h3 l4,-3.5 v12 l-4,-3.5 h-3 z" fill="{FROST}"/><path d="M{sp + 4},{CY - 3.5} a5,5 0 0 1 0,7" fill="none" stroke="{FROST}" stroke-width="1.5" stroke-linecap="round"/>')
add(tx(x + 4 + 8 + GW + CW, MID, "35%", BAR_SIZE, FROST))
LAMPS_R = x - 6  # the lamps pill ends here

# Waybill's lamp, state by state. Waybar redraws, it does not fade.
LAMP_STATES = [  # (lamp, spans)
    ("calm", [(0, ASK), (CLICK_OPTION + LAG, T)]),
    ("needs", [(ASK, CLICK_OPTION + LAG)]),
]
LAMP = 16 + GW
SONG_BAR = "Marlow Fen · Paper Lanterns"
WAYBILL_X = {}
for lamp, spans in LAMP_STATES:
    color, word = LAMPS[lamp]
    extra = (CW + len(word) * CW) if word else 0
    lamps_w = 4 + 4 * LAMP + extra + 4
    lx = LAMPS_R - lamps_w
    still = any(s <= CLICK_LAMP + 1 <= e for s, e in spans)  # the reduced-motion still
    group(fade(spans, 0.01, 0.01), still)
    pill(lx, lamps_w)
    cx = [lx + 4 + LAMP / 2 + i * LAMP for i in range(3)]
    ring = lx + 4 + 3 * LAMP + extra + LAMP / 2
    # Roundhouse's dot and Switchyard's signal, at rest.
    add(f'<circle cx="{cx[0]}" cy="{CY}" r="5.5" fill="{GO}"/>')
    add(f'<rect x="{cx[1] - 5}" y="{CY - 9}" width="10" height="18" rx="4" fill="none" stroke="{FROST}" stroke-width="1.4"/>')
    for dy in (-5, 0, 5):
        add(f'<circle cx="{cx[1]}" cy="{CY + dy}" r="1.8" fill="{FROST}"/>')
    # Waybill: a dot, and how many things of yours hold work up.
    add(f'<circle cx="{cx[2]}" cy="{CY}" r="4.6" fill="{color}"/>')
    if word:
        add(tx(cx[2] + GW / 2 + CW - 2, MID, word, BAR_SIZE, color))
    add(f'<circle cx="{ring}" cy="{CY}" r="5.5" fill="none" stroke="{BAR_DIM}" stroke-width="1.6"/>')
    WAYBILL_X[lamp] = cx[2]
    # The media pill sits left of the lamps.
    w = 10 + 18 + 8 + len(SONG_BAR) * CW + 12
    px = lx - 6 - w
    pill(px, w)
    add(f'<use href="#song" x="{px + 10:.1f}" y="{CY - 9}" width="18" height="18"/>')
    add(tx(px + 36, MID, SONG_BAR, BAR_SIZE, AMBER))
    assert px - 6 >= LEFT_END, f"the bar overflows by {LEFT_END - px + 6:.0f}px in {lamp}"
    add("</g>")

# ---------------------------------------------------------------- the drawer
STATES = [  # (desk, spans)
    (ASKED, [(0, CLICK_OPTION + LAG)]),
    (RULED, [(CLICK_OPTION + LAG, T)]),
]
DH = max(drawer_height(d) for d, _ in STATES)
LAMP_X = WAYBILL_X["needs"]
DX = min(max(10, LAMP_X - DRAWER_W / 2), W - 10 - DRAWER_W)
DY = BAR_Y - 8 - DH

spring = anim("transform", [
    (0, "translateY(26px) scale(.06)"), (OPEN - 0.01, "translateY(26px) scale(.06)"),
    (OPEN + 0.10, "translateY(14px) scale(.5)"), (OPEN + 0.20, "translateY(4px) scale(.9)"),
    (OPEN + 0.30, "translateY(-1px) scale(1.035)"), (OPEN + 0.42, "translateY(0) scale(.988)"),
    (OPEN + 0.56, "scale(1.006)"), (OPEN + 0.72, "scale(1)"), (CLICK_OUT + 0.08, "scale(1)"),
    (CLOSED, "translateY(14px) scale(.4)"), (T, "translateY(14px) scale(.4)"),
])
show = anim("opacity", [(0, 0), (OPEN - 0.01, 0), (OPEN + 0.12, 0.85), (OPEN + 0.2, 1), (CLICK_OUT + 0.08, 1), (CLOSED, 0), (T, 0)])
# The still (reduced motion) shows the drawer with the decision waiting.
add(f'<g {style(spring, show, extra=f"transform-origin:{LAMP_X:.1f}px {BAR_Y}px")}><g transform="translate({DX:.1f},{DY:.1f})">')
SPOTS = []
for i, (d, spans) in enumerate(STATES):
    svg, h, spots = draw_drawer(d)
    SPOTS.append((spots, DH - h))
    end = spans[0][1] + 0.04 if i + 1 < len(STATES) else T
    group(fade([(spans[0][0], end)], 0.04, 0.01), i == 0)
    add(f'<g transform="translate(0,{DH - h})">{svg}</g>')
    add("</g>")


def spot(i, name):
    spots, dy = SPOTS[i]
    cx, cy = center(spots[name])
    return cx, cy + dy


def flash(i, name, t):
    spots, dy = SPOTS[i]
    x, y, w, h = spots[name]
    add(f'<rect x="{x:.1f}" y="{y + dy:.1f}" width="{w:.1f}" height="{h}" rx="8" fill="#fff" fill-opacity=".16" {style(press(t))} opacity="0"/>')


flash(0, "option:0", CLICK_OPTION)
add("</g></g>")

# ---------------------------------------------------------------- cursor


def screen(p):
    return (DX + p[0], DY + p[1])


lamp_c = (LAMP_X, CY)
option_c = screen(spot(0, "option:0"))
rest = (DX - 70, DY + 150)
outside = (DX - 150, 470)
clicks = [(CLICK_LAMP, *lamp_c), (CLICK_OPTION, *option_c), (CLICK_OUT, *outside)]
for t, cx_, cy_ in clicks:
    k = anim("opacity", [(0, 0), (t - 0.01, 0), (t, 0.9), (t + 0.45, 0), (T, 0)])
    k2 = anim("transform", [(0, "scale(.3)"), (t, "scale(.3)"), (t + 0.45, "scale(1.6)"), (T, "scale(1.6)")])
    add(f'<circle class="a ripple" style="animation-name:{k},{k2};transform-box:fill-box;transform-origin:center" cx="{cx_:.1f}" cy="{cy_:.1f}" r="18" fill="none" stroke="{FROST}" stroke-width="2.5" opacity="0"/>')


def tr(p):
    return f"translate({p[0] - 3:.1f}px, {p[1] - 4:.1f}px)"


home = (DX - 110, 500)
cur = anim("transform", [
    (0, tr(home)), (1.6, tr(home)), (CLICK_LAMP - 0.2, tr(lamp_c)), (3.4, tr(lamp_c)),
    (CLICK_OPTION - 0.2, tr(option_c)), (5.6, tr(option_c)),
    (6.4, tr(rest)), (9.6, tr(rest)),
    (CLICK_OUT - 0.2, tr(outside)), (11.4, tr(outside)), (T, tr(home)),
])
add(f'<g id="cursor" class="a" style="animation-name:{cur};animation-timing-function:cubic-bezier(.45,0,.25,1);transform:{tr(home)}">')
add('<path d="M0,0 L0,22 L6,17 L10,26 L14,24 L10,15 L18,15 Z" fill="#eceff4" stroke="#1d2024" stroke-width="1.4" stroke-linejoin="round"/>')
add("</g>")
add("</g></svg>")

out[STYLE_AT] = "<style>" + css + "\n".join(kfs) + "</style>"
Path(__file__).with_name("hero.svg").write_text("\n".join(out) + "\n")
print(f"wrote hero.svg ({sum(len(s) + 1 for s in out) // 1024} KB, {len(kfs)} keyframe sets); "
      f"drawer {DRAWER_W}x{DH} at ({DX:.0f}, {DY:.0f})")
