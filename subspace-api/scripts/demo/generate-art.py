"""Regenerate the fictional screenshot fixtures with Python 3.12 and rsvg-convert."""

from html import escape
from pathlib import Path
from subprocess import run

WIDTH, HEIGHT = 880, 500
ASSETS = Path(__file__).resolve().parent / "assets"


def text(x, y, value, size=15, colour="#c3cedf", weight=400):
    return (
        f'<text x="{x}" y="{y}" fill="{colour}" font-size="{size}" '
        f'font-weight="{weight}">{escape(value)}</text>'
    )


def box(x, y, w, h, fill="#161f30", stroke="#30415b", radius=10):
    return (
        f'<rect x="{x}" y="{y}" width="{w}" height="{h}" '
        f'rx="{radius}" fill="{fill}" stroke="{stroke}"/>'
    )


def line(x1, y1, x2, y2, colour="#2d415c"):
    return f'<path d="M{x1} {y1}L{x2} {y2}" stroke="{colour}"/>'


def dot(x, y, colour, radius=5):
    return f'<circle cx="{x}" cy="{y}" r="{radius}" fill="{colour}"/>'


def layout(space, title, meta, tabs):
    elements = [
        box(0, 0, WIDTH, HEIGHT, "#0e1624", "#0e1624", 0),
        box(0, 0, WIDTH, 56, "#1b2638", "#2c3a50", 0),
        dot(26, 27, "#75e6cf", 6),
        text(43, 33, "Fallow Reach", 18, "#f4f7ff", 700),
        text(206, 33, space),
        text(655, 33, "PILOT SIMULATOR", 13, "#75e6cf", 700),
        box(0, 57, 154, 443, "#111c2c", "#2c3a50", 0),
        text(24, 94, "REHEARSAL", 11, "#8f9db2", 700),
        text(181, 96, title, 27, "#f1f6ff", 700),
        text(181, 121, meta, 13, "#a9b9d1"),
    ]
    for i, tab in enumerate(tabs):
        y = 111 + i * 44
        if i == 0:
            elements.append(box(13, y, 128, 34, "#203850", "#294763", 6))
        elements.append(text(24, y + 22, tab, 13, "#d9e8f5" if i == 0 else "#96a8c2"))
    return elements


def route_chart(top, height, hue):
    elements = [box(181, top, 444, height, "#131f32")]
    for x in range(203, 624, 39):
        elements.append(line(x, top + 23, x, top + height - 25))
    for y in range(top + 29, top + height - 15, 32):
        elements.append(line(203, y, 602, y))
    nodes = [(236, 221), (325, 267), (403, 245), (515, 361), (573, 332)]
    route = " ".join(f"{x},{y}" for x, y in nodes)
    elements.extend([
        f'<polyline points="{route}" fill="none" stroke="{hue}" stroke-opacity=".15" stroke-width="18"/>',
        f'<polyline points="{route}" fill="none" stroke="{hue}" stroke-width="3"/>',
    ])
    for (x, y), name in zip(nodes, ("Ash Gate", "Cairn", "Fallow", "Lichen", "Orchard"), strict=True):
        elements.extend([dot(x, y, hue), text(x - 12, y - 12, name, 12, "#f1f6ff")])
    return elements


def navigation():
    elements = layout("Navigation console", "Fallow Reach course preview",
                      "Rehearsal 07   /   Sector 14   /   Cold cache",
                      ["Route planner", "Waypoints", "Telemetry", "Run history"])
    return elements + route_chart(150, 309, "#75e6cf") + [
        box(641, 150, 216, 146), text(659, 180, "Tile decode", 16, "#f4f7ff", 700),
        text(659, 235, "87 ms", 36, "#ffd07a", 700), text(661, 266, "Cold-cache peak", 13),
        box(641, 311, 216, 148), text(659, 341, "Pointer latency", 16, "#f4f7ff", 700),
        text(659, 397, "4 ms", 36, "#75e6cf", 700), text(661, 427, "Worst frame", 13),
    ]


