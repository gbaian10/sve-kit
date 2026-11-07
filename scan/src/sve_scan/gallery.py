"""Training gallery from a public export, plus fixed synthetic query photos for quick checks.

Outputs (in SVE_SCAN_WORK):
  gallery.json      one row per printing face with an image, sorted by image key:
                    {n, f, printing_id, face_id, card_id, region, card_no, rarity, art_id, class, type, name}
  gallery.npy       uint8 [N, 336, 240, 3] official card images, resized
  queries/<cond>/<i>.jpg and queries.json (gallery rows the queries come from)
  sheet_<cond>.jpg  contact sheet (official | synthetic)

usage: python -m sve_scan gallery [--regallery]
"""

import json
import sys
from multiprocessing import Pool
from pathlib import Path

import cv2
import numpy as np

from sve_scan import paths

W, H = 240, 336
N_QUERIES = 1500


def image_file(n: int, f: int, size: str = "card_l") -> Path:
    """The export's permanent image key: images/<size>/<int_id>[-f<ordinal>].webp."""
    return paths.card_images(size) / (f"{n}.webp" if f == 0 else f"{n}-f{f}.webp")


def load_rgb(key: tuple[int, int]) -> np.ndarray:
    bgr = cv2.imread(str(image_file(*key)), cv2.IMREAD_COLOR)
    if bgr is None:
        raise FileNotFoundError(image_file(*key))
    return cv2.cvtColor(bgr, cv2.COLOR_BGR2RGB)


def load_small(key: tuple[int, int]) -> np.ndarray:
    return cv2.resize(load_rgb(key), (W, H), interpolation=cv2.INTER_AREA)


def build_gallery() -> list[dict]:
    """Every approved, available printing face image of the export's current snapshot (both editions)."""
    from sve_carddb.export.read_api import load_snapshot  # the export's verified reader, as publish/ uses

    snapshot = load_snapshot(paths.export())
    tables = snapshot.tables
    (paths.work() / "gallery-meta.json").write_text(
        json.dumps({"data_version": snapshot.manifest["data_version"], "manifest_sha256": snapshot.manifest_sha256})
    )
    prints = {r["id"]: r for r in tables["printing"]}
    faces = {r["id"]: r for r in tables["face"]}
    revisions = {r["id"]: r for r in tables["face_revision"]}
    texts = {r["id"]: r["text"] for r in tables["text_unit"]}
    gallery = []
    for media in tables["printing_image"]:
        if media["publication_state"] != "approved" or media["availability"] != "available":
            continue
        p, face = prints[media["printing_id"]], faces[media["face_id"]]
        art = next((pf["art_id"] for pf in p["faces"] if pf["face_id"] == face["id"]), None)
        rev_id = next((c["revision_id"] for c in face["current"] if c["region"] == p["region"]), None)
        rev = revisions.get(rev_id, {})
        gallery.append(
            {
                "n": p["int_id"],
                "f": face["ordinal"],
                "printing_id": p["id"],
                "face_id": face["id"],
                "card_id": p["card_id"],
                "region": p["region"],
                "card_no": p["card_no"],
                "rarity": p["rarity_raw"],
                "art_id": art,
                "class": rev.get("class_code") or "",
                "type": rev.get("type_code") or "",
                "name": texts.get(rev.get("name_unit_id"), ""),
            }
        )
    gallery.sort(key=lambda g: (g["n"], g["f"]))
    return gallery


# ---------- synthetic degradations ----------


def photometric(img: np.ndarray, rng: np.random.Generator, s: float) -> np.ndarray:
    x = img.astype(np.float32) / 255.0
    gains = 1.0 + rng.uniform(-0.12, 0.12, 3) * s
    x = x * gains
    x = (x - 0.5) * (1.0 + rng.uniform(-0.25, 0.25) * s) + 0.5 + rng.uniform(-0.15, 0.15) * s
    gray = x.mean(axis=2, keepdims=True)
    x = gray + (x - gray) * (1.0 + rng.uniform(-0.3, 0.3) * s)
    x = np.clip(x, 0, 1) ** rng.uniform(1 - 0.25 * s, 1 + 0.3 * s)
    return (np.clip(x, 0, 1) * 255).astype(np.uint8)


