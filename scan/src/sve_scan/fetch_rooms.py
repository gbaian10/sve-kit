"""CC0 indoor panoramas (Poly Haven 2k HDR) → tone-mapped perspective crops for the room behind a table.

Writes SVE_SCAN_WORK/bg_rooms/<id>.hdr (download), bg_rooms/manifest.json and rooms.npy (uint8 [N, 384, 384, 3]).
usage: python -m sve_scan fetch-rooms
Poly Haven assets are CC0 (https://polyhaven.com/license). Requests are at least 2 s apart (fetch.Polite) and every
file keeps its source URL, fetch time, ETag and SHA-256 in bg_rooms/manifest.json.
"""

import math

import cv2
import numpy as np

from sve_scan import paths
from sve_scan.fetch import Polite

API = "https://api.polyhaven.com"
DIR = paths.work() / "bg_rooms"
web = Polite(DIR)
WANT = (
    "room",
    "lounge",
    "office",
    "kitchen",
    "library",
    "cafe",
    "shop",
    "house",
    "apartment",
    "interior",
    "hotel",
    "bar",
    "restaurant",
    "studio_small",
    "workshop",
    "classroom",
    "hall",
    "store",
    "bedroom",
    "living",
)


ids = list(web.json(f"{API}/assets?type=hdris&categories=indoor"))
pick = [a for a in ids if any(w in a for w in WANT)]
rng = np.random.default_rng(5)
pick = list(rng.choice(pick, min(32, len(pick)), replace=False))
for aid in pick:
    name = f"{aid}.hdr"
    meta = {"id": str(aid), "source": f"https://polyhaven.com/a/{aid}", "license": "CC0"}
    if web.have(name) and name in web.records:
        web.download(web.records[name]["url"], name, **meta)
    else:
        web.download(web.json(f"{API}/files/{aid}")["hdri"]["2k"]["hdr"]["url"], name, **meta)
    print(len(web.records), aid, flush=True)
web.save()
manifest = [web.records[f"{aid}.hdr"] for aid in pick]


def tonemap(hdr: np.ndarray) -> np.ndarray:
    lum = 0.2126 * hdr[..., 2] + 0.7152 * hdr[..., 1] + 0.0722 * hdr[..., 0]
    key = math.exp(np.log(lum + 1e-4).mean())
    x = hdr * (0.25 / key)
    x = x / (1 + x)
    return np.clip(255 * x ** (1 / 2.2), 0, 255).astype(np.uint8)


def crop(pano: np.ndarray, yaw: float, pitch: float, fov: float, size: int = 384) -> np.ndarray:
    h, w = pano.shape[:2]
    f = (size / 2) / math.tan(fov / 2)
    xs, ys = np.meshgrid(np.arange(size) - size / 2 + 0.5, np.arange(size) - size / 2 + 0.5)
    d = np.stack([xs, -ys, np.full_like(xs, f)], -1)
    d /= np.linalg.norm(d, axis=-1, keepdims=True)
    cp, sp, cy, sy = math.cos(pitch), math.sin(pitch), math.cos(yaw), math.sin(yaw)
    d = d @ np.array([[1, 0, 0], [0, cp, sp], [0, -sp, cp]]).T  # pitch up
    d = d @ np.array([[cy, 0, sy], [0, 1, 0], [-sy, 0, cy]]).T  # yaw
    lon = np.arctan2(d[..., 0], d[..., 2])
    lat = np.arcsin(np.clip(d[..., 1], -1, 1))
    mx = ((lon / (2 * math.pi) + 0.5) * w).astype(np.float32)
    my = ((0.5 - lat / math.pi) * h).astype(np.float32)
    return cv2.remap(pano, mx, my, cv2.INTER_LINEAR, borderMode=cv2.BORDER_WRAP)


crops = []
for m in manifest:
    pano = tonemap(cv2.imread(str(DIR / m["file"]), cv2.IMREAD_UNCHANGED).astype(np.float32))
    for _ in range(14):
        c = crop(
            pano, rng.uniform(0, 2 * math.pi), math.radians(rng.uniform(-15, 25)), math.radians(rng.uniform(40, 80))
        )
        crops.append(cv2.cvtColor(c, cv2.COLOR_BGR2RGB))
np.save(paths.work() / "rooms.npy", np.stack(crops))
print("crops", len(crops))
