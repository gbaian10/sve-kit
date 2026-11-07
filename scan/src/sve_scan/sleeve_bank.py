"""Build a bank of REAL sleeve/glare/foil effect layers from the deck photos (texbank method, quad-initialised).
For every SIFT-confirmed card of a *material* event: quad -> official frame, refine with SIFT, then
photo ~= a*O + b + R  (a, b smooth local gain/offset; R high-frequency residual on flat regions).
a, b are stored at quarter resolution (they are 31-px smooth), R at full 336x240.
Only *material* events: the six held-out evaluation events (EVAL_EVENTS) never feed a bank or training.
usage: python -m sve_scan sleeve-bank <out_dir>   (SPLIT=eval builds the eval-event layers, for inspection only)"""

import json
import os
import sys
from pathlib import Path

import cv2
import numpy as np

from sve_scan import deck_label, paths
from sve_scan import gallery as GL
from sve_scan.splits import EVAL_EVENTS

cv2.setNumThreads(3)
ROOT = paths.deck_photos()
LAB = ROOT / "auto-labels"
H, W = 336, 240
OW, OH = 459, 641
meta = json.loads((paths.work() / "gallery.json").read_text())
n_of = {}
for m in meta:
    n_of.setdefault(m["card_no"], (m["n"], m["f"]))
sift = cv2.SIFT_create(nfeatures=3000)
bf = cv2.BFMatcher()
out = Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=True)
split = os.environ.get("SPLIT", "material")  # material | eval


def refine(A, O):
    g1 = cv2.cvtColor(A, cv2.COLOR_BGR2GRAY)
    g2 = cv2.cvtColor(O, cv2.COLOR_BGR2GRAY)
    k1, d1 = sift.detectAndCompute(g1, None)
    k2, d2 = sift.detectAndCompute(g2, None)
    if d1 is None or d2 is None:
        return None, 0
    good = [x[0] for x in bf.knnMatch(d1, d2, k=2) if len(x) == 2 and x[0].distance < 0.75 * x[1].distance]
    if len(good) < 12:
        return None, 0
    src = np.float32([k1[q.queryIdx].pt for q in good])
    dst = np.float32([k2[q.trainIdx].pt for q in good])
    Hm, mask = cv2.findHomography(src, dst, cv2.RANSAC, 3.0)
    if Hm is None:
        return None, 0
    # refinement must stay close to identity (the quad already put the card in place)
    c = np.float32([[0, 0], [OW, 0], [OW, OH], [0, OH]]).reshape(-1, 1, 2)
    if np.abs(cv2.perspectiveTransform(c, Hm) - c).max() > 0.12 * OW:
        return None, 0
    return Hm, int(mask.sum())


def box(x):
    return cv2.blur(x, (31, 31))


