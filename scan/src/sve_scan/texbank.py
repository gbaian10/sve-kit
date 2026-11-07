"""Foil texture bank: REAL foil/glare/lighting effect layers measured on labelled second-hand listing photos.
photo --SIFT/RANSAC--> official frame; effect = aligned_photo - official, kept on flat regions of the official image
(edges would carry ghosts of the source art) and filled in by normalised blur. Saved as float16 [n, 336, 240, 3].
The current bank came from two folders of second-hand marketplace listing photos (66 layers); folders used
here are texture sources only and never used for evaluation.
usage: python -m sve_scan texbank <out.npz> <folder under SVE_SCAN_DATA/real> [...]"""

import json
import sys

import cv2
import numpy as np

from sve_scan import gallery as GL
from sve_scan import paths

REAL = paths.real()
META = json.loads((paths.work() / "gallery.json").read_text())
H, W = 336, 240
by_no = {(m["region"], m["card_no"]): i for i, m in enumerate(META)}
sift = cv2.SIFT_create(nfeatures=3000)
bf = cv2.BFMatcher()


def align(photo_bgr, off_bgr):
    g1 = cv2.cvtColor(photo_bgr, cv2.COLOR_BGR2GRAY)
    g2 = cv2.cvtColor(off_bgr, cv2.COLOR_BGR2GRAY)
    k1, d1 = sift.detectAndCompute(g1, None)
    k2, d2 = sift.detectAndCompute(g2, None)
    if d1 is None or d2 is None:
        return None, 0
    good = [x[0] for x in bf.knnMatch(d1, d2, k=2) if len(x) == 2 and x[0].distance < 0.75 * x[1].distance]
    if len(good) < 12:
        return None, 0
    src = np.float32([k1[q.queryIdx].pt for q in good])
    dst = np.float32([k2[q.trainIdx].pt for q in good])
    Hm, mask = cv2.findHomography(src, dst, cv2.RANSAC, 4.0)
    return Hm, int(mask.sum()) if mask is not None else 0


layers, info = [], []
for f in sys.argv[2:]:
    for l in (REAL / f / "manifest.jsonl").read_text().splitlines():
        if not l.strip():
            continue
        r = json.loads(l)
        if r.get("exclude"):
            continue
        gi = by_no.get((r["region"], r["card_no"]))
        ph = cv2.imread(str(REAL / f / r["file"]))
        if gi is None or ph is None:
            continue
        k = 1600 / max(ph.shape[:2])
        if k < 1:
            ph = cv2.resize(ph, (int(ph.shape[1] * k), int(ph.shape[0] * k)), interpolation=cv2.INTER_AREA)
        off = cv2.imread(str(GL.image_file(META[gi]["n"], META[gi]["f"])))  # 459x641
        Hm, inl = align(ph, off)
        if Hm is None or inl < 40:
            continue
        A = cv2.warpPerspective(ph, Hm, (off.shape[1], off.shape[0]), flags=cv2.INTER_AREA)
        valid = cv2.warpPerspective(np.ones(ph.shape[:2], np.uint8), Hm, (off.shape[1], off.shape[0])) > 0
        A = cv2.resize(A, (W, H), interpolation=cv2.INTER_AREA).astype(np.float32) / 255
        O = cv2.resize(off, (W, H), interpolation=cv2.INTER_AREA).astype(np.float32) / 255
        valid = cv2.resize(valid.astype(np.uint8), (W, H), interpolation=cv2.INTER_NEAREST) > 0
        if valid.mean() < 0.85:
            continue

        # photo ~= a*O + b (smooth local gain/offset: lighting, contrast loss, tint) + R (foil sparkle/rainbow).
        # a, b come from a guided-filter style local regression, so the source art itself is not copied.
        def box(x):
            return cv2.blur(x, (31, 31))

        m = valid.astype(np.float32)[..., None]
        # the photo is blurrier than the scan: blur the official image to match before comparing
        best = None
        for sg in (0.0, 0.8, 1.4, 2.2, 3.2):
            Ob = cv2.GaussianBlur(O, (0, 0), sg) if sg else O
            err = float(np.mean((cv2.Laplacian(Ob, cv2.CV_32F) - cv2.Laplacian(A, cv2.CV_32F))[valid] ** 2))
            if best is None or err < best[0]:
                best = (err, sg, Ob)
        O = best[2]
        mo, ma = box(O * m), box(A * m)
        cnt = np.maximum(box(valid.astype(np.float32)), 1e-3)[..., None]
        mo, ma = mo / cnt, ma / cnt
        cov = box(O * A * m) / cnt - mo * ma
        var = box(O * O * m) / cnt - mo * mo
        a = cov / (var + 0.01)
        bb = ma - a * mo
        a, bb = cv2.blur(a, (31, 31)), cv2.blur(bb, (31, 31))
        Rr = np.clip(A - (a * O + bb), -0.3, 0.3)
        # drop the residual where the source card has text/lines (misalignment leaves its edges there)
        gO = cv2.cvtColor(O, cv2.COLOR_BGR2GRAY)
        edge = np.hypot(cv2.Sobel(gO, cv2.CV_32F, 1, 0), cv2.Sobel(gO, cv2.CV_32F, 0, 1))
        edge = cv2.dilate((edge > 0.25).astype(np.uint8), np.ones((3, 3), np.uint8)).astype(bool)
        Rr[edge] = 0
        a, bb, Rr = [np.where(valid[..., None], v, d) for v, d in ((a, 1.0), (bb, 0.0), (Rr, 0.0))]
        E = np.concatenate([a, bb, Rr], 2)  # 9 channels, BGR order inside each block
        layers.append(np.concatenate([E[..., i : i + 3][..., ::-1] for i in (0, 3, 6)], 2).astype(np.float16))  # to RGB
        info.append({"folder": f, "file": r["file"], "card_no": r["card_no"], "inliers": inl, "gain": float(a.mean())})
        print(f"{f:12s} {r['file']:34s} {r['card_no']:12s} inliers={inl:4d} gain={a.mean():.2f}", flush=True)
np.savez_compressed(sys.argv[1], layers=np.stack(layers), info=json.dumps(info))
print("layers", len(layers))
