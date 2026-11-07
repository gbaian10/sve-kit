"""Download CC0 table-like textures (1k diffuse JPG) from Poly Haven into SVE_SCAN_WORK/bg/ with a manifest.

Poly Haven assets are CC0 (https://polyhaven.com/license). One request per second.
usage: python -m sve_scan fetch-bg
"""

import json
import time
import urllib.request

from sve_scan import paths

OUT = paths.work() / "bg"
OUT.mkdir(exist_ok=True)
UA = {"User-Agent": "sve-kit-scan/0 (texture fetch for synthetic training data)"}


def get(url: str) -> bytes:
    with urllib.request.urlopen(urllib.request.Request(url, headers=UA), timeout=60) as r:
        return r.read()


def assets(cat: str) -> list[str]:
    time.sleep(1)
    return list(json.loads(get(f"https://api.polyhaven.com/assets?type=textures&categories={cat}")))


picks: list[tuple[str, str]] = []
wood = [a for a in assets("wood") if not any(k in a for k in ("bark", "wall", "log", "branch", "root", "chip"))]
picks += [("wood", a) for a in wood[:40]]
for cat, n in (("fabric", 25), ("leather", 6), ("tiles", 6), ("metal", 4), ("plaster-concrete", 6)):
    picks += [(cat, a) for a in assets(cat)[:n]]

seen, manifest = set(), []
for cat, aid in picks:
    if aid in seen:
        continue
    seen.add(aid)
    dst = OUT / f"{aid}.jpg"
    if not dst.exists():
        time.sleep(1)
        files = json.loads(get(f"https://api.polyhaven.com/files/{aid}"))
        diff = files.get("Diffuse") or files.get("diffuse") or files.get("Color")
        if not diff or "1k" not in diff or "jpg" not in diff["1k"]:
            print("skip (no 1k diffuse jpg):", aid)
            continue
        url = diff["1k"]["jpg"]["url"]
        time.sleep(1)
        dst.write_bytes(get(url))
    else:
        url = ""
    manifest.append(
        {
            "id": aid,
            "category": cat,
            "file": dst.name,
            "source": f"https://polyhaven.com/a/{aid}",
            "license": "CC0",
            "url": url,
        }
    )
    print(len(manifest), cat, aid)
(OUT / "manifest.json").write_text(json.dumps(manifest, indent=1))