def glare(img: np.ndarray, rng: np.random.Generator, max_alpha: float) -> np.ndarray:
    h, w = img.shape[:2]
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    mask = np.zeros((h, w), np.float32)
    for _ in range(rng.integers(1, 3)):
        if rng.random() < 0.5:  # elliptical highlight
            cx, cy = rng.uniform(0, w), rng.uniform(0, h)
            ax, ay = rng.uniform(0.1, 0.5) * w, rng.uniform(0.05, 0.3) * h
            d = ((xx - cx) / ax) ** 2 + ((yy - cy) / ay) ** 2
            mask = np.maximum(mask, np.exp(-d * 2))
        else:  # diagonal streak (sleeve reflection)
            ang = rng.uniform(0, np.pi)
            off = rng.uniform(-0.5, 0.5) * (w + h)
            width = rng.uniform(0.03, 0.15) * (w + h)
            d = np.abs((xx - w / 2) * np.cos(ang) + (yy - h / 2) * np.sin(ang) - off)
            mask = np.maximum(mask, np.exp(-((d / width) ** 2)))
    a = mask[..., None] * rng.uniform(0.15, max_alpha)
    out = img.astype(np.float32) * (1 - a) + 255.0 * a
    return np.clip(out, 0, 255).astype(np.uint8)


def occlude(img: np.ndarray, rng: np.random.Generator, other: np.ndarray) -> np.ndarray:
    h, w = img.shape[:2]
    out = img.copy()
    frac = rng.uniform(0.08, 0.25)
    if rng.random() < 0.5:  # finger-ish blob from an edge
        color = np.array([rng.uniform(170, 230), rng.uniform(120, 180), rng.uniform(100, 150)])
        cx = rng.choice([0, w]) if rng.random() < 0.5 else rng.uniform(0, w)
        cy = rng.uniform(0, h) if cx in (0, w) else rng.choice([0, h])
        r = np.sqrt(frac * w * h / np.pi)
        cv2.ellipse(
            out, (int(cx), int(cy)), (int(r * 0.7), int(r * 1.4)), rng.uniform(0, 180), 0, 360, color.tolist(), -1
        )
    else:  # another card overlapping a corner
        cw, ch = int(w * np.sqrt(frac * 2)), int(h * np.sqrt(frac * 2))
        patch = cv2.resize(other, (cw, ch))
        corner = rng.integers(4)
        y0 = 0 if corner < 2 else h - ch
        x0 = 0 if corner % 2 == 0 else w - cw
        out[y0 : y0 + ch, x0 : x0 + cw] = patch
    return out


