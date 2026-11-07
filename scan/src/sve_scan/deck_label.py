"""Deck-spread photos: tiled quad detection -> rectify -> recogniser top-K -> SIFT check against the official
images of those K candidates. A SIFT match (enough RANSAC inliers, homography close to the crop frame up to a
180-degree turn) gives a 'sure' label; the recogniser's top-1 is then scored against it at card level.

The 'sure' labels feed the sleeve glare bank (sleeve-bank) and the deck evaluation (eval-deck). They lean towards
easy cards (SIFT fails on heavy glare and blur too), so the scores are optimistic.

usage: python -m sve_scan deck-label <rec tag> <det tag> <labels.json> <out dir> [limit]
  labels.json rows: {"i", "event", "file", "cls"}; photos are SVE_SCAN_DATA/web-x/sample-events-orig/<event>/<file>
  CLS=deck_photo,... picks the image classes; SHARD=k/n splits the work.
"""

import json
import os
import sys
import time
from collections import OrderedDict
from pathlib import Path

import cv2
import numpy as np

from sve_scan import gallery as GL
from sve_scan import paths, runtime
from sve_scan.tiledet import detect

W, H = 240, 336  # recogniser crop
VW, VH = 320, 448  # SIFT compare size
K = 30
MIN_INL = 14
cv2.setNumThreads(3)
REC = None  # (Recogniser, Gallery) once loaded by setup()
card_of = no_of = None


def setup(rec_tag):
    global REC, card_of, no_of
    REC = runtime.load(rec_tag)
    card_of = np.array([c["c"] for c in REC[1].cards])
    no_of = [c["no"] for c in REC[1].cards]


sift = cv2.SIFT_create(nfeatures=1500)
bf = cv2.BFMatcher(cv2.NORM_L2)
_off = OrderedDict()  # LRU: an unbounded cache got the run OOM-killed at 4G


def off_desc(g):
    if g in _off:
        _off.move_to_end(g)
    else:
        c = REC[1].cards[g]
        im = cv2.imread(str(GL.image_file(c["n"], c["f"])), cv2.IMREAD_GRAYSCALE)
        im = cv2.resize(im, (VW, VH), interpolation=cv2.INTER_AREA)
        kp, des = sift.detectAndCompute(im, None)
        _off[g] = (np.float32([k.pt for k in kp]).reshape(-1, 2), des)
        if len(_off) > 1000:
            _off.popitem(last=False)
    return _off[g]


def warp(img, q, w, h):
    e = np.linalg.norm(np.roll(q, -1, 0) - q, axis=1)
    if (e[0] + e[2]) > (e[1] + e[3]):
        q = np.roll(q, -1, axis=0)
    m = cv2.getPerspectiveTransform(q.astype(np.float32), np.float32([[0, 0], [w, 0], [w, h], [0, h]]))
    return cv2.warpPerspective(img, m, (w, h), flags=cv2.INTER_AREA)


def verify(kp, des, g):
    opts, odes = off_desc(g)
    if des is None or odes is None or len(opts) < 8 or len(kp) < 8:
        return 0
    ms = bf.knnMatch(odes, des, k=2)
    good = [a for a, *b in ms if b and a.distance < 0.78 * b[0].distance]
    if len(good) < MIN_INL:
        return 0
    src = np.float32([opts[m.queryIdx] for m in good])
    dst = np.float32([kp[m.trainIdx].pt for m in good])
    Hm, inl = cv2.findHomography(src, dst, cv2.RANSAC, 6.0)
    if Hm is None:
        return 0
    n = int(inl.sum())
    c = cv2.perspectiveTransform(np.float32([[0, 0], [VW, 0], [VW, VH], [0, VH]])[None], Hm)[0]
    frame = np.float32([[0, 0], [VW, 0], [VW, VH], [0, VH]])
    err = min(np.abs(c - frame).max(), np.abs(c - np.roll(frame, 2, 0)).max())
    return n if err < 0.3 * VW else 0


