"""Recogniser evaluation with the shipped int8 model and index (what users get), card level.

Per labelled photo folder (SVE_SCAN_DATA/real/<folder>/manifest.jsonl, rows {file, region, card_no,
[conditions], [exclude]}): the centre crop (what an aim frame sees) and the whole photo squashed, each with the
shipped hub correction and without; plus the synthetic query sets of the gallery step. A query counts as right
when the right card id is among the first k distinct cards (printings of one card score alike).

Pass the evaluation folders explicitly; folders used as foil texture sources (texbank) never count as
evaluation.

usage: python -m sve_scan eval-rec <tag> <folder> [...]   ->  SVE_SCAN_WORK/eval/<tag>-rec.json
"""

import json
import sys

import cv2
import numpy as np

from sve_scan import artifacts, paths, runtime
from sve_scan.quad import center_crop, squash

KS = (1, 5)
HUB = artifacts.REC_SCORING["hubWeight"]


def hits(gal: runtime.Gallery, sim: np.ndarray, truth: list[str]) -> dict[str, float]:
    order = np.argsort(-sim, axis=1)[:, :200]
    got = [runtime.card_level(row, gal.cards, max(KS)) for row in order]
    return {f"top{k}": round(float(np.mean([t in g[:k] for t, g in zip(truth, got)])), 4) for k in KS}


def score(rec: runtime.Recogniser, gal: runtime.Gallery, arr: np.ndarray, truth: list[str]) -> dict[str, object]:
    e0, e1 = rec.embed_both(arr)
    return {
        "n": len(truth),
        "hub": hits(gal, gal.scores(e0, e1, HUB), truth),
        "plain": hits(gal, gal.scores(e0, e1, 0.0), truth),
    }


def folder_set(name: str, card_of: dict[tuple[str, str], str]) -> tuple[list[np.ndarray], list[str]]:
    root = paths.real() / name
    ims, truth = [], []
    for line in (root / "manifest.jsonl").read_text().splitlines():
        if not line.strip():
            continue
        r = json.loads(line)
        card = card_of.get((r["region"], r["card_no"]))
        im = cv2.imread(str(root / r["file"]))
        if r.get("exclude") or card is None or im is None:
            continue
        k = 1600 / max(im.shape[:2])
        if k < 1:
            im = cv2.resize(im, (int(im.shape[1] * k), int(im.shape[0] * k)), interpolation=cv2.INTER_AREA)
        ims.append(cv2.cvtColor(im, cv2.COLOR_BGR2RGB))
        truth.append(card)
    return ims, truth


def main() -> None:
    tag, folders = sys.argv[1], sys.argv[2:]
    rec, gal = runtime.load(tag)
    card_of = {(c["r"], c["no"]): c["c"] for c in gal.cards}
    result: dict[str, object] = {"tag": tag, "folders": {}, "synthetic": {}}
    for name in folders:
        ims, truth = folder_set(name, card_of)
        result["folders"][name] = {  # type: ignore[index]
            "center": score(rec, gal, np.stack([center_crop(i) for i in ims]), truth),
            "whole": score(rec, gal, np.stack([squash(i) for i in ims]), truth),
        }
        print(name, json.dumps(result["folders"][name]), flush=True)  # type: ignore[index]
    meta = json.loads((paths.work() / "gallery.json").read_text())
    qidx = json.loads((paths.work() / "queries.json").read_text())
    truth = [meta[i]["card_id"] for i in qidx]
    for cond in ("rect_mild", "rect_hard", "loose_crop"):
        files = [paths.work() / "queries" / cond / f"{i}.jpg" for i in range(len(qidx))]
        if not all(f.exists() for f in files):
            continue
        arr = np.stack([cv2.cvtColor(cv2.imread(str(f)), cv2.COLOR_BGR2RGB) for f in files])
        result["synthetic"][cond] = score(rec, gal, arr, truth)  # type: ignore[index]
        print(cond, json.dumps(result["synthetic"][cond]), flush=True)  # type: ignore[index]
    out = paths.work() / "eval" / f"{tag}-rec.json"
    out.parent.mkdir(exist_ok=True)
    out.write_text(json.dumps(result, ensure_ascii=False, indent=1))
    print("->", out)


if __name__ == "__main__":
    main()
