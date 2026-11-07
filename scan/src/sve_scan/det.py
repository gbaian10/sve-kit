"""Corner finder: a CenterNet-style card corner detector trained on synthetic table scenes plus real photos.

Scene: 1-10 official card images (most sleeved) plus decoys — face-down sleeve backs, playmat zone outlines,
paper, phones, keyboards, 3D deck boxes — on a table plane of limited size (CC0 texture or blurred-art playmat),
seen by a pinhole camera tilted 0-78°; beyond the table edge and above the horizon a CC0 room photo shows.
Then shadows, glare, hands, blur, noise, JPEG. Everything after the geometry is rendered on the GPU.
Targets at stride 8: a Gaussian peak at each visible card centre + 8 offsets from that cell to its 4 corners,
compared clockwise under the best cyclic shift (rectify() re-orders corners, so the start corner is free).
Recipe knobs are environment variables; the four stages of the current model are in scan/README.md.

usage: python -m sve_scan det sheet <out.png>            synthetic samples with their labels drawn
       python -m sve_scan det train <tag> <iters> [bs]   train (REC_TAG=<rec tag>: end with the phone checks)
       python -m sve_scan det eval <tag> [sheet.png]     phone-save check only (needs REC_TAG)
       python -m sve_scan det realval <tag>              side-seat burst check only (needs REC_TAG)
       python -m sve_scan det export <tag> <dst.onnx>    phone graph (weight-only int8)
"""

import json
import math
import os
import sys
import time
from pathlib import Path

import cv2
import kornia
import numpy as np
import timm
import torch
import torch.nn as nn
import torch.nn.functional as F
from kornia.enhance import jpeg_codec_differentiable
from kornia.filters import gaussian_blur2d

from sve_scan import paths

OUT = paths.work()
DEV = "cuda"
S = 256  # detector input
STRIDE = 8
G = S // STRIDE
CARD_MM = (63.0, 88.0)
TH, TW = 168, 120  # card texture
M = 6  # sleeve margin room in the texture canvas
CH, CW = TH + 2 * M, TW + 2 * M
_os = os

MAX_SLOTS = int(_os.environ.get("MAX_SLOTS", "20"))
DENSE_P = float(
    _os.environ.get("DENSE_P", "0")
)  # tournament-board grids (rows of tightly packed cards); tried, not adopted
TILT_HI = float(_os.environ.get("TILT_HI", "0.35"))  # share of steep 60-78° views
# Close-ups: the target card is 70-160% of the view. Deck photos with a few large cards hand the reader tiles
# that sit inside one card, where the earlier stages (8-70% only) boxed frames in the art as small cards.
ZOOM_P = float(_os.environ.get("ZOOM_P", "0"))
MM_PER_PX = CARD_MM[0] / TW
PHONE = paths.real() / "phone-2026-10-18"  # the maintainer's aim-frame saves (validation only)
DECOYS = ("back", "ring", "paper", "phone", "keyboard", "box")


def log(*a: object) -> None:
    print(time.strftime("%H:%M:%S"), *a, flush=True)


# ---------------- assets on the GPU ----------------
def rounded_mask(h: int, w: int, inset: int, r: float) -> torch.Tensor:
    yy, xx = torch.meshgrid(
        torch.arange(h, dtype=torch.float32) + 0.5, torch.arange(w, dtype=torch.float32) + 0.5, indexing="ij"
    )
    x0, y0, x1, y1 = inset + r, inset + r, w - inset - r, h - inset - r
    dx = torch.clamp(torch.maximum(x0 - xx, xx - x1), min=0)
    dy = torch.clamp(torch.maximum(y0 - yy, yy - y1), min=0)
    d = torch.sqrt(dx * dx + dy * dy) - r
    inside = (xx >= inset) & (xx <= w - inset) & (yy >= inset) & (yy <= h - inset)
    return (torch.clamp(0.5 - d, 0, 1) * inside).to(DEV)


def key_grid() -> torch.Tensor:
    """Keyboard keys as seen on the canvas (the canvas is stretched ~7×1.6 when placed): 1 on keys, 0 in gaps."""
    m = torch.zeros(CH, CW)
    rows = 6
    for r in range(rows):
        y0 = int(CH * (0.06 + r * 0.155))
        y1 = y0 + int(CH * 0.12)
        n = 15 if r else 16
        for k in range(n):
            x0 = int(CW * (0.02 + k * 0.96 / n))
            m[y0:y1, x0 : x0 + max(1, int(CW * 0.96 / n * 0.8))] = 1
    return m.to(DEV)


class Assets:
    def __init__(self) -> None:
        g = np.load(OUT / "gallery.npy", mmap_mode="r")
        bank = []
        for i in range(0, len(g), 512):
            x = torch.from_numpy(np.ascontiguousarray(g[i : i + 512])).to(DEV).permute(0, 3, 1, 2).float()
            bank.append(
                F.interpolate(x, size=(TH, TW), mode="bilinear", antialias=True).round().clamp(0, 255).to(torch.uint8)
            )
        self.cards = torch.cat(bank)  # [N, 3, TH, TW] uint8
        tex = []
        for f in sorted((OUT / "bg").glob("*.jpg")):  # table textures (fetch-bg)
            im = cv2.cvtColor(cv2.imread(str(f)), cv2.COLOR_BGR2RGB)
            tex.append(torch.from_numpy(cv2.resize(im, (512, 512), interpolation=cv2.INTER_AREA)).permute(2, 0, 1))
        if not tex or not (OUT / "rooms.npy").exists():
            raise SystemExit("synthetic scenes need SVE_SCAN_WORK/bg/*.jpg and rooms.npy: run fetch-bg and fetch-rooms")
        self.tables = torch.stack(tex).to(DEV)  # [T, 3, 512, 512] uint8
        rooms = np.load(OUT / "rooms.npy")  # CC0 room views (fetch-rooms)
        self.rooms = torch.from_numpy(rooms).permute(0, 3, 1, 2).contiguous().to(DEV)  # [R, 3, 384, 384] uint8
        self.card_mask = rounded_mask(CH, CW, M, 6.0)
        self.sleeve_masks = [rounded_mask(CH, CW, M - m, 6.0 + m) for m in range(1, M + 1)]
        self.ring = rounded_mask(CH, CW, 0, 4.0) - rounded_mask(CH, CW, 3, 2.0)
        self.full = rounded_mask(CH, CW, 0, 1.5)
        self.phone = rounded_mask(CH, CW, 0, 16.0)
        self.keys = key_grid()
        import os  # noqa: E402

        self.glare = (
            Glare() if any(os.environ.get(k) for k in ("REAL_P", "REAL2_P", "WR_P", "HS_P", "MISC_P")) else None
        )
        log(f"assets: {len(self.cards)} cards, {len(self.tables)} tables, {len(self.rooms)} rooms")


# ---------------- geometry (numpy, per scene) ----------------
def card_to_table(cx: float, cy: float, ang: float, sx: float = 1.0, sy: float = 1.0) -> np.ndarray:
    """Texture canvas px -> table mm. Texture v grows downwards, table y grows away from the viewer side,
    so v maps to -y (otherwise a camera above the table would see the card mirrored)."""
    k = MM_PER_PX
    c, s = math.cos(ang), math.sin(ang)
    rot = np.array([[c, -s, 0], [s, c, 0], [0, 0, 1]])
    return (
        np.array([[1, 0, cx], [0, 1, cy], [0, 0, 1]])
        @ rot
        @ np.diag([k * sx, -k * sy, 1])
        @ np.array([[1, 0, -CW / 2], [0, 1, -CH / 2], [0, 0, 1]])
    )