def run(rgb, det_tag):
    quads = detect(rgb, det_tag)
    if not quads:
        return []
    crops = np.stack([warp(rgb, q, W, H) for _, q, _ in quads])
    rec, gal = REC
    e, er = rec.embed_both(crops)
    raw = gal.scores(e, er, hub_weight=0.0)
    adj = raw - 0.75 * gal.hub[None]
    gray = cv2.cvtColor(rgb, cv2.COLOR_RGB2GRAY)
    out = []
    for j, (sc, q, s) in enumerate(quads):
        order = np.argsort(-adj[j])
        cands, rank = [], {}  # top-K gallery entries with the card-level rank of their card
        for g in order[:400]:
            if card_of[g] not in rank:
                if len(rank) == K:
                    break
                rank[card_of[g]] = len(rank) + 1
            cands.append((g, rank[card_of[g]]))
        v = warp(gray, q, VW, VH)
        kp, des = sift.detectAndCompute(v, None)
        per_card = {}
        for g, rk in cands:
            n = verify(kp, des, g)
            if n > per_card.get(card_of[g], (0,))[0]:
                per_card[card_of[g]] = (n, g, rk)
        ranked = sorted(per_card.values(), key=lambda t: -t[0])
        best = ranked[0] if ranked and ranked[0][0] > 0 else (0, None, None)
        sec_inl = ranked[1][0] if len(ranked) > 1 else 0
        top = order[0]
        second = next(g for g in order if card_of[g] != card_of[top])
        gate = bool(raw[j, top] >= 0.50 and adj[j, top] - adj[j, second] >= 0.03)
        out.append(
            {
                "quad": q.round(1).tolist(),
                "det": round(sc, 3),
                "scale": s,
                "top1": no_of[top],
                "top1_card": card_of[top],
                "raw": round(float(raw[j, top]), 3),
                "margin": round(float(adj[j, top] - adj[j, second]), 3),
                "gate": gate,
                "sift_inl": int(best[0]),
                "sift_no": no_of[best[1]] if best[1] is not None else None,
                "sift_card": card_of[best[1]] if best[1] is not None else None,
                "sift_rank": best[2],
                "sift_second": int(sec_inl),
                "sure": bool(best[0] >= 25 and best[0] >= 1.5 * sec_inl),
                # card-level list: [card_no of its best printing, adjusted, raw]
                "topk": [
                    [no_of[g], round(float(adj[j, g]), 4), round(float(raw[j, g]), 4)]
                    for g in [next(g for g, r in cands if r == k) for k in range(1, len(rank) + 1)]
                ],
            }
        )
    return out


def draw(bgr, res, dst):
    for r in res:
        q = np.int32(r["quad"])
        if not r["sure"]:
            col = (0, 0, 255)  # unverified
        elif r["sift_rank"] == 1:
            col = (0, 220, 0)  # top-1 right
        else:
            col = (0, 200, 255)  # wrong, truth at rank sift_rank
        cv2.polylines(bgr, [q], True, col, 4)
        if r["sure"] and r["sift_rank"] > 1:
            cv2.putText(bgr, str(r["sift_rank"]), tuple(q.mean(0).astype(int)), cv2.FONT_HERSHEY_SIMPLEX, 2, col, 5)
    s = 1600 / max(bgr.shape[:2])
    cv2.imwrite(
        str(dst), cv2.resize(bgr, None, fx=s, fy=s, interpolation=cv2.INTER_AREA), [cv2.IMWRITE_JPEG_QUALITY, 80]
    )


def main():
    setup(sys.argv[1])
    det_tag = sys.argv[2]
    labels, root, outd = json.load(open(sys.argv[3])), paths.deck_photos(), Path(sys.argv[4])
    lim = int(sys.argv[5]) if len(sys.argv) > 5 else 10**9
    outd.mkdir(parents=True, exist_ok=True)
    cls = os.environ.get("CLS", "deck_photo").split(",")
    todo = [l for l in labels if l["cls"] in cls][:lim]
    a, n = map(int, os.environ.get("SHARD", "0/1").split("/"))
    todo = todo[a::n]
    for k, l in enumerate(todo):
        stem = f"{l['i']:03d}"
        if (outd / f"{stem}.json").exists():
            continue
        p = root / l["event"] / l["file"]
        if not p.exists():
            print("missing", p, flush=True)
            continue
        t0 = time.time()
        bgr = cv2.imread(str(p))
        rgb = cv2.cvtColor(bgr, cv2.COLOR_BGR2RGB)
        res = run(rgb, det_tag)
        (outd / f"{stem}.json").write_text(
            json.dumps({"label": l, "size": [rgb.shape[1], rgb.shape[0]], "cards": res}, ensure_ascii=False)
        )
        draw(bgr, res, outd / f"{stem}.jpg")
        nv = sum(r["sure"] for r in res)
        n1 = sum(r["sure"] and r["sift_rank"] == 1 for r in res)
        print(
            f"{k + 1}/{len(todo)} {stem} det {len(res)} verified {nv} top1-ok {n1} ({time.time() - t0:.0f}s)",
            flush=True,
        )


if __name__ == "__main__":
    main()