AB, RR, info, thumbs = [], [], [], []
classes = json.loads((LAB / "image-classes.json").read_text())
for e in classes:
    p = LAB / "cards" / deck_label.label_name(e["i"])
    if e["cls"] != "deck_photo" or not p.exists():
        continue
    is_eval = e["event"].replace("_p2", "") in EVAL_EVENTS
    if is_eval != (split == "eval"):
        continue
    ph = cv2.imread(str(ROOT / e["event"] / e["file"]))
    for ci, c in enumerate(json.loads(p.read_text())["cards"]):
        if not c["sure"]:
            continue
        n = n_of.get(c["sift_no"])
        if n is None:
            continue
        q = np.float32(c["quad"])
        # the quad may be 180 deg off; try both and keep the better SIFT refinement
        O = cv2.imread(str(GL.image_file(*n)))
        O = cv2.resize(O, (OW, OH), interpolation=cv2.INTER_AREA)
        best = None
        for rot in (0, 2):
            qq = np.roll(q, rot, 0)
            M0 = cv2.getPerspectiveTransform(qq, np.float32([[0, 0], [OW, 0], [OW, OH], [0, OH]]))
            A0 = cv2.warpPerspective(ph, M0, (OW, OH), flags=cv2.INTER_LINEAR, borderMode=cv2.BORDER_REPLICATE)
            Hm, inl = refine(A0, O)
            if Hm is not None and (best is None or inl > best[0]):
                best = (inl, Hm @ M0)
        if best is None or best[0] < 25:
            continue
        M = best[1]
        A = cv2.warpPerspective(ph, M, (OW, OH), flags=cv2.INTER_AREA)
        valid = cv2.warpPerspective(np.ones(ph.shape[:2], np.uint8), M, (OW, OH)) > 0
        side = float(np.linalg.norm(q[1] - q[0]))  # source width in px: low-res cards make blurry layers
        A = cv2.resize(A, (W, H), interpolation=cv2.INTER_AREA).astype(np.float32) / 255
        Of = cv2.resize(O, (W, H), interpolation=cv2.INTER_AREA).astype(np.float32) / 255
        valid = cv2.resize(valid.astype(np.uint8), (W, H), interpolation=cv2.INTER_NEAREST) > 0
        if valid.mean() < 0.9:
            continue
        bestb = None
        for sg in (0.0, 0.8, 1.4, 2.2, 3.2):
            Ob = cv2.GaussianBlur(Of, (0, 0), sg) if sg else Of
            err = float(np.mean((cv2.Laplacian(Ob, cv2.CV_32F) - cv2.Laplacian(A, cv2.CV_32F))[valid] ** 2))
            if bestb is None or err < bestb[0]:
                bestb = (err, sg, Ob)
        Ob = bestb[2]
        m = valid.astype(np.float32)[..., None]
        cnt = np.maximum(box(valid.astype(np.float32)), 1e-3)[..., None]
        mo, ma = box(Ob * m) / cnt, box(A * m) / cnt
        cov = box(Ob * A * m) / cnt - mo * ma
        var = box(Ob * Ob * m) / cnt - mo * mo
        a = cov / (var + 0.01)
        bb = ma - a * mo
        a, bb = cv2.blur(a, (31, 31)), cv2.blur(bb, (31, 31))
        R = np.clip(A - (a * Ob + bb), -0.3, 0.3)
        gO = cv2.cvtColor(Ob, cv2.COLOR_BGR2GRAY)
        edge = np.hypot(cv2.Sobel(gO, cv2.CV_32F, 1, 0), cv2.Sobel(gO, cv2.CV_32F, 0, 1))
        edge = cv2.dilate((edge > 0.25).astype(np.uint8), np.ones((3, 3), np.uint8)).astype(bool)
        R[edge] = 0
        a, bb, R = [np.where(valid[..., None], v, d) for v, d in ((a, 1.0), (bb, 0.0), (R, 0.0))]
        ab = np.concatenate([cv2.resize(v, (W // 4, H // 4), interpolation=cv2.INTER_AREA) for v in (a, bb)], 2)
        AB.append(ab[..., [2, 1, 0, 5, 4, 3]].astype(np.float16))
        RR.append(R[..., ::-1].astype(np.float16))
        # stats for the catalogue: contrast loss, wash (offset), how uneven the wash is, residual energy
        g = a.mean(2)
        o = bb.mean(2)
        st = {
            "gain": float(g.mean()),
            "gain_min": float(np.percentile(g, 5)),
            "off": float(o.mean()),
            "off_max": float(np.percentile(o, 95)),
            "off_std": float(o.std()),
            "res": float(np.abs(R).mean()),
            "sat": float((np.abs(R).max(2) > 0.12).mean()),
        }
        info.append(
            {
                "i": e["i"],
                "event": e["event"],
                "file": e["file"],
                "card": ci,
                "card_no": c["sift_no"],
                "inliers": best[0],
                "src_w": round(side),
                **st,
            }
        )
        thumbs.append(cv2.resize(np.hstack([(A * 255).astype(np.uint8), (Of * 255).astype(np.uint8)]), (W, H // 2)))
        if len(info) % 50 == 0:
            print(len(info), e["event"], flush=True)
if not AB:
    raise SystemExit(f"no usable sure cards found under {LAB / 'cards'}: run deck-label first")
np.savez(out / f"sleeve_{split}.npz", ab=np.stack(AB), res=np.stack(RR))
(out / f"sleeve_{split}.json").write_text(json.dumps(info, ensure_ascii=False))
np.save(out / f"sleeve_{split}_thumbs.npy", np.stack(thumbs))
print("layers", len(info))