def layout(rng: np.random.Generator) -> list[tuple[float, float, float]]:
    mode = 3 if rng.random() < DENSE_P else rng.choice(3, p=[0.45, 0.35, 0.2])
    out = []
    if mode == 3:  # deck-list board: 3-5 rows of 5-11 cards, 1-8 mm apart, all upright
        rows, cols = rng.integers(3, 6), rng.integers(5, 12)
        gx, gy = CARD_MM[0] + rng.uniform(1, 6), CARD_MM[1] + rng.uniform(1, 8)
        for r in range(rows):
            for c in range(cols):
                if rng.random() < 0.05:
                    continue
                out.append((c * gx + rng.normal(0, 1.2), r * gy + rng.normal(0, 1.2), rng.normal(0, 0.03)))
    elif mode == 0:  # rows like a play area; some cards engaged (turned 90°)
        rows, cols = rng.integers(1, 4), rng.integers(2, 7)
        gx, gy = rng.uniform(68, 82), rng.uniform(95, 125)
        for r in range(rows):
            for c in range(cols):
                if rng.random() < 0.25:
                    continue
                tap = rng.random() < 0.2
                out.append(
                    (
                        c * (gx + (14 if tap else 0)) + rng.normal(0, 3),
                        r * gy + rng.normal(0, 3),
                        (math.pi / 2 if tap else 0) + rng.normal(0, 0.06),
                    )
                )
    elif mode == 1:  # scattered
        for _ in range(rng.integers(1, 9)):
            out.append((rng.uniform(-220, 220), rng.uniform(-160, 160), rng.uniform(0, 2 * math.pi)))
    else:  # a fanned stack
        x, y, a = 0.0, 0.0, rng.uniform(0, 2 * math.pi)
        for _ in range(rng.integers(2, 7)):
            out.append((x, y, a))
            x += rng.uniform(8, 28)
            y += rng.uniform(-6, 6)
            a += rng.normal(0, 0.12)
    if not out:
        out.append((0.0, 0.0, 0.0))
    pts = np.array([(x, y) for x, y, _ in out])
    pts -= pts.mean(0)
    rot = rng.uniform(0, 2 * math.pi)
    c, s = math.cos(rot), math.sin(rot)
    pts = pts @ np.array([[c, s], [-s, c]])
    return [(float(px), float(py), a + rot) for (px, py), (_, _, a) in zip(pts, out)]


def camera(target: tuple[float, float], rng: np.random.Generator) -> dict:
    """Pinhole camera looking at a table point; returns K (with aim jitter), R, t and the table homography."""
    fov = math.radians(rng.uniform(18, 55))
    f = (S / 2) / math.tan(fov / 2)
    r = rng.random()
    lo = (
        0.2 * (1 - TILT_HI) / 0.65
    )  # keeps the 0-25 : 25-60 ratio of the early stages while TILT_HI moves the steep share
    tilt = math.radians(
        rng.uniform(0, 25) if r < lo else rng.uniform(25, 60) if r < 1 - TILT_HI else rng.uniform(60, 78)
    )
    az = rng.uniform(0, 2 * math.pi)
    roll = math.radians(rng.normal(0, 12))
    # target card's long side in px, before foreshortening
    size = (rng.uniform(0.7, 1.6) if rng.random() < ZOOM_P else rng.uniform(0.08, 0.7)) * S
    d = f * CARD_MM[1] / size
    v = np.array([math.sin(tilt) * math.cos(az), math.sin(tilt) * math.sin(az), math.cos(tilt)])
    cam = np.array([target[0], target[1], 0.0]) + d * v
    z = -v
    h = -np.array([math.cos(az), math.sin(az), 0.0])  # towards the far side of the table
    y = -h - np.dot(-h, z) * z
    y /= np.linalg.norm(y)
    x = np.cross(y, z)
    x, y = math.cos(roll) * x + math.sin(roll) * y, -math.sin(roll) * x + math.cos(roll) * y
    rmat = np.stack([x, y, z])
    t = -rmat @ cam
    j = rng.uniform(-0.12, 0.12, 2) * S  # aim error
    k = np.array([[1, 0, j[0]], [0, 1, j[1]], [0, 0, 1]]) @ np.array([[f, 0, S / 2], [0, f, S / 2], [0, 0, 1]])
    return {"K": k, "R": rmat, "t": t, "cam": cam, "H": k @ np.stack([rmat[:, 0], rmat[:, 1], t], 1)}


def plane_h(cam: dict, origin: np.ndarray, e1: np.ndarray, e2: np.ndarray) -> np.ndarray:
    """Homography from canvas px (u right, v down) to image for the 3D rectangle origin + u·e1 + v·e2."""
    a = np.stack([e1, e2, origin], 1)
    ra = cam["R"] @ a
    return cam["K"] @ np.stack([ra[:, 0], ra[:, 1], ra[:, 2] + cam["t"]], 1)


def box_faces(cam: dict, cx: float, cy: float, ang: float, w: float, dp: float, ht: float) -> list[np.ndarray]:
    """Visible faces of a deck box (mm) standing on the table, far faces first."""
    u = np.array([math.cos(ang), math.sin(ang), 0.0])
    v = np.array([-math.sin(ang), math.cos(ang), 0.0])
    z = np.array([0.0, 0.0, 1.0])
    base = np.array([cx, cy, 0.0])
    faces = [
        (base - v * dp / 2 + z * ht / 2, -v, w, ht, z),
        (base + v * dp / 2 + z * ht / 2, v, w, ht, z),
        (base - u * w / 2 + z * ht / 2, -u, dp, ht, z),
        (base + u * w / 2 + z * ht / 2, u, dp, ht, z),
        (base + z * ht, z, w, dp, v),
    ]
    out = []
    for centre, n, l1, l2, up in faces:
        if np.dot(n, cam["cam"] - centre) <= 0:
            continue
        right = np.cross(-n, up)  # viewer looks along -n
        down = -up
        origin = centre - right * l1 / 2 - down * l2 / 2
        out.append((float(np.linalg.norm(cam["cam"] - centre)), plane_h(cam, origin, right * l1 / CW, down * l2 / CH)))
    return [p for _, p in sorted(out, key=lambda t: -t[0])]


def project(hm: np.ndarray, pts: np.ndarray) -> np.ndarray:
    p = np.c_[pts, np.ones(len(pts))] @ hm.T
    return p[:, :2] / p[:, 2:3]


def in_front(hm: np.ndarray, pts: np.ndarray) -> bool:
    p = np.c_[pts, np.ones(len(pts))] @ hm.T
    return bool(np.all(p[:, 2] > 1e-6)) and bool(np.all(np.abs(p[:, :2] / p[:, 2:3]) < 8 * S))


CARD_CORNERS = np.array([[M, M], [M + TW, M], [M + TW, M + TH], [M, M + TH]], dtype=np.float64)
CANVAS_CORNERS = np.array([[0, 0], [CW, 0], [CW, CH], [0, CH]], dtype=np.float64)


