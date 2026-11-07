"""Corner finder on the held-out deck photos (never used for training, labels or texture banks).

Per photo and detector: boxes, 'useful' boxes (the recogniser's top-1 for the box is a card SIFT confirmed
somewhere in the same photo), and the SIFT corner error of useful boxes (% of card width). Offline proxy only;
the end-to-end reference stays the web client's deck reader.

usage: python -m sve_scan eval-det <rec tag> <out.json> <det tag:area> [...]   (area: global | local)
"""

import json
import sys
from pathlib import Path

import cv2
import numpy as np

from sve_scan import deck_label as DE
from sve_scan import paths
from sve_scan import tiledet as TD
from sve_scan.splits import is_eval

cv2.setNumThreads(3)
ROOT = paths.deck_photos()
LAB = ROOT / "auto-labels"
FR = np.float32([[0, 0], [DE.VW, 0], [DE.VW, DE.VH], [0, DE.VH]])


def corner_err(gray, quad, card, g_of_card):
    g = g_of_card.get(card)
    if g is None:
        return None
    v = DE.warp(gray, quad, DE.VW, DE.VH)
    kp, des = DE.sift.detectAndCompute(v, None)
    opts, odes = DE.off_desc(g)
    if des is None or len(kp) < 8:
        return None
    good = [a for a, *b in DE.bf.knnMatch(odes, des, k=2) if b and a.distance < 0.78 * b[0].distance]
    if len(good) < 14:
        return None
    Hm, inl = cv2.findHomography(
        np.float32([opts[m.queryIdx] for m in good]), np.float32([kp[m.trainIdx].pt for m in good]), cv2.RANSAC, 6.0
    )
    if Hm is None or inl.sum() < 25:
        return None
    c = cv2.perspectiveTransform(FR[None], Hm)[0]
    return float(
        min(np.linalg.norm(c - FR, axis=1).max(), np.linalg.norm(c - np.roll(FR, 2, 0), axis=1).max()) / DE.VW * 100
    )


def main():
    DE.setup(sys.argv[1])
    rec, gal = DE.REC
    cid = np.array([c["c"] for c in gal.cards])
    g_of_card = {}
    for g, c in enumerate(cid):
        g_of_card.setdefault(c, g)
    out_path, configs = sys.argv[2], [c.split(":") for c in sys.argv[3:]]
    res = []
    for e in json.loads((LAB / "image-classes.json").read_text()):
        p = LAB / f"cards/{e['i']:03d}.json"
        if e["cls"] != "deck_photo" or not p.exists() or not is_eval(e["event"]):
            continue
        deck = {c["sift_card"] for c in json.loads(p.read_text())["cards"] if c["sure"]}
        bgr = cv2.imread(str(ROOT / e["event"] / e["file"]))
        rgb = cv2.cvtColor(bgr, cv2.COLOR_BGR2RGB)
        gray = cv2.cvtColor(bgr, cv2.COLOR_BGR2GRAY)
        row = {"i": e["i"], "event": e["event"], "deck": len(deck)}
        for model, area in configs:
            ks = TD.detect(rgb, model, area=area)
            quads = [k[1].astype(np.float32) for k in ks]
            useful, errs = 0, []
            if quads:
                crops = [DE.warp(rgb, qd, 240, 336) for qd in quads]
                top = gal.scores(*rec.embed_both(np.stack(crops)), hub_weight=0.0).argmax(1)
                for qd, t in zip(quads, top):
                    if cid[t] in deck:
                        useful += 1
                        ce = corner_err(gray, qd, cid[t], g_of_card)
                        if ce is not None:
                            errs.append(ce)
            row[f"{model}:{area}"] = {
                "boxes": len(quads),
                "useful": useful,
                "errs": errs,
                "quads": [qd.tolist() for qd in quads],
            }
        res.append(row)
        print(
            e["i"],
            {
                k: (v["boxes"], v["useful"], round(float(np.median(v["errs"])), 1) if v["errs"] else None)
                for k, v in row.items()
                if isinstance(v, dict)
            },
            flush=True,
        )
    Path(out_path).write_text(json.dumps(res))
    for model, area in configs:
        k = f"{model}:{area}"
        errs = np.concatenate([r[k]["errs"] for r in res])
        print(
            k,
            "photos",
            len(res),
            "boxes",
            sum(r[k]["boxes"] for r in res),
            "useful",
            sum(r[k]["useful"] for r in res),
            "corner err median",
            round(float(np.median(errs)), 2),
            "n",
            len(errs),
        )


if __name__ == "__main__":
    main()
