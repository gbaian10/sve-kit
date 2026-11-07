"""Card index for one recogniser: embed every gallery image with the shipped int8 model, quantise per row,
add the hub values, and split by edition (docs/schema/scan/artifacts.md).

Rebuild after every new export (new cards) and whenever the recogniser changes; the hub file must be
recomputed together with it (`rec-hub`), or one card starts winning uncertain photos.

usage: python -m sve_scan index <tag>   ->  SVE_SCAN_WORK/index/<tag>/{jp,en}.{i8,scale,hub,json}
"""

import json
import sys

import numpy as np

from sve_scan import artifacts, paths, runtime


def card(m: dict) -> artifacts.Json:
    return {
        "no": m["card_no"],
        "c": m["card_id"],
        "n": m["n"],
        "f": m["f"],
        "nm": m["name"],
        "a": m["art_id"] or "",
        "k": m["class"],
        "y": m["type"],
    }


def main() -> None:
    tag = sys.argv[1]
    work = paths.work()
    meta = json.loads((work / "gallery.json").read_text())
    gallery = np.load(work / "gallery.npy", mmap_mode="r")
    hub = np.load(work / f"hub_r_{tag}.npy").astype(np.float32)
    if len(hub) != len(meta) or len(gallery) != len(meta):
        raise SystemExit(f"gallery ({len(meta)}), images ({len(gallery)}) and hub ({len(hub)}) disagree: rerun rec-hub")
    rec = runtime.Recogniser(runtime.session(work / f"{tag}.w8.onnx"))
    emb = rec.embed(gallery, bs=64)
    vectors, scale = artifacts.quantize(emb)
    out = runtime.index_dir(tag)
    for ed in artifacts.EDITIONS:
        rows = np.array([i for i, m in enumerate(meta) if m["region"] == ed])
        if len(rows) == 0:
            continue
        idx = artifacts.Index(vectors[rows], scale[rows], hub[rows], [card(meta[i]) for i in rows])
        files = artifacts.write_index(out, ed, idx)
        print(ed, len(rows), "cards ->", ", ".join(p.name for p in files.values()))


if __name__ == "__main__":
    main()
