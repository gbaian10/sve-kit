"""Hubness correction (CSLS-style) for the index: r_g = mean top-k similarity of gallery row g to the synthetic
query photos of OTHER cards. Search score = cos(q, g) - hubWeight * r_g, which stops a few "hub" cards from
winning every uncertain photo. Recompute whenever the recogniser or the gallery changes.

usage: python -m sve_scan rec-hub <arch> <tag>   ->  SVE_SCAN_WORK/hub_r_<tag>.npy (float32, one per gallery row)
"""

import json
import sys

import numpy as np
import torch

from sve_scan import gallery as GL
from sve_scan import paths
from sve_scan import rec_train as T

K = 10


def main() -> None:
    arch, tag = sys.argv[1], sys.argv[2]
    _, meta = GL.current(paths.work())
    model = T.Embedder(arch).to(T.DEV)
    model.load_state_dict(torch.load(paths.work() / f"model_{tag}.pt", map_location=T.DEV))
    model.eval()
    g = T.embed_uint8(model, np.load(paths.work() / "gallery.npy", mmap_mode="r"), bs=32)
    card_idx = {c: i for i, c in enumerate(np.unique(T.card_of))}
    gcard = torch.tensor([card_idx[c] for c in T.card_of], device=T.DEV)
    # reference set: synthetic queries (both orientations), each knows its source card
    refs, rcard = [], []
    for cond in T.COND:
        q = T.load_queries(cond)
        for arr in (q, np.ascontiguousarray(q[:, ::-1, ::-1])):
            refs.append(T.embed_uint8(model, arr, bs=32))
            rcard.append(gcard[torch.from_numpy(T.qidx).to(T.DEV)])
    ref, ref_card = torch.cat(refs), torch.cat(rcard)
    r = torch.empty(len(g), device=T.DEV)
    for s in range(0, len(g), 2048):
        sim = ref @ g[s : s + 2048].T  # [n_ref, chunk]
        sim[ref_card[:, None] == gcard[None, s : s + 2048]] = -1  # ignore a card's own synthetic photos
        r[s : s + 2048] = sim.topk(K, dim=0).values.mean(0)
    print(f"r_g: mean {r.mean():.3f}, p99 {r.quantile(0.99):.3f}, max {r.max():.3f}")
    print("biggest hubs:", [T.meta[i]["card_no"] for i in r.topk(8).indices.cpu().numpy()])
    np.save(paths.work() / f"hub_r_{tag}.npy", r.cpu().numpy().astype(np.float32))
    (paths.work() / f"hub_r_{tag}.json").write_text(json.dumps({"rows_sha256": meta["rows_sha256"]}))


if __name__ == "__main__":
    main()
