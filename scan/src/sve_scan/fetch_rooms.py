"""CC0 indoor panoramas (Poly Haven 2k HDR) → tone-mapped perspective crops for the room behind a table.

Writes SVE_SCAN_WORK/bg_rooms/<id>.hdr (download), bg_rooms/manifest.json and rooms.npy (uint8 [N, 384, 384, 3]).
usage: python -m sve_scan fetch-rooms
Poly Haven assets are CC0 (https://polyhaven.com/license). One request per second.
"""

import json
import math
import time
import urllib.request

import cv2
import numpy as np

from sve_scan import paths

DIR = paths.work() / "bg_rooms"
DIR.mkdir(exist_ok=True)
UA = {"User-Agent": "sve-kit-scan/0 (texture fetch for synthetic training data)"}
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


def get(url: str) -> bytes:
    with urllib.request.urlopen(urllib.request.Request(url, headers=UA), timeout=120) as r:
        return r.read()


ids = list(json.loads(get("https://api.polyhaven.com/assets?type=hdris&categories=indoor")))
pick = [a for a in ids if any(w in a for w in WANT)]
rng = np.random.default_rng(5)
pick = list(rng.choice(pick, min(32, len(pick)), replace=False))
manifest = []
for aid in pick:
    dst = DIR / f"{aid}.hdr"
    if not dst.exists():
        time.sleep(1)
        files = json.loads(get(f"https://api.polyhaven.com/files/{aid}"))
        url = files["hdri"]["2k"]["hdr"]["url"]
        time.sleep(1)
        dst.write_bytes(get(url))
    manifest.append({"id": aid, "file": dst.name, "source": f"https://polyhaven.com/a/{aid}", "license": "CC0"})
    print(len(manifest), aid, flush=True)
(DIR / "manifest.json").write_text(json.dumps(manifest, indent=1))


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