def scene(rng: np.random.Generator, n_cards: int) -> dict:
    cards = layout(rng)
    tgt = (
        cards[rng.integers(len(cards))][:2] if rng.random() < 0.85 else (rng.uniform(-150, 150), rng.uniform(-150, 150))
    )
    cam = camera(tgt, rng)
    hm = cam["H"]
    xs, ys = [c[0] for c in cards], [c[1] for c in cards]
    near = lambda pad: (rng.uniform(min(xs) - pad, max(xs) + pad), rng.uniform(min(ys) - pad, max(ys) + pad))
    flat: list[tuple] = []  # (kind, x, y, angle, sx, sy)
    for _ in range(rng.integers(0, 3)):  # face-down sleeve backs (deck, EX area)
        flat.append(("back", *near(140), rng.uniform(0, 2 * math.pi), 1.0, 1.0))
    for _ in range(rng.choice(5, p=[0.35, 0.25, 0.2, 0.12, 0.08])):  # desk clutter
        kind = rng.choice(["paper", "phone", "keyboard"], p=[0.45, 0.3, 0.25])
        sx, sy = {
            "paper": (rng.uniform(2.3, 3.4),) * 2,
            "phone": (1.11 * rng.uniform(0.95, 1.1), 1.7 * rng.uniform(0.95, 1.1)),
            "keyboard": (rng.uniform(5.5, 7.2), rng.uniform(1.4, 1.8)),
        }[kind]
        flat.append((kind, *near(320), rng.uniform(0, 2 * math.pi), sx, sy))
    for x, y, a in cards:
        flat.append(("card", x, y, a, 1.0, 1.0))
    order = rng.permutation(len(flat)) if rng.random() < 0.5 else np.arange(len(flat))
    flat = [flat[i] for i in order]
    if rng.random() < 0.4:  # playmat zone outlines, drawn under everything
        flat = [
            ("ring", *near(120), rng.choice([0.0, math.pi / 2]) + rng.normal(0, 0.03), s, s)
            for s in rng.uniform(1.0, 1.35, rng.integers(1, 5))
        ] + flat
    slots = []
    for kind, x, y, a, sx, sy in flat:
        ph = hm @ card_to_table(x, y, a, sx, sy)
        if in_front(ph, CANVAS_CORNERS):
            slots.append(
                {
                    "kind": kind,
                    "P": ph,
                    "corners": project(ph, CARD_CORNERS) if kind == "card" else None,
                    "card": int(rng.integers(n_cards)),
                }
            )
    for _ in range(rng.choice(3, p=[0.55, 0.3, 0.15])):  # deck boxes stand on top of everything
        bx, by = near(260)
        art = int(rng.integers(n_cards))
        for ph in box_faces(
            cam, bx, by, rng.uniform(0, 2 * math.pi), rng.uniform(72, 86), rng.uniform(68, 80), rng.uniform(85, 100)
        ):
            if in_front(ph, CANVAS_CORNERS):
                slots.append({"kind": "box", "P": ph, "corners": None, "card": art})
    # table: a texture square of side L mm near the target; past its edge (and above the horizon) is the room
    L = rng.uniform(450, 1500)
    c, s = math.cos(rng.uniform(0, 6.3)), math.sin(rng.uniform(0, 6.3))
    off = rng.uniform(-200, 200, 2)
    tb = (
        np.array([[1, 0, tgt[0] + off[0]], [0, 1, tgt[1] + off[1]], [0, 0, 1]])
        @ np.array([[c, -s, 0], [s, c, 0], [0, 0, 1]])
        @ np.diag([L / 512, L / 512, 1])
        @ np.array([[1, 0, -256], [0, 1, -256], [0, 0, 1]])
    )
    horizon = np.linalg.inv(hm).T @ np.array([0.0, 0.0, 1.0])  # image of the table's line at infinity
    pt = project(hm, np.array([tgt]))[0]
    side = 1.0 if horizon @ np.array([pt[0], pt[1], 1.0]) > 0 else -1.0
    return {"bg": hm @ tb, "horizon": horizon * side, "slots": slots[:MAX_SLOTS]}


# ---------------- rendering (GPU) ----------------
def rand(b: int, *shape: int) -> torch.Tensor:
    return torch.rand(b, *shape, device=DEV)


