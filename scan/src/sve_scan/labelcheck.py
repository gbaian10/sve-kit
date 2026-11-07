"""Flag likely label errors in a photo folder before it is used: SIFT+RANSAC inliers between the photo and the
official image of the labelled card vs the recogniser's top-3 other cards. Flag when another card clearly
wins. Hand-made labels run about 3% wrong, so every new photo set goes through this first.

usage: python -m sve_scan labelcheck <rec tag> <folder under SVE_SCAN_DATA/real> [...]
"""

import json
import sys

import cv2
import numpy as np

from sve_scan import gallery as GL
from sve_scan import paths, runtime
from sve_scan.quad import center_crop

sift = cv2.SIFT_create(nfeatures=2000)
bf = cv2.BFMatcher()


def inliers(photo_gray: np.ndarray, card: dict) -> int:
    off = cv2.imread(str(GL.image_file(card["n"], card["f"])), cv2.IMREAD_GRAYSCALE)
    k1, d1 = sift.detectAndCompute(photo_gray, None)
    k2, d2 = sift.detectAndCompute(off, None)
    if d1 is None or d2 is None or len(k1) < 8 or len(k2) < 8:
        return 0
    good = [x[0] for x in bf.knnMatch(d1, d2, k=2) if len(x) == 2 and x[0].distance < 0.75 * x[1].distance]
    if len(good) < 8:
        return len(good) // 4
    _, mask = cv2.findHomography(
        np.float32([k1[q.queryIdx].pt for q in good]), np.float32([k2[q.trainIdx].pt for q in good]), cv2.RANSAC, 6.0
    )
    return int(mask.sum()) if mask is not None else 0


def main() -> None:
    rec, gal = runtime.load(sys.argv[1])
    row_of = {(c["r"], c["no"]): i for i, c in enumerate(gal.cards)}
    for f in sys.argv[2:]:
        root = paths.real() / f
        rows = [json.loads(line) for line in (root / "manifest.jsonl").read_text().splitlines() if line.strip()]
        flagged = 0
        for r in rows:
            gi = row_of.get((r["region"], r["card_no"]))
            im = cv2.imread(str(root / r["file"]))
            if gi is None or im is None:
                continue
            k = 1400 / max(im.shape[:2])
            if k < 1:
                im = cv2.resize(im, (int(im.shape[1] * k), int(im.shape[0] * k)), interpolation=cv2.INTER_AREA)
            sim = gal.scores(*rec.embed_both(center_crop(cv2.cvtColor(im, cv2.COLOR_BGR2RGB))[None]))[0]
            truth = gal.cards[gi]["c"]
            others: list[int] = []
            for x in np.argsort(-sim)[:60]:
                if gal.cards[x]["c"] != truth and gal.cards[x]["c"] not in [gal.cards[o]["c"] for o in others]:
                    others.append(int(x))
                if len(others) == 3:
                    break
            gray = cv2.cvtColor(im, cv2.COLOR_BGR2GRAY)
            t_in = inliers(gray, gal.cards[gi])
            best_in, best_o = max((inliers(gray, gal.cards[o]), o) for o in others)
            if best_in >= max(25, 2 * t_in):
                flagged += 1
                print(
                    f"{f:14s} {r['file']:30s} label={r['card_no']:12s} ({t_in:3d} inliers)  "
                    f"better={gal.cards[best_o]['no']:12s} ({best_in:3d})",
                    flush=True,
                )
        print(f"== {f}: {flagged} flagged of {len(rows)}", flush=True)


if __name__ == "__main__":
    main()
