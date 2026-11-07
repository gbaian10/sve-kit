"""Real corner labels for deck photos (corner-finder training; held-out evaluation events are refused).

Detector boxes (the union of DETS at a low threshold, so few real cards are left unboxed) -> recogniser
top-10 printings -> SIFT vs each official image on the rectified crop -> for a sure match (inliers >= 25 and
>= 1.5x the best other card) the official frame corners are projected back to the photo: that quad is the
label (independent of how tight the box was). Unsure boxes become 'ignore' regions; unsure boxes lying mostly
on confirmed cards become counted negatives.

The current labels were made with the union of three earlier corner finders (one of them, whose boxes ran
~4% small, grown by 1.04); any detector set works.

usage: python -m sve_scan det-label <rec tag> <list.txt (image paths)> <out.jsonl> [shard k/n]
  DETS=<det tag>[:grow],...   detectors to union (default: DET)
"""

import json
import os
import sys
import time
from pathlib import Path

import cv2
import numpy as np

from sve_scan import deck_label as DE
from sve_scan import tiledet as TD
from sve_scan.splits import is_eval

cv2.setNumThreads(3)
VW, VH, GROW = DE.VW, DE.VH, 1.10
FR = np.float32([[0, 0], [VW, 0], [VW, VH], [0, VH]])


def detectors():
    spec = os.environ.get("DETS") or os.environ.get("DET")
    if not spec:
        raise SystemExit("set DETS=<det tag>[:grow],... (or DET=<det tag>)")
    out = []
    for item in spec.split(","):
        tag, _, grow = item.partition(":")
        out.append((tag, float(grow or 1.0)))
    return out


def area2(qd):
    return float(np.sum(qd[:, 0] * np.roll(qd[:, 1], -1) - qd[:, 1] * np.roll(qd[:, 0], -1)) / 2)


def cw(qd):
    qd = np.asarray(qd, np.float32)
    return qd if area2(qd) > 0 else qd[::-1].copy()


def rect_m(qd, w, h):
    e = np.linalg.norm(np.roll(qd, -1, 0) - qd, axis=1)
    if (e[0] + e[2]) > (e[1] + e[3]):
        qd = np.roll(qd, -1, axis=0)
    return cv2.getPerspectiveTransform(qd.astype(np.float32), np.float32([[0, 0], [w, 0], [w, h], [0, h]]))


def sift_h(kp, des, g):
    opts, odes = DE.off_desc(g)
    if des is None or odes is None or len(kp) < 8 or len(opts) < 8:
        return 0, None
    good = [a for a, *b in DE.bf.knnMatch(odes, des, k=2) if b and a.distance < 0.78 * b[0].distance]
    if len(good) < 14:
        return 0, None
    Hm, inl = cv2.findHomography(
        np.float32([opts[m.queryIdx] for m in good]), np.float32([kp[m.trainIdx].pt for m in good]), cv2.RANSAC, 6.0
    )
    return (int(inl.sum()), Hm) if Hm is not None else (0, None)


def label(path):
    bgr = cv2.imread(str(path))
    if bgr is None:
        return None
    rgb = cv2.cvtColor(bgr, cv2.COLOR_BGR2RGB)
    gray = cv2.cvtColor(bgr, cv2.COLOR_BGR2GRAY)
    # union of three detectors at a low threshold so few real cards are left unboxed (an unboxed card would be
    # trained as background); a detector whose boxes run small gets a grow factor in DETS
    ks = []
    for m, grow in detectors():
        ks += [(k[0], (k[1] - k[1].mean(0)) * grow + k[1].mean(0)) for k in TD.detect(rgb, m, thr=0.2, area="local")]
    ks.sort(key=lambda k: -k[0])
    quads = []
    for k in ks:
        if all(TD.iou(k[1], o) < 0.35 for o in quads):
            quads.append(k[1])
    sure, ignore, info = [], [], []
    if quads:
        crops = [DE.warp(rgb, qd.astype(np.float32), 240, 336) for qd in quads]
        rec, gal = DE.REC
        cid = [c["c"] for c in gal.cards]
        sim = gal.scores(*rec.embed_both(np.stack(crops)), hub_weight=0.0)
        for qd, s in zip(quads, sim):
            big = ((qd - qd.mean(0)) * GROW + qd.mean(0)).astype(np.float32)
            M = rect_m(big, VW, VH)
            v = cv2.warpPerspective(gray, M, (VW, VH), flags=cv2.INTER_AREA)
            kp, des = DE.sift.detectAndCompute(v, None)
            best = {}  # card id -> (inliers, H)
            for g in np.argsort(-s)[:10]:
                n, Hm = sift_h(kp, des, int(g))
                if n > best.get(cid[g], (0,))[0]:
                    best[cid[g]] = (n, Hm)
            ranked = sorted(best.items(), key=lambda t: -t[1][0])
            n1 = ranked[0][1][0] if ranked else 0
            n2 = ranked[1][1][0] if len(ranked) > 1 else 0
            ok = n1 >= 25 and n1 >= 1.5 * max(n2, 1)
            if ok:
                c_crop = cv2.perspectiveTransform(FR[None], ranked[0][1][1])
                c_img = cv2.perspectiveTransform(c_crop, np.linalg.inv(M))[0]
                oq = TD.order(c_img.copy())
                # no aspect test: an oblique shot can make a card look square; the SIFT projection is the truth
                ok = (
                    cv2.isContourConvex(oq.reshape(-1, 1, 2).astype(np.float32))
                    and np.linalg.norm(np.roll(oq, -1, 0) - oq, axis=1).min() > 12
                    and TD.iou(oq, TD.order(qd.copy())) > 0.6
                )
            if ok:
                sure.append(cw(c_img).tolist())
            else:
                ignore.append(cw(qd).tolist())
            info.append([n1, n2])
    # an unsure box lying mostly on confirmed cards (straddling two of them, or a part of one) is a known
    # mistake: keep it as background so training learns to avoid it; other unsure boxes are not scored
    keep, neg = [], 0
    for b in ignore:
        b32 = TD.order(np.float32(b))
        a = cv2.contourArea(b32)
        cov = sum(cv2.intersectConvexConvex(b32, TD.order(np.float32(s)))[0] for s in sure) / max(a, 1)
        if cov > 0.5:
            neg += 1
        else:
            keep.append(b)
    ignore = keep
    h, w = gray.shape
    return {
        "image": str(path),
        "w": w,
        "h": h,
        "dataset": "x",
        "split": "train",
        "max_side": 2048,
        "quads": sure,
        "ignore": ignore,
        "neg_boxes": neg,
        "sift": info,
    }


def main():
    DE.setup(sys.argv[1])
    paths = [l.strip() for l in open(sys.argv[2]) if l.strip()]
    held = [p for p in paths if is_eval(Path(p).parent.name)]
    if held:
        raise SystemExit(f"{len(held)} photos belong to held-out evaluation events, e.g. {held[0]}")
    if len(sys.argv) > 4:
        k, n = map(int, sys.argv[4].split("/"))
        paths = paths[k::n]
    out = Path(sys.argv[3])
    done = {json.loads(l)["image"] for l in out.open()} if out.exists() else set()
    for p in paths:
        if p in done:
            continue
        t = time.time()
        r = label(p)
        if r is None:
            continue
        with out.open("a") as f:
            f.write(json.dumps(r) + "\n")
        print(
            Path(p).name,
            "boxes",
            len(r["quads"]) + len(r["ignore"]),
            "sure",
            len(r["quads"]),
            f"{time.time() - t:.0f}s",
            flush=True,
        )


if __name__ == "__main__":
    main()