def degrade(
    img: np.ndarray, rng: np.random.Generator, target_h: float, blur: float, noise: float, jpeg: int
) -> np.ndarray:
    h, w = img.shape[:2]
    th = int(max(24, target_h))
    tw = int(th * w / h)
    small = cv2.resize(img, (tw, th), interpolation=cv2.INTER_AREA)
    if blur > 0.05:
        small = cv2.GaussianBlur(small, (0, 0), blur)
    if rng.random() < 0.25:  # motion blur
        k = int(rng.integers(3, 8))
        kern = np.zeros((k, k), np.float32)
        kern[k // 2, :] = 1.0 / k
        m = cv2.getRotationMatrix2D((k / 2, k / 2), rng.uniform(0, 180), 1.0)
        kern = cv2.warpAffine(kern, m, (k, k))
        kern /= max(kern.sum(), 1e-6)
        small = cv2.filter2D(small, -1, kern)
    small = np.clip(small.astype(np.float32) + rng.normal(0, noise, small.shape), 0, 255).astype(np.uint8)
    ok, enc = cv2.imencode(".jpg", cv2.cvtColor(small, cv2.COLOR_RGB2BGR), [cv2.IMWRITE_JPEG_QUALITY, jpeg])
    small = cv2.cvtColor(cv2.imdecode(enc, cv2.IMREAD_COLOR), cv2.COLOR_BGR2RGB)
    return cv2.resize(small, (w, h), interpolation=cv2.INTER_LINEAR)


def rectified(card: np.ndarray, rng: np.random.Generator, hard: bool, other: np.ndarray) -> np.ndarray:
    """Card already found and warped upright, with residual corner error."""
    j = 0.06 if hard else 0.03
    src = np.float32([[0, 0], [card.shape[1], 0], [card.shape[1], card.shape[0]], [0, card.shape[0]]])
    dst = (np.float32([[0, 0], [W, 0], [W, H], [0, H]]) + rng.uniform(-j, j, (4, 2)) * np.float32([W, H])).astype(
        np.float32
    )
    border = rng.uniform(0, 80, 3).tolist()
    img = cv2.warpPerspective(
        card, cv2.getPerspectiveTransform(src, dst), (W, H), flags=cv2.INTER_AREA, borderValue=border
    )
    if hard and rng.random() < 0.5:
        img = occlude(img, rng, other)
    if rng.random() < (0.8 if hard else 0.6):
        img = glare(img, rng, 0.6 if hard else 0.35)
    img = photometric(img, rng, 1.0 if hard else 0.6)
    if hard:
        img = degrade(
            img, rng, rng.uniform(70, 130), rng.uniform(0.3, 1.8), rng.uniform(2, 8), int(rng.integers(40, 80))
        )
    else:
        img = degrade(
            img, rng, rng.uniform(140, 336), rng.uniform(0, 1.0), rng.uniform(0, 4), int(rng.integers(55, 90))
        )
    return img


def background(rng: np.random.Generator, size: int, art: np.ndarray) -> np.ndarray:
    if rng.random() < 0.5:  # playmat with art
        bg = cv2.resize(art, (size, size))
        bg = cv2.GaussianBlur(bg, (0, 0), rng.uniform(2, 8))
        bg = (bg.astype(np.float32) * rng.uniform(0.3, 0.8)).astype(np.uint8)
    else:  # plain table / mat
        base = rng.uniform(20, 200, 3)
        bg = np.ones((size, size, 3), np.float32) * base
        g = np.linspace(-1, 1, size)[None, :, None] * rng.uniform(-30, 30)
        bg = np.clip(bg + g + rng.normal(0, rng.uniform(2, 12), bg.shape), 0, 255).astype(np.uint8)
    return bg


def card_quad(rng: np.random.Generator, cx: float, cy: float, ch: float, rot: float) -> np.ndarray:
    """Corners of a card seen from across a table: foreshortened top, in-plane rotation."""
    cw = ch * 459 / 641
    squash = rng.uniform(0.6, 1.0)  # viewing tilt
    top = rng.uniform(0.7, 1.0)  # far edge narrower
    pts = np.float32(
        [
            [-cw / 2 * top, -ch / 2 * squash],
            [cw / 2 * top, -ch / 2 * squash],
            [cw / 2, ch / 2 * squash],
            [-cw / 2, ch / 2 * squash],
        ]
    )
    c, s = np.cos(rot), np.sin(rot)
    pts = pts @ np.float32([[c, s], [-s, c]])
    return (pts + np.float32([cx, cy])).astype(np.float32)


def loose_crop(card: np.ndarray, rng: np.random.Generator, others: list[np.ndarray]) -> np.ndarray:
    """No corner finding: crop the bounding box around the card in a cluttered scene."""
    size = 900
    scene = background(rng, size, others[0])
    ch = rng.uniform(260, 420)
    rot = rng.uniform(-0.35, 0.35) + (np.pi if rng.random() < 0.5 else 0)
    src = np.float32([[0, 0], [459, 0], [459, 641], [0, 641]])
    # neighbours first so the target lies on top
    for o in others[1:]:
        dx, dy = rng.choice([-1, 1]) * rng.uniform(0.75, 1.0) * ch * 0.75, rng.uniform(-0.3, 0.3) * ch
        q = card_quad(rng, size / 2 + dx, size / 2 + dy, ch, rot + rng.uniform(-0.1, 0.1))
        m = cv2.getPerspectiveTransform(src, q)
        warped = cv2.warpPerspective(o, m, (size, size))
        mask = cv2.warpPerspective(np.ones((641, 459), np.uint8), m, (size, size)) > 0
        scene[mask] = warped[mask]
    q = card_quad(rng, size / 2, size / 2, ch, rot)
    m = cv2.getPerspectiveTransform(src, q)
    warped = cv2.warpPerspective(card, m, (size, size))
    mask = cv2.warpPerspective(np.ones((641, 459), np.uint8), m, (size, size)) > 0
    scene[mask] = warped[mask]
    x0, y0 = q.min(0)
    x1, y1 = q.max(0)
    mx, my = (x1 - x0) * rng.uniform(0.03, 0.15), (y1 - y0) * rng.uniform(0.03, 0.15)
    x0, y0 = int(max(0, x0 - mx)), int(max(0, y0 - my))
    x1, y1 = int(min(size, x1 + mx)), int(min(size, y1 + my))
    crop = scene[y0:y1, x0:x1]
    if rng.random() < 0.6:
        crop = glare(crop, rng, 0.35)
    crop = photometric(crop, rng, 0.6)
    crop = cv2.resize(crop, (W, H), interpolation=cv2.INTER_AREA)
    return degrade(crop, rng, rng.uniform(140, 336), rng.uniform(0, 1.0), rng.uniform(0, 4), int(rng.integers(55, 90)))


COND = ("rect_mild", "rect_hard", "loose_crop")


def make_query(args: tuple[int, int, int, list[int]]) -> None:
    i, seed, n, other_ns = args
    rng = np.random.default_rng(seed)
    card = load_rgb(n)
    others = [load_rgb(o) for o in other_ns]
    out = {
        "rect_mild": rectified(card, rng, False, others[0]),
        "rect_hard": rectified(card, rng, True, others[0]),
        "loose_crop": loose_crop(card, rng, others),
    }
    for cond, img in out.items():
        if cond != "loose_crop" and rng.random() < 0.5:
            img = np.ascontiguousarray(img[::-1, ::-1])  # seen from the opponent's side
        cv2.imwrite(
            str(paths.work() / "queries" / cond / f"{i}.jpg"),
            cv2.cvtColor(img, cv2.COLOR_RGB2BGR),
            [cv2.IMWRITE_JPEG_QUALITY, 95],
        )


def main() -> None:
    OUT = paths.work()
    for c in COND:
        (OUT / "queries" / c).mkdir(parents=True, exist_ok=True)
    gallery = build_gallery()
    (OUT / "gallery.json").write_text(json.dumps(gallery, ensure_ascii=False))
    print("gallery", len(gallery), flush=True)

    rng = np.random.default_rng(0)
    idx = rng.choice(len(gallery), N_QUERIES, replace=False)
    jobs = []
    for i, gi in enumerate(idx):
        other = rng.choice(len(gallery), 3, replace=False)
        jobs.append(
            (
                i,
                int(rng.integers(1 << 31)),
                (gallery[gi]["n"], gallery[gi]["f"]),
                [(gallery[o]["n"], gallery[o]["f"]) for o in other],
            )
        )
    (OUT / "queries.json").write_text(json.dumps([int(g) for g in idx]))
    with Pool(4) as pool:
        for k, _ in enumerate(pool.imap_unordered(make_query, jobs, chunksize=16)):
            if k % 300 == 0:
                print("queries", k, flush=True)

    if not (OUT / "gallery.npy").exists() or "--regallery" in sys.argv:
        with Pool(4) as pool:
            arr = np.stack(pool.map(load_small, [(g["n"], g["f"]) for g in gallery], chunksize=64))
        np.save(OUT / "gallery.npy", arr)
        print("gallery.npy", arr.shape, flush=True)

    for cond in COND:
        rows = []
        for i in range(8):
            g = gallery[idx[i]]
            a = load_small((g["n"], g["f"]))
            b = cv2.cvtColor(cv2.imread(str(OUT / "queries" / cond / f"{i}.jpg")), cv2.COLOR_BGR2RGB)
            rows.append(np.concatenate([a, b], axis=0))
        sheet = np.concatenate(rows, axis=1)
        cv2.imwrite(str(OUT / f"sheet_{cond}.jpg"), cv2.cvtColor(sheet, cv2.COLOR_RGB2BGR))
    print("done", flush=True)


if __name__ == "__main__":
    main()