def thermal():
    elements = layout("Warp diagnostics", "Plasma temperature trend",
                      "Rehearsal 07   /   09:40–09:48 UTC   /   5-minute window",
                      ["Thermal alerts", "Pressure", "Calibration", "Run history"])
    elements += [box(181, 150, 676, 308)]
    for y, reading in ((198, "92"), (253, "88"), (308, "84"), (363, "80"), (410, "76")):
        elements.extend([text(199, y + 5, reading, 12), line(231, y, 832, y)])
    elements += [
        box(232, 197, 600, 57, "#42313b", "#42313b", 0),
        text(247, 221, "Warning band", 13, "#ffc88d"),
        '<path d="M235 386L290 370 333 374 382 248 417 358 460 333 515 345 563 315 '
        '615 284 665 269 714 245 770 223 828 211" fill="none" stroke="#ffbc74" stroke-width="3"/>',
        dot(714, 245, "#ffda9c", 7),
        text(757, 285, "Alert fired", 13, "#ffc88d", 700),
    ]
    return elements + [text(x, 433, stamp, 12) for x, stamp in
                       ((231, "09:40"), (374, "09:42"), (517, "09:44"), (660, "09:46"), (803, "09:48"))]


def atlas():
    elements = layout("Sector atlas", "Regional overlay review",
                      "Design proposal 03   /   800 km scale   /   Regional zoom",
                      ["Layers", "Annotations", "Style tokens", "Review notes"])
    return elements + route_chart(150, 309, "#b7a4fa") + [
        box(641, 150, 216, 309), text(661, 181, "Visible layers", 17, "#f4f7ff", 700),
        *[text(661, 222 + n * 41, name, 13, "#b7a4fa" if n < 2 else "#91a5c4")
          for n, name in enumerate(("●  Survey corridor", "●  Danger overlay",
                                    "○  Waypoint labels", "○  Radio shadows"))],
        line(661, 370, 837, 370), text(661, 406, "Corridor stays pinned", 14, "#f4f7ff", 700),
        text(661, 431, "Review label density next", 12),
    ]


def rollout():
    elements = layout("Relay reliability", "Backup relay rollout",
                      "Rehearsal 12   /   pilot-simulator   /   Late acknowledgement",
                      ["Rollout", "Sequence log", "Health checks", "Runbook"])
    for x, name, value, colour in (
        (181, "Traffic on backup", "25%", "#b7a4fa"),
        (410, "Acknowledged", "240 / 240", "#75e6cf"),
        (639, "Replays filtered", "1", "#ffd07a"),
    ):
        elements.extend([box(x, 150, 218, 115), text(x + 17, 179, name, 14),
                         text(x + 17, 230, value, 32, colour, 700)])
    elements.extend([box(181, 285, 676, 173), text(199, 313, "Rollout events", 16, "#f4f7ff", 700)])
    for i, (stamp, message, colour) in enumerate((
        ("09:40:04", "Connected to backup relay", "#75e6cf"),
        ("09:42:10", "Reached traffic limit for this rehearsal", "#b7a4fa"),
        ("09:42:11", "Filtered replay for frame 0183", "#ffd07a"),
        ("09:45:00", "Waiting for rollback rehearsal", "#a9b9d1"),
    )):
        y = 340 + i * 30
        elements.extend([dot(205, y - 4, colour, 4), text(224, y, stamp, 12), text(312, y, message, 13)])
    return elements


if __name__ == "__main__":
    ASSETS.mkdir(exist_ok=True)
    for filename, make in {
        "navigation-console.png": navigation,
        "thermal-alerts.png": thermal,
        "sector-atlas.png": atlas,
        "relay-rollout.png": rollout,
    }.items():
        svg = (
            f'<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{HEIGHT}">'
            '<g font-family="DejaVu Sans, sans-serif">' + "".join(make()) + "</g></svg>"
        )
        run(["rsvg-convert", "--output", str(ASSETS / filename)], input=svg.encode(), check=True)
