"""Card index for one recogniser: embed every gallery image with the shipped int8 model, quantise per row,
add the hub values, and split by edition (docs/schema/scan/artifacts.md).

Rebuild after every new export (new cards) and whenever the recogniser changes; the hub file must be
recomputed together with it (`rec-hub`), or one card starts winning uncertain photos.

usage: python -m sve_scan index <tag>   ->  SVE_SCAN_WORK/index/<tag>/{jp,en}.{i8,scale,hub,json}
"""

import json
import sys
from pathlib import Path

import numpy as np

from sve_scan import artifacts, paths, runtime
from sve_scan import gallery as GL


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


def build(work: Path, tag: str, rec: runtime.Recogniser) -> artifacts.Json:
    """Write index/<tag>/ from scratch: every edition the gallery has now, then build.json naming exactly those."""
    meta, gmeta = GL.current(work)
    gallery = np.load(work / "gallery.npy", mmap_mode="r")
    hub = np.load(work / f"hub_r_{tag}.npy").astype(np.float32)
    hub_for = json.loads((work / f"hub_r_{tag}.json").read_text()).get("rows_sha256")
    if len(gallery) != len(meta) or len(hub) != len(meta) or hub_for != gmeta["rows_sha256"]:
        raise SystemExit("the hub values were computed for another gallery: rerun rec-hub")
    vectors, scale = artifacts.quantize(rec.embed(gallery, bs=64))
    out = work / "index" / tag
    out.mkdir(parents=True, exist_ok=True)
    # an earlier build stops counting before any of its files changes, and none of its files outlive it:
    # release reads only build.json, which is written last and lists what this build made
    (out / "build.json").unlink(missing_ok=True)
    for old in out.iterdir():
        old.unlink()
    editions = []
    for ed in artifacts.EDITIONS:
        rows = np.array([i for i, m in enumerate(meta) if m["region"] == ed])
        if len(rows) == 0:
            continue
        idx = artifacts.Index(vectors[rows], scale[rows], hub[rows], [card(meta[i]) for i in rows])
        files = artifacts.write_index(out, ed, idx)
        editions.append(ed)
        print(ed, len(rows), "cards ->", ", ".join(p.name for p in files.values()))
    record = {
        "data_version": gmeta["data_version"],
        "manifest_sha256": gmeta["manifest_sha256"],
        "rows_sha256": gmeta["rows_sha256"],
        "rec_sha256": artifacts.sha256(work / f"{tag}.w8.onnx"),
        "editions": editions,
    }
    (out / "build.json").write_text(json.dumps(record, indent=1) + "\n")
    return record


def main() -> None:
    tag = sys.argv[1]
    work = paths.work()
    build(work, tag, runtime.Recogniser(runtime.session(work / f"{tag}.w8.onnx")))


if __name__ == "__main__":
    main()
