"""Download CC0 table-like textures (1k diffuse JPG) from Poly Haven into SVE_SCAN_WORK/bg/ with a manifest.

Poly Haven assets are CC0 (https://polyhaven.com/license). Requests are at least 2 s apart (fetch.Polite) and every
file keeps its source URL, fetch time, ETag and SHA-256 in bg/manifest.json.
usage: python -m sve_scan fetch-bg
"""

from sve_scan import paths
from sve_scan.fetch import Polite

API = "https://api.polyhaven.com"
web = Polite(paths.work() / "bg")


def assets(cat: str) -> list[str]:
    return list(web.json(f"{API}/assets?type=textures&categories={cat}"))


picks: list[tuple[str, str]] = []
wood = [a for a in assets("wood") if not any(k in a for k in ("bark", "wall", "log", "branch", "root", "chip"))]
picks += [("wood", a) for a in wood[:40]]
for cat, n in (("fabric", 25), ("leather", 6), ("tiles", 6), ("metal", 4), ("plaster-concrete", 6)):
    picks += [(cat, a) for a in assets(cat)[:n]]

seen = set()
for cat, aid in picks:
    if aid in seen:
        continue
    seen.add(aid)
    name = f"{aid}.jpg"
    meta = {"id": aid, "category": cat, "source": f"https://polyhaven.com/a/{aid}", "license": "CC0"}
    if web.complete(name):
        web.download(web.records[name]["url"], name, **meta)
        continue
    files = web.json(f"{API}/files/{aid}")
    diff = files.get("Diffuse") or files.get("diffuse") or files.get("Color")
    if not diff or "1k" not in diff or "jpg" not in diff["1k"]:
        print("skip (no 1k diffuse jpg):", aid)
        continue
    web.download(diff["1k"]["jpg"]["url"], name, **meta)
    print(len(web.records), cat, aid, flush=True)
web.save()