def background(assets: Assets, scenes: list[dict], rng: np.random.Generator) -> torch.Tensor:
    b = len(scenes)
    kind_bg = rng.choice(3, size=b, p=[0.6, 0.28, 0.12])
    tex = assets.tables[torch.from_numpy(rng.integers(len(assets.tables), size=b)).to(DEV)].float() / 255
    art = assets.cards[torch.from_numpy(rng.integers(len(assets.cards), size=b)).to(DEV)].float() / 255
    art = gaussian_blur2d(F.interpolate(art[:, :, : TH // 2], size=(512, 512), mode="bilinear"), (21, 21), (8.0, 8.0))
    art = art * (0.35 + 0.5 * rand(b, 1, 1, 1))
    flat = rand(b, 3, 1, 1) * 0.8 + 0.04 * torch.randn(b, 3, 512, 512, device=DEV)
    kb = torch.from_numpy(kind_bg).to(DEV).view(b, 1, 1, 1)
    src = torch.cat(
        [
            torch.where(kb == 0, tex, torch.where(kb == 1, art, flat)).clamp(0, 1),
            torch.ones(b, 1, 512, 512, device=DEV),
        ],
        1,
    )
    pb = torch.from_numpy(np.stack([sc["bg"] for sc in scenes])).float().to(DEV)
    table = kornia.geometry.transform.warp_perspective(src, pb, (S, S), padding_mode="zeros")
    # close-ups: a few pixels next to the horizon can warp to NaN; they show the room instead
    table = torch.nan_to_num(table, nan=0.0, posinf=0.0, neginf=0.0)
    yy, xx = torch.meshgrid(
        torch.arange(S, device=DEV).float() + 0.5, torch.arange(S, device=DEV).float() + 0.5, indexing="ij"
    )
    hz = torch.from_numpy(np.stack([sc["horizon"] for sc in scenes])).float().to(DEV)
    below = (hz[:, 0, None, None] * xx + hz[:, 1, None, None] * yy + hz[:, 2, None, None]) > 0
    mask = table[:, 3:4] * below.unsqueeze(1).float()
    room = assets.rooms[torch.from_numpy(rng.integers(len(assets.rooms), size=b)).to(DEV)].float() / 255
    room = F.interpolate(room, size=(S, S), mode="bilinear", antialias=True)
    sig = 0.3 + 2.5 * rand(b)  # the room is out of focus
    room = gaussian_blur2d(room, (11, 11), torch.stack([sig, sig], 1))
    return table[:, :3] * mask + room * (1 - mask)


class Glare:
    """The recogniser's second-stage effect banks (sleeve glare from deck photos, wrinkles, ceiling lights, misc, foil),
    applied per card as x' = a*x + b (+ residual). Env: TEXBANK+REAL_P (foil), REALBANK+REAL2_P,
    PROCBANK+WR_P/HS_P/MISC_P. Big banks stay in host memory; each step copies only the layers it uses."""

    def __init__(self) -> None:
        import os  # noqa: E402

        env = lambda k: float(os.environ.get(k, "0"))
        self.p = {
            "tex": env("REAL_P"),
            "real": env("REAL2_P"),
            "wr": env("WR_P"),
            "hs": env("HS_P"),
            "misc": env("MISC_P"),
        }
        self.tex = self.ab = self.res = None
        self.proc: dict[str, torch.Tensor] = {}
        if self.p["tex"] > 0:
            t = torch.from_numpy(np.load(os.environ["TEXBANK"])["layers"].astype(np.float32)).permute(0, 3, 1, 2)
            self.tex = F.interpolate(t, size=(TH, TW), mode="area").half().to(DEV)
        if self.p["real"] > 0:
            z = np.load(os.environ["REALBANK"])
            self.ab = torch.from_numpy(z["ab"])  # [n, 6, TH, TW] fp16
            self.res = F.interpolate(torch.from_numpy(z["res"]).float(), size=(TH, TW), mode="area").half()
        if any(self.p[k] > 0 for k in ("wr", "hs", "misc")):
            z = np.load(os.environ["PROCBANK"])
            self.proc = {k: torch.from_numpy(z[k]) for k in ("wr", "hs", "misc") if self.p[k] > 0}
        # one stage's boxes came out ~4% small: bank layers carry sleeve-rim brightness near their edges,
        # which the detector read as the card edge. GLARE_TAPER fades every layer to identity over that border band.
        tp = float(os.environ.get("GLARE_TAPER", "0"))
        self.taper = None
        if tp > 0:
            ry = torch.linspace(0, 1, TH, device=DEV).view(TH, 1)
            rx = torch.linspace(0, 1, TW, device=DEV).view(1, TW)
            d = torch.minimum(torch.minimum(ry, 1 - ry) * TH / TW, torch.minimum(rx, 1 - rx))  # distance in card widths
            self.taper = (d / tp).clamp(0, 1).view(1, 1, TH, TW)
        log("glare banks:", {k: v for k, v in self.p.items() if v > 0}, "taper", tp)

    @staticmethod
    def _theta(n: int, smin: float) -> torch.Tensor:
        s = smin + (1 - smin) * torch.rand(n, device=DEV)
        fx = torch.where(torch.rand(n, device=DEV) < 0.5, -1.0, 1.0)
        fy = torch.where(torch.rand(n, device=DEV) < 0.5, -fx, fx)
        th = torch.zeros(n, 2, 3, device=DEV)
        th[:, 0, 0], th[:, 1, 1] = s * fx, s * fy
        th[:, 0, 2], th[:, 1, 2] = (
            (torch.rand(n, device=DEV) * 2 - 1) * (1 - s),
            (torch.rand(n, device=DEV) * 2 - 1) * (1 - s),
        )
        return th

    @staticmethod
    def _sample(layer: torch.Tensor, th: torch.Tensor) -> torch.Tensor:
        grid = F.affine_grid(th, [layer.shape[0], layer.shape[1], TH, TW], align_corners=False)
        return F.grid_sample(layer, grid, mode="bilinear", padding_mode="border", align_corners=False)

    def _t(self, k: torch.Tensor) -> torch.Tensor:
        return k if self.taper is None else k * self.taper

    def _pick(self, bank: torch.Tensor, m: int) -> torch.Tensor:
        return bank[torch.randint(len(bank), (m,))].to(DEV, non_blocking=True).float()

    def __call__(self, x: torch.Tensor) -> torch.Tensor:
        n = x.shape[0]
        sel = lambda p: torch.nonzero(torch.rand(n) < p).flatten().to(DEV)
        lam = lambda m, lo, span: lo + span * torch.rand(m, 1, 1, 1, device=DEV)
        if self.tex is not None and len(s := sel(self.p["tex"])):
            layer = self.tex[torch.randint(len(self.tex), (len(s),), device=DEV)].float()
            layer = self._sample(layer, self._theta(len(s), 1.0))  # mirror / 180 turn only
            k = self._t(lam(len(s), 0.5, 0.5))
            x[s] = ((1 + k * (layer[:, 0:3] - 1)) * x[s] + k * layer[:, 3:6] + k * layer[:, 6:9]).clamp(0, 1)
        if self.ab is not None and len(s := sel(self.p["real"])):
            m = len(s)
            th1, th2 = self._theta(m, 0.55), self._theta(m, 0.55)
            k1, k2 = torch.randint(len(self.ab), (m,)), torch.randint(len(self.ab), (m,))
            ab1, ab2 = self._sample(self.ab[k1].to(DEV).float(), th1), self._sample(self.ab[k2].to(DEV).float(), th2)
            r1, r2 = self._sample(self.res[k1].to(DEV).float(), th1), self._sample(self.res[k2].to(DEV).float(), th2)
            w = torch.where(
                torch.rand(m, 1, 1, 1, device=DEV) < 0.5, lam(m, 0.3, 0.4), torch.ones(m, 1, 1, 1, device=DEV)
            )
            ab, res = w * ab1 + (1 - w) * ab2, w * r1 + (1 - w) * r2
            k = self._t(lam(m, 0.5, 0.6))
            x[s] = ((1 + k * (ab[:, 0:3] - 1)) * x[s] + k * ab[:, 3:6] + k * res).clamp(0, 1)
        for name, bank in self.proc.items():
            if len(s := sel(self.p[name])):
                ab = self._sample(self._pick(bank, len(s)), self._theta(len(s), 0.7))
                k = self._t(lam(len(s), 0.6, 0.4))
                x[s] = ((1 + k * (ab[:, 0:3] - 1)) * x[s] + k * ab[:, 3:6]).clamp(0, 1)
        return x


def slot_textures(
    assets: Assets, kinds: list[str], idx: torch.Tensor, rng: np.random.Generator
) -> tuple[torch.Tensor, torch.Tensor]:
    n = len(kinds)
    is_k = {k: torch.tensor([x == k for x in kinds], device=DEV).view(n, 1, 1, 1) for k in ("card",) + DECOYS}
    art = torch.zeros(n, 3, CH, CW, device=DEV)
    face = assets.cards[idx].float() / 255
    art[:, :, M : M + TH, M : M + TW] = assets.glare(face) if assets.glare is not None else face
    # cards: sleeves mostly clear plastic (light, translucent), sometimes a coloured edge; some unsleeved
    sleeve = (rand(n, 1, 1, 1) < 0.8).float()
    sm = torch.stack([assets.sleeve_masks[k] for k in rng.integers(1, M + 1, size=n) - 1]).unsqueeze(1)
    coloured = rand(n, 1, 1, 1) < 0.25
    scol = torch.where(coloured, rand(n, 3, 1, 1), 0.8 + 0.2 * rand(n, 1, 1, 1).expand(n, 3, 1, 1))
    salpha = torch.where(coloured, 0.6 + 0.4 * rand(n, 1, 1, 1), 0.25 + 0.4 * rand(n, 1, 1, 1))
    cm = assets.card_mask.view(1, 1, CH, CW)
    ring_only = (sm - cm).clamp(0, 1)
    rgb = art * cm + scol * ring_only
    alpha = cm + ring_only * salpha * sleeve
    full = assets.full.view(1, 1, CH, CW).expand(n, 1, CH, CW)
    xx = torch.linspace(0, 1, CW, device=DEV).view(1, 1, 1, CW)
    yy = torch.linspace(0, 1, CH, device=DEV).view(1, 1, CH, 1)
    # backs: solid sleeve colour with a faint pattern
    back = rand(n, 3, 1, 1) * 0.7 + 0.05 * torch.sin(xx * 40) * rand(n, 1, 1, 1)
    rgb = torch.where(is_k["back"], back.expand(n, 3, CH, CW), rgb)
    alpha = torch.where(is_k["back"], sm, alpha)
    # playmat outlines
    rgb = torch.where(is_k["ring"], (0.7 + 0.3 * rand(n, 1, 1, 1)).expand(n, 3, CH, CW), rgb)
    alpha = torch.where(is_k["ring"], assets.ring.view(1, 1, CH, CW) * (0.3 + 0.5 * rand(n, 1, 1, 1)), alpha)
    # paper: off-white, ruled or printed lines
    lines = (torch.sin(yy * (60 + 80 * rand(n, 1, 1, 1)) * math.pi) > 0.92).float() * (rand(n, 1, 1, 1) < 0.6)
    paper = (0.82 + 0.16 * rand(n, 1, 1, 1)) * torch.tensor([1.0, 0.98, 0.93], device=DEV).view(
        1, 3, 1, 1
    ) - 0.25 * lines
    rgb = torch.where(is_k["paper"], paper.expand(n, 3, CH, CW), rgb)
    alpha = torch.where(is_k["paper"], full, alpha)
    # phone: dark glass with a reflection gradient
    phone = 0.03 + 0.08 * rand(n, 1, 1, 1) + 0.25 * (xx + yy - 0.6).clamp(0, 1) * rand(n, 1, 1, 1)
    rgb = torch.where(is_k["phone"], phone.expand(n, 3, CH, CW), rgb)
    alpha = torch.where(is_k["phone"], assets.phone.view(1, 1, CH, CW).expand(n, 1, CH, CW), alpha)
    # keyboard: dark frame, lighter keys, sometimes RGB glow
    glow = 0.5 + 0.5 * torch.sin(xx * 12 + torch.arange(3, device=DEV).view(1, 3, 1, 1) * 2.1 + rand(n, 1, 1, 1) * 6)
    keyc = 0.15 + 0.5 * rand(n, 1, 1, 1) + glow * 0.5 * (rand(n, 1, 1, 1) < 0.4)
    kbd = (0.05 + 0.1 * rand(n, 1, 1, 1)) * (1 - assets.keys) + keyc * assets.keys
    rgb = torch.where(is_k["keyboard"], kbd.expand(n, 3, CH, CW), rgb)
    alpha = torch.where(is_k["keyboard"], full, alpha)
    # deck boxes: printed art (stretched over the canvas) or a plain colour, shaded per face
    box_art = F.interpolate(assets.cards[idx].float() / 255, size=(CH, CW), mode="bilinear")
    box = torch.where(rand(n, 1, 1, 1) < 0.6, box_art, rand(n, 3, 1, 1).expand(n, 3, CH, CW)) * (
        0.45 + 0.55 * rand(n, 1, 1, 1)
    )
    rgb = torch.where(is_k["box"], box, rgb)
    alpha = torch.where(is_k["box"], full, alpha)
    # plastic / foil sheen
    ang = math.pi * rand(n, 1, 1, 1)
    band = torch.exp(
        -(
            (((xx * torch.cos(ang) + yy * torch.sin(ang)) - rand(n, 1, 1, 1) * 1.4) / (0.05 + 0.3 * rand(n, 1, 1, 1)))
            ** 2
        )
    )
    sheen = band * (rand(n, 1, 1, 1) * 0.5) * (rand(n, 1, 1, 1) < 0.5)
    rgb = (rgb * (1 - sheen) + sheen).clamp(0, 1)
    return torch.cat([rgb, alpha, cm.expand(n, 1, CH, CW) * is_k["card"]], 1), ~is_k["ring"].view(n)


def render(assets: Assets, scenes: list[dict], rng: np.random.Generator) -> tuple[torch.Tensor, list[list[dict]]]:
    b = len(scenes)
    canvas = background(assets, scenes, rng)
    flat_slots = [(i, it) for i, sc in enumerate(scenes) for it in sc["slots"]]
    n = len(flat_slots)
    labels: list[list[dict]] = [[] for _ in range(b)]
    if n == 0:
        return canvas, labels
    kinds = [it["kind"] for _, it in flat_slots]
    idx = torch.tensor([it["card"] for _, it in flat_slots], device=DEV)
    layer, occl = slot_textures(assets, kinds, idx, rng)
    pm = torch.from_numpy(np.stack([it["P"] for _, it in flat_slots])).float().to(DEV)
    owner = torch.tensor([i for i, _ in flat_slots], device=DEV)
    posn = torch.tensor([j for sc in scenes for j in range(len(sc["slots"]))], device=DEV)
    nmax = max(len(sc["slots"]) for sc in scenes)
    grid = torch.zeros(b, nmax, 5, S, S, device=DEV, dtype=torch.float16)  # fp16: the GPU is shared
    for c0 in range(0, n, 96):  # warp in chunks to bound memory
        w = kornia.geometry.transform.warp_perspective(
            layer[c0 : c0 + 96], pm[c0 : c0 + 96], (S, S), padding_mode="zeros"
        )
        # A close camera can put part of a large decoy (playmat ring, deck box) behind the lens; its nearly
        # singular homography warps to NaN there. Those pixels are simply not drawn.
        w = torch.nan_to_num(w, nan=0.0, posinf=0.0, neginf=0.0)
        grid[owner[c0 : c0 + 96], posn[c0 : c0 + 96]] = w.half()
    del layer
    occludes = torch.zeros(b, nmax, device=DEV)
    occludes[owner, posn] = occl.float()
    above = torch.zeros(b, 1, S, S, device=DEV)
    vis_grid = torch.zeros(b, nmax, device=DEV)
    for j in reversed(range(nmax)):  # top first, for visibility
        vis_grid[:, j] = (grid[:, j, 4:5].float() * (1 - above)).sum((1, 2, 3))
        above = torch.maximum(above, (grid[:, j, 3:4] > 0.5).float() * occludes[:, j].view(b, 1, 1, 1))
    for j in range(nmax):  # bottom first, for drawing; a soft shadow under each object
        a = grid[:, j, 3:4].float()
        sh = gaussian_blur2d(a, (9, 9), (3.0, 3.0)).roll((2, 2), (2, 3)) * occludes[:, j].view(b, 1, 1, 1)
        canvas = canvas * (1 - 0.35 * sh)
        canvas = canvas * (1 - a) + grid[:, j, :3].float() * a
    vis_px = vis_grid[owner, posn].cpu().numpy()
    del grid
    for k, (i, it) in enumerate(flat_slots):
        if it["kind"] != "card":
            continue
        q = it["corners"]
        area = 0.5 * abs(np.dot(q[:, 0], np.roll(q[:, 1], -1)) - np.dot(q[:, 1], np.roll(q[:, 0], -1)))
        labels[i].append({"corners": q, "vis": vis_px[k] / max(area, 1.0), "area": area})
    # hands / fingers
    hand = rand(b) < 0.2
    if hand.any():
        yy, xx = torch.meshgrid(torch.arange(S, device=DEV).float(), torch.arange(S, device=DEV).float(), indexing="ij")
        cx, cy = rand(b, 1, 1) * S, rand(b, 1, 1) * S
        rx, ry = 15 + 45 * rand(b, 1, 1), 30 + 90 * rand(b, 1, 1)
        th = math.pi * rand(b, 1, 1)
        u = (xx - cx) * torch.cos(th) + (yy - cy) * torch.sin(th)
        v = -(xx - cx) * torch.sin(th) + (yy - cy) * torch.cos(th)
        m = (((u / rx) ** 2 + (v / ry) ** 2) < 1).float().unsqueeze(1) * hand.view(b, 1, 1, 1)
        m = gaussian_blur2d(m, (7, 7), (2.0, 2.0))
        skin = torch.tensor([0.85, 0.65, 0.52], device=DEV).view(1, 3, 1, 1) * (0.6 + 0.5 * rand(b, 1, 1, 1))
        canvas = canvas * (1 - m) + skin * m
    return canvas.clamp(0, 1), labels


def camera_effects(x: torch.Tensor) -> torch.Tensor:
    b = x.shape[0]
    yy, xx = torch.meshgrid(torch.linspace(0, 1, S, device=DEV), torch.linspace(0, 1, S, device=DEV), indexing="ij")
    ang = 2 * math.pi * rand(b, 1, 1)
    grad = (xx - 0.5) * torch.cos(ang) + (yy - 0.5) * torch.sin(ang)
    x = x * (1 + (rand(b, 1, 1) * 0.6 * grad)).unsqueeze(1)
    cx, cy = rand(b, 1, 1), rand(b, 1, 1)
    ell = torch.exp(
        -2 * (((xx - cx) / (0.05 + 0.3 * rand(b, 1, 1))) ** 2 + ((yy - cy) / (0.05 + 0.2 * rand(b, 1, 1))) ** 2)
    )
    m = (ell * rand(b, 1, 1) * 0.8 * (rand(b, 1, 1) < 0.5)).unsqueeze(1)
    x = x * (1 - m) + m
    x = x * (1 + (rand(b, 3, 1, 1) * 2 - 1) * 0.12)
    x = (x - 0.5) * (0.7 + 0.6 * rand(b, 1, 1, 1)) + 0.5 + (rand(b, 1, 1, 1) * 2 - 1) * 0.15
    x = x.clamp(0, 1) ** (0.75 + 0.55 * rand(b, 1, 1, 1))
    parts = []
    for chunk in x.chunk(4):  # capture resolution: digital zoom on a far card
        r = int(np.random.uniform(100, S + 1))
        small = F.interpolate(chunk, size=(r, r), mode="bilinear", antialias=True)
        parts.append(F.interpolate(small, size=(S, S), mode="bilinear", align_corners=False))
    x = torch.cat(parts)
    sig = 0.05 + 1.5 * rand(b) ** 2
    x = gaussian_blur2d(x, (9, 9), torch.stack([sig, sig], 1))
    shake = rand(b) < 0.25
    if shake.any():
        x[shake] = kornia.filters.motion_blur(x[shake], 9, float(np.random.uniform(0, 180)), 0.0)
    x = x + torch.randn_like(x) * (0.04 * rand(b, 1, 1, 1))
    return jpeg_codec_differentiable(x.clamp(0, 1), 35 + 60 * rand(b)).clamp(0, 1)


def batch(assets: Assets, rng: np.random.Generator, b: int) -> tuple[torch.Tensor, list[list[dict]]]:
    scenes = [scene(rng, len(assets.cards)) for _ in range(b)]
    with torch.no_grad():
        img, labels = render(assets, scenes, rng)
        img = camera_effects(img)
    return img, labels


# ---------------- targets ----------------
def clockwise(q: np.ndarray) -> np.ndarray:
    area2 = np.dot(q[:, 0], np.roll(q[:, 1], -1)) - np.dot(q[:, 1], np.roll(q[:, 0], -1))
    return q if area2 > 0 else q[::-1].copy()


def targets(labels: list[list[dict]]) -> tuple[torch.Tensor, torch.Tensor, list[tuple[int, int, int, np.ndarray]]]:
    b = len(labels)
    heat = np.zeros((b, G, G), np.float32)
    weight = np.ones((b, G, G), np.float32)
    pos = []
    gy, gx = np.mgrid[0:G, 0:G]
    for i, labs in enumerate(labels):
        for lab in labs:
            q = clockwise(lab["corners"]) / STRIDE
            c = q.mean(0)
            if not (0 <= c[0] < G and 0 <= c[1] < G):
                continue
            ix, iy = int(c[0]), int(c[1])
            side = math.sqrt(lab["area"]) / STRIDE
            sig = max(0.6, 0.15 * side)
            g = np.exp(-((gx - ix) ** 2 + (gy - iy) ** 2) / (2 * sig * sig))
            if lab["vis"] >= 0.4:
                heat[i] = np.maximum(heat[i], g)
                heat[i, iy, ix] = 1.0
                pos.append((i, iy, ix, q - np.array([ix + 0.5, iy + 0.5])))
            elif lab["vis"] >= 0.12 and side < G:  # half hidden: neither asked for nor punished
                # (a card bigger than the view is cut by the frame, not hidden: its art stays a negative)
                weight[i][g > 0.3] = 0
    return torch.from_numpy(heat).to(DEV), torch.from_numpy(weight).to(DEV), pos


# ---------------- model ----------------
class Det(nn.Module):
    """Stride-8 head on MobileNetV3-Small strides 8/16/32. Static shapes only (no Shape/Resize-by-size ops),
    so the phone's WebGPU runs the whole graph."""

    def __init__(self, arch: str = "mobilenetv3_small_100", ch: int = 48, pretrained: bool = True):
        super().__init__()
        self.bb = timm.create_model(arch, pretrained=pretrained, features_only=True, out_indices=(2, 3, 4))
        chs = self.bb.feature_info.channels()
        self.lat = nn.ModuleList([nn.Conv2d(c, ch, 1) for c in chs])
        self.smooth = nn.Sequential(nn.Conv2d(ch, ch, 3, padding=1), nn.BatchNorm2d(ch), nn.ReLU(inplace=True))
        self.heat = nn.Sequential(nn.Conv2d(ch, ch, 3, padding=1), nn.ReLU(inplace=True), nn.Conv2d(ch, 1, 1))
        self.corn = nn.Sequential(nn.Conv2d(ch, ch, 3, padding=1), nn.ReLU(inplace=True), nn.Conv2d(ch, 8, 1))
        nn.init.constant_(self.heat[-1].bias, -2.19)
        self.register_buffer("mean", torch.tensor((0.485, 0.456, 0.406)).view(1, 3, 1, 1))
        self.register_buffer("std", torch.tensor((0.229, 0.224, 0.225)).view(1, 3, 1, 1))

    def forward(self, x: torch.Tensor) -> tuple[torch.Tensor, torch.Tensor]:
        """x: [B, 3, 256, 256] RGB 0..1 → (heat logits [B,1,32,32], corner offsets [B,8,32,32] in cells)."""
        f = self.bb((x - self.mean) / self.std)
        p = self.lat[2](f[2])
        for i in (1, 0):
            p = F.interpolate(p, scale_factor=2.0, mode="nearest") + self.lat[i](f[i])
        p = self.smooth(p)
        return self.heat(p), self.corn(p)


def focal(logits: torch.Tensor, gt: torch.Tensor, w: torch.Tensor) -> torch.Tensor:
    p = torch.sigmoid(logits.float()).clamp(1e-4, 1 - 1e-4)
    posm = (gt == 1).float()
    pos = -torch.log(p) * (1 - p) ** 2 * posm
    neg = -torch.log(1 - p) * p**2 * (1 - gt) ** 4 * (1 - posm) * w
    return (pos.sum() + neg.sum()) / posm.sum().clamp(min=1)


def corner_loss(pred: torch.Tensor, pos: list) -> torch.Tensor:
    if not pos:
        return pred.sum() * 0
    bi = torch.tensor([p[0] for p in pos], device=DEV)
    yi = torch.tensor([p[1] for p in pos], device=DEV)
    xi = torch.tensor([p[2] for p in pos], device=DEV)
    tgt = torch.from_numpy(np.stack([p[3] for p in pos])).float().to(DEV)  # [P, 4, 2] cells, clockwise
    out = pred[bi, :, yi, xi].float().view(-1, 4, 2)
    scale = (tgt[:, 0] - tgt[:, 2]).norm(dim=1).clamp(min=1)
    errs = torch.stack([(out - tgt.roll(k, 1)).abs().sum(-1).mean(-1) for k in range(4)], 1)
    return (errs.min(1).values / scale).mean()


@torch.no_grad()
def decode(model: Det, img: torch.Tensor, thr: float = 0.5) -> list[list[tuple[float, np.ndarray]]]:
    """img [B,3,S,S] 0..1 → per image: [(score, corners [4,2] in input px)], best first."""
    hl, co = model(img)
    h = torch.sigmoid(hl.float())
    peaks = (h == F.max_pool2d(h, 3, 1, 1)) & (h > thr)
    out = []
    for i in range(img.shape[0]):
        ys, xs = torch.nonzero(peaks[i, 0], as_tuple=True)
        dets = []
        for y, x in zip(ys.tolist(), xs.tolist()):
            off = co[i, :, y, x].float().view(4, 2).cpu().numpy()
            dets.append((float(h[i, 0, y, x]), (off + np.array([x + 0.5, y + 0.5])) * STRIDE))
        out.append(sorted(dets, key=lambda d: -d[0]))
    return out


def decode_chunks(dec, model: nn.Module, inp: torch.Tensor, n: int = 16) -> list:
    """Decode in small batches: the GPU is shared, a whole eval set at once can run out of memory."""
    out = []
    for i in range(0, len(inp), n):
        out += dec(model, inp[i : i + n])
    return out


# ---------------- phone-save evaluation ----------------
def square_region(d: dict, full: np.ndarray, grow: float = 1.6) -> tuple[np.ndarray, float, float, float]:
    """Square crop of the full frame around the aim frame (side = grow × frame height), padded where it leaves
    the picture. Returns (crop, x0, y0, side) in full-frame px."""
    fl, ft, fw, fh = d["frame_rect"]
    vl, vt, vwc, vhc = d["video_rect"]
    vw, vh = d["video_size"]
    s = max(vwc / vw, vhc / vh)
    ox, oy = vl + (vwc - vw * s) / 2, vt + (vhc - vh * s) / 2
    k = full.shape[1] / vw
    cx, cy = (fl + fw / 2 - ox) / s * k, (ft + fh / 2 - oy) / s * k
    side = max(fw, fh) / s * k * grow
    x0, y0 = cx - side / 2, cy - side / 2
    m = np.array([[1, 0, -x0], [0, 1, -y0]], dtype=np.float32)
    crop = cv2.warpAffine(
        full, m, (int(round(side)), int(round(side))), flags=cv2.INTER_AREA, borderMode=cv2.BORDER_CONSTANT
    )
    return crop, x0, y0, side


def pick_aimed(dets: list[tuple[float, np.ndarray]]) -> np.ndarray | None:
    """The card being aimed at: confident and near the middle (same rule as the page)."""
    if not dets:
        return None
    return max(dets, key=lambda t: t[0] - 0.6 * np.linalg.norm(t[1].mean(0) / S - 0.5))[1]


def recogniser():
    """The shipped recogniser + index named by REC_TAG, for the end-to-end checks (None: skip them)."""
    tag = os.environ.get("REC_TAG")
    if not tag:
        return None
    from sve_scan import runtime  # noqa: E402

    return runtime.load(tag)


def _top_cards(rec, arr: np.ndarray) -> tuple[list[str], np.ndarray]:
    model, gal = rec
    sim = gal.scores(*model.embed_both(arr), hub_weight=0.0)
    return [gal.cards[t]["c"] for t in sim.argmax(1)], sim.max(1)


def phone_eval(model: nn.Module, rec, sheet: str | None = None, dec=None) -> dict:
    from sve_scan.quad import rectify, squash  # noqa: E402

    dec = dec or decode
    by_no = {(c["r"], c["no"]): c["c"] for c in rec[1].cards}
    rows = []
    for f in sorted(PHONE.glob("*.json")):
        d = json.loads(f.read_text())
        lab = d.get("label") or d.get("label_manual")
        if not lab or d.get("landscape"):
            continue
        rd = lambda key: cv2.cvtColor(cv2.imread(str(f.with_name(f"{f.stem}_{key}.jpg"))), cv2.COLOR_BGR2RGB)
        full = rd("full")
        reg, x0, y0, side = square_region(d, full)
        rows.append(
            {
                "stem": f.stem,
                "truth": by_no[(lab["r"], lab["no"])],
                "crop": rd("crop"),
                "full": full,
                "reg": reg,
                "x0": x0,
                "y0": y0,
                "side": side,
                "none": d.get("label") is None,
            }
        )
    model.eval()
    inp = (
        torch.from_numpy(np.stack([cv2.resize(r["reg"], (S, S), interpolation=cv2.INTER_AREA) for r in rows]))
        .to(DEV)
        .permute(0, 3, 1, 2)
        .float()
        / 255
    )
    quads = []
    for r, ds in zip(rows, decode_chunks(dec, model, inp)):
        q = pick_aimed(ds)
        quads.append(None if q is None else (q * (r["side"] / S) + np.array([r["x0"], r["y0"]])).astype(np.float32))

    def judge(imgs: list[np.ndarray]) -> tuple[np.ndarray, np.ndarray]:
        top, best = _top_cards(rec, np.stack(imgs))
        return np.array([t == r["truth"] for t, r in zip(top, rows)]), best

    base_ok, base_top = judge([squash(r["crop"]) for r in rows])
    ok, top = judge([rectify(r["full"], q) if q is not None else squash(r["crop"]) for r, q in zip(rows, quads)])
    found = np.array([q is not None for q in quads])
    none = np.array([r["none"] for r in rows])
    res = {
        "n": len(rows),
        "found": round(float(found.mean()), 3),
        "base": round(float(base_ok.mean()), 3),
        "rect_or_frame": round(float(ok.mean()), 3),
        "rescued": f"{int(ok[none].sum())}/{int(none.sum())}",
        "lost": int((base_ok & ~ok).sum()),
    }
    if sheet:
        tiles = []
        for r, q, o in zip(rows, quads, ok):
            im = cv2.resize(r["reg"], (220, 220), interpolation=cv2.INTER_AREA)
            if q is not None:
                qq = (q - np.array([r["x0"], r["y0"]])) * (220 / r["side"])
                cv2.polylines(im, [np.int32(qq).reshape(-1, 1, 2)], True, (40, 255, 40) if o else (255, 40, 40), 2)
            cv2.putText(im, r["stem"][9:], (3, 12), cv2.FONT_HERSHEY_SIMPLEX, 0.38, (255, 255, 0), 1)
            tiles.append(im)
        while len(tiles) % 8:
            tiles.append(np.zeros_like(tiles[0]))
        grid = np.concatenate([np.concatenate(tiles[j : j + 8], 1) for j in range(0, len(tiles), 8)], 0)
        cv2.imwrite(sheet, cv2.cvtColor(grid, cv2.COLOR_RGB2BGR))
    return res


# ---------------- commands ----------------
def cmd_sheet(path: str) -> None:
    assets = Assets()
    rng = np.random.default_rng(0)
    img, labels = batch(assets, rng, 24)
    arr = (img.permute(0, 2, 3, 1).cpu().numpy() * 255).astype(np.uint8)
    tiles, mirrored = [], 0
    for im, labs in zip(arr, labels):
        im = np.ascontiguousarray(im)
        for lab in labs:
            q = lab["corners"]
            mirrored += (np.dot(q[:, 0], np.roll(q[:, 1], -1)) - np.dot(q[:, 1], np.roll(q[:, 0], -1))) < 0
            col = (40, 255, 40) if lab["vis"] >= 0.4 else (255, 200, 0) if lab["vis"] >= 0.12 else (255, 40, 40)
            cv2.polylines(im, [np.int32(q).reshape(-1, 1, 2)], True, col, 1)
        tiles.append(im)
    grid = np.concatenate([np.concatenate(tiles[j : j + 6], 1) for j in range(0, 24, 6)], 0)
    cv2.imwrite(path, cv2.cvtColor(grid, cv2.COLOR_RGB2BGR))
    log(f"{sum(len(x) for x in labels)} card labels, mirrored {mirrored}")


def cmd_train(tag: str, iters: int, bs: int) -> None:
    """Env: INIT=<tag> continues from det_<tag>.pt; REAL=<realdet.jsonl> mixes REAL_FRAC (default 0.4) real crops
    into every batch (RealDet from realdet.py); LR overrides the peak learning rate."""
    import os  # noqa: E402

    assets = Assets()
    model = Det(pretrained=not os.environ.get("INIT")).to(DEV)
    if os.environ.get("INIT"):
        model.load_state_dict(torch.load(OUT / f"det_{os.environ['INIT']}.pt", map_location=DEV))
    real, n_real = None, 0
    if os.environ.get("REAL"):
        from sve_scan.det_real import RealDet  # noqa: E402

        real = RealDet(
            os.environ["REAL"],
            "train",
            size=S,
            seed=int(time.time()),
            zoom_p=ZOOM_P,
            inside_p=float(os.environ.get("INSIDE_P", "0")),
        )
        n_real = int(round(bs * float(os.environ.get("REAL_FRAC", "0.4"))))
    lr = float(os.environ.get("LR", "2e-3"))
    opt = torch.optim.AdamW(model.parameters(), lr=lr, weight_decay=1e-4)
    sched = torch.optim.lr_scheduler.OneCycleLR(opt, max_lr=lr, total_steps=iters, pct_start=0.05)
    rng = np.random.default_rng(int(time.time()))
    t0, hist = time.time(), []
    for it in range(1, iters + 1):
        model.train()
        img, labels = batch(assets, rng, bs - n_real)
        if real is not None:
            rimg, rlab = real.batch(n_real)
            rimg = torch.from_numpy(rimg).to(DEV).permute(0, 3, 1, 2).float() / 255
            with torch.no_grad():  # real photos still get camera noise, blur and JPEG like the synthetic ones
                rimg = camera_effects(rimg)
            img = torch.cat([img, rimg])
            labels = labels + [
                [
                    {
                        "corners": np.asarray(l["corners"], np.float64),
                        "vis": float(l.get("vis", 1.0)),
                        "area": float(l["area"]),
                    }
                    for l in ls
                ]
                for ls in rlab
            ]
        if not torch.isfinite(img).all():  # checked before the forward pass: BatchNorm statistics would keep a NaN
            log(f"it {it}: non-finite input, batch skipped")
            sched.step()
            continue
        heat, w, pos = targets(labels)
        with torch.autocast("cuda", dtype=torch.bfloat16):
            hl, co = model(img)
        lh = focal(hl[:, 0], heat, w)
        lc = corner_loss(co, pos)
        loss = lh + 4.0 * lc
        if not torch.isfinite(loss):  # one bad batch would turn every weight into NaN
            log(f"it {it}: non-finite loss, batch skipped")
            sched.step()
            continue
        opt.zero_grad(set_to_none=True)
        loss.backward()
        torch.nn.utils.clip_grad_norm_(model.parameters(), 5.0)
        opt.step()
        sched.step()
        hist.append((lh.item(), lc.item()))
        if it % 200 == 0 or it == iters:
            h = np.array(hist[-200:])
            log(
                f"it {it}/{iters} heat {h[:, 0].mean():.3f} corner {h[:, 1].mean():.4f} "
                f"({(time.time() - t0) / it:.2f} s/it)"
            )
        if it % 2000 == 0 or it == iters:
            torch.save(model.state_dict(), OUT / f"det_{tag}.pt")
    rec = recogniser()
    if rec is None:
        log("REC_TAG not set: skipping the phone and burst checks")
        return
    log("phone eval:", phone_eval(model, rec, str(OUT / f"det_{tag}_phone.png")))
    log("burst eval:", real_val(model, rec))


BURSTS = paths.real() / "det-train"  # side-seat bursts, SIFT-labelled (validation only, never trained on)


def quad_iou(a: np.ndarray, b: np.ndarray) -> float:
    a, b = clockwise(a.astype(np.float32)), clockwise(b.astype(np.float32))
    if not (cv2.isContourConvex(a.reshape(-1, 1, 2)) and cv2.isContourConvex(b.reshape(-1, 1, 2))):
        return 0.0
    inter, _ = cv2.intersectConvexConvex(a, b)
    union = cv2.contourArea(a) + cv2.contourArea(b) - inter
    return float(inter / union) if union > 0 else 0.0


def corner_err(a: np.ndarray, b: np.ndarray) -> float:
    a, b = clockwise(a), clockwise(b)
    return min(float(np.linalg.norm(a - np.roll(b, k, 0), axis=1).mean()) for k in range(4))


def real_val(model: nn.Module, rec, dec=None) -> dict:
    """The user's side-seat bursts (validation only, never trained on): labels from detlabel.py (SIFT).
    Per frame the page's square around the aim frame goes in; scores are in that square's 256-px frame."""
    from sve_scan.quad import rectify, squash  # noqa: E402

    dec = dec or decode
    labels = json.loads((BURSTS / "labels.json").read_text())
    rows = []
    for stem, labs in labels.items():
        if not labs:
            continue
        d = json.loads((BURSTS / f"{stem}.json").read_text())
        full = cv2.cvtColor(cv2.imread(str(BURSTS / f"{stem}_full.jpg")), cv2.COLOR_BGR2RGB)
        reg, x0, y0, side = square_region(d, full)
        k = S / side
        gts = [(np.float32(l["corners"]) - np.float32([x0, y0])) * k for l in labs]
        inside = [g for g in gts if 0 <= g.mean(0)[0] < S and 0 <= g.mean(0)[1] < S]
        rows.append(
            {
                "stem": stem,
                "full": full,
                "reg": cv2.resize(reg, (S, S), interpolation=cv2.INTER_AREA),
                "gts": inside,
                "nos": [l["card_no"] for l, g in zip(labs, gts) if 0 <= g.mean(0)[0] < S and 0 <= g.mean(0)[1] < S],
                "x0": x0,
                "y0": y0,
                "k": k,
            }
        )
    model.eval()
    inp = torch.from_numpy(np.stack([r["reg"] for r in rows])).to(DEV).permute(0, 3, 1, 2).float() / 255
    dets = decode_chunks(dec, model, inp)
    n_gt = hit = fp = aim_ok = aim_n = 0
    errs, aimed = [], []
    for r, ds in zip(rows, dets):
        n_gt += len(r["gts"])
        used = set()
        for _score, q in ds:
            ious = [quad_iou(q, g) for g in r["gts"]]
            j = int(np.argmax(ious)) if ious else -1
            if j >= 0 and ious[j] >= 0.5 and j not in used:
                used.add(j)
                hit += 1
                errs.append(corner_err(q, r["gts"][j]))
            else:
                fp += 1
        q = pick_aimed(ds)
        if r["gts"]:
            aim_n += 1
            if q is not None and max(quad_iou(q, g) for g in r["gts"]) >= 0.5:
                aim_ok += 1
        aimed.append(None if q is None else q / r["k"] + np.float32([r["x0"], r["y0"]]))
    # end to end: does the recogniser name the aimed card right after straightening?
    by_no = {c["no"]: c["c"] for c in rec[1].cards}
    picks = [(r, q) for r, q in zip(rows, aimed) if q is not None and r["gts"]]
    named = 0
    if picks:
        top, _ = _top_cards(rec, np.stack([rectify(r["full"], q) for r, q in picks]))
        named = sum(t in {by_no[n] for n in r["nos"]} for t, (r, _) in zip(top, picks))
    # baseline: the page without the corner finder (the aim-frame crop it saved)
    crops = np.stack(
        [
            squash(cv2.cvtColor(cv2.imread(str(BURSTS / f"{r['stem']}_crop.jpg")), cv2.COLOR_BGR2RGB))
            for r in rows
            if r["gts"]
        ]
    )
    topc, _ = _top_cards(rec, crops)
    frame_named = sum(t in {by_no[n] for n in r["nos"]} for t, r in zip(topc, [r for r in rows if r["gts"]]))
    return {
        "frames": len(rows),
        "cards": n_gt,
        "recall": round(hit / max(n_gt, 1), 3),
        "false_pos": fp,
        "frame_crop_named_right": f"{frame_named}/{aim_n}",
        "corner_err_px": round(float(np.mean(errs)), 2) if errs else None,
        "aimed_on_card": f"{aim_ok}/{aim_n}",
        "aimed_named_right": f"{named}/{aim_n}",
    }


class Exported(nn.Module):
    """Phone graph: image [1,3,256,256] RGB 0..1 → heat [1,1,32,32] (sigmoid) and corners [1,8,32,32]
    (offsets in cells from each cell centre, clockwise x,y pairs). Peak picking happens in the page."""

    def __init__(self, m: Det):
        super().__init__()
        self.m = m

    def forward(self, x: torch.Tensor) -> tuple[torch.Tensor, torch.Tensor]:
        hl, co = self.m(x)
        return torch.sigmoid(hl), co


def cmd_export(tag: str, dst: str) -> None:
    import onnx  # noqa: E402
    import onnxruntime as ort  # noqa: E402

    from sve_scan.rec_export import weight_only_int8  # noqa: E402

    m = Det(pretrained=False)
    m.load_state_dict(torch.load(OUT / f"det_{tag}.pt", map_location="cpu"))
    w = Exported(m).eval()
    f32 = OUT / f"det_{tag}.fp32.onnx"
    torch.onnx.export(
        w,
        torch.rand(1, 3, S, S),
        str(f32),
        input_names=["image"],
        output_names=["heat", "corners"],
        opset_version=17,
        dynamo=False,
    )
    w8 = OUT / f"det_{tag}.w8.onnx"
    weight_only_int8(f32, w8)
    ops = sorted({n.op_type for n in onnx.load(str(w8)).graph.node})
    print("ops:", ops)
    x = torch.rand(1, 3, S, S)
    with torch.no_grad():
        ref = [t.numpy() for t in w(x)]
    for p in (f32, w8):
        got = ort.InferenceSession(str(p), providers=["CPUExecutionProvider"]).run(None, {"image": x.numpy()})
        print(
            p.name,
            f"{p.stat().st_size / 2**20:.2f} MiB",
            "max |diff| heat",
            float(np.abs(got[0] - ref[0]).max()),
            "corners",
            float(np.abs(got[1] - ref[1]).max()),
        )
    Path(dst).write_bytes(w8.read_bytes())
    print("copied to", dst)


if __name__ == "__main__":
    cmd = sys.argv[1]
    if cmd == "sheet":
        cmd_sheet(sys.argv[2])
    elif cmd == "train":
        cmd_train(sys.argv[2], int(sys.argv[3]), int(sys.argv[4]) if len(sys.argv) > 4 else 32)
    elif cmd == "eval":
        mdl = Det(pretrained=False).to(DEV)
        mdl.load_state_dict(torch.load(OUT / f"det_{sys.argv[2]}.pt", map_location=DEV))
        print(phone_eval(mdl, recogniser(), sys.argv[3] if len(sys.argv) > 3 else None))
    elif cmd == "export":
        cmd_export(sys.argv[2], sys.argv[3])
    elif cmd == "realval":
        mdl = Det(pretrained=False).to(DEV)
        mdl.load_state_dict(torch.load(OUT / f"det_{sys.argv[2]}.pt", map_location=DEV))
        print(real_val(mdl, recogniser()))
