"""Real-photo quad index for corner-finder training from the CC0 Pokémon card segmentation set
(polygon masks fitted to clockwise quads, original pixel coordinates).

usage: python -m sve_scan det-real-build [--index out.jsonl] [--report-dir dir]
"""

import argparse
import collections
import json
from pathlib import Path

import cv2
import numpy as np
from PIL import Image

from sve_scan import paths

cv2.setNumThreads(3)
POKEMON = paths.det_photos() / "pokemon-card-segmentation"  # CC0 card photos with polygon masks


def signed_area(q):
    return float(np.sum(q[:, 0] * np.roll(q[:, 1], -1) - q[:, 1] * np.roll(q[:, 0], -1)) / 2)


def clockwise(q):
    q = np.asarray(q, dtype=np.float64)
    return q if signed_area(q) > 0 else q[::-1].copy()


def validate(q, w, h):
    if q.shape != (4, 2) or not np.isfinite(q).all():
        raise ValueError("nonfinite_or_wrong_shape")
    e = np.roll(q, -1, axis=0) - q
    lens = np.linalg.norm(e, axis=1)
    if lens.min() < max(3, min(w, h) * 0.005):
        raise ValueError("short_edge")
    cross = e[:, 0] * np.roll(e[:, 1], -1) - e[:, 1] * np.roll(e[:, 0], -1)
    if np.any(cross <= 0.001 * lens * np.roll(lens, -1)) or signed_area(q) < 25:
        raise ValueError("nonconvex_or_degenerate")
    if np.any(q < [-0.1 * w, -0.1 * h]) or np.any(q > [1.1 * w, 1.1 * h]):
        raise ValueError("intersection_far_outside_image")


def polygon_quad(points, w, h):
    """Four contiguous sides, robust fitted lines, adjacent intersections.

    A hull approximation locates the side breaks, but its vertices are never
    returned as the result. Dense contours are assigned to these four sides;
    the end 15% is excluded to avoid fitting through rounded corners.
    """
    p = np.asarray(points, np.float64)
    if not np.isfinite(p).all():
        raise ValueError("nonfinite_polygon")
    p = p[np.r_[True, np.linalg.norm(np.diff(p, axis=0), axis=1) > 1e-5]]
    if len(p) > 1 and np.linalg.norm(p[-1] - p[0]) < 1e-5:
        p = p[:-1]
    if len(p) < 4:
        raise ValueError("too_few_points")
    p = clockwise(p)
    if len(p) == 4:
        validate(p, w, h)
        seed = p.copy()
    else:
        hull = cv2.convexHull(p.astype(np.float32)).reshape(-1, 2)
        perimeter = cv2.arcLength(hull, True)
        seed = None
        for epsilon in np.linspace(0.003, 0.08, 100):
            approx = cv2.approxPolyDP(hull, epsilon * perimeter, True).reshape(-1, 2)
            if len(approx) == 4:
                seed = clockwise(approx)
                break
        if seed is None:
            raise ValueError("cannot_split_four_sides")
    # Resampling also supports sparse edges (the source's common 4-point case).
    dense = np.concatenate(
        [
            a + np.linspace(0, 1, max(8, int(np.linalg.norm(b - a) / 3)), endpoint=False)[:, None] * (b - a)
            for a, b in zip(p, np.roll(p, -1, axis=0))
        ]
    )
    edges = np.roll(seed, -1, axis=0) - seed
    t = np.sum((dense[:, None, :] - seed) * edges, axis=2) / np.sum(edges * edges, axis=1)
    nearest = seed + np.clip(t, 0, 1)[:, :, None] * edges
    distance = np.linalg.norm(dense[:, None, :] - nearest, axis=2)
    assigned = np.argmin(distance, axis=1)
    lines = []
    for i in range(4):
        pts = dense[(assigned == i) & (t[:, i] >= 0.15) & (t[:, i] <= 0.85)]
        if len(pts) < 2:
            raise ValueError("insufficient_edge_points")
        vx, vy, x, y = cv2.fitLine(pts.astype(np.float32), cv2.DIST_HUBER, 0, 0.01, 0.01).ravel()
        normal = np.array([-vy, vx], np.float64)
        lines.append((normal, float(normal @ [x, y])))
    result = []
    for i in range(4):
        a, b = lines[i - 1], lines[i]
        normals = np.stack([a[0], b[0]])
        if abs(np.linalg.det(normals)) < 0.01:
            raise ValueError("parallel_adjacent_lines")
        result.append(np.linalg.solve(normals, [a[1], b[1]]))
    q = clockwise(result)
    validate(q, w, h)
    return q


def preview(rows, dataset, dest):
    selected = []
    groups = [[r for r in rows if r["dataset"] == dataset and r["split"] == split] for split in ("train", "val")]
    nonempty = [group for group in groups if group]
    for subset in nonempty:
        selected.extend(subset[int(i)] for i in np.linspace(0, len(subset) - 1, 12 // len(nonempty)))
    tiles = []
    for row in selected:
        im = cv2.imread(row["image"])
        scale = min(480 / row["w"], 310 / row["h"])
        im = cv2.resize(im, (round(row["w"] * scale), round(row["h"] * scale)), interpolation=cv2.INTER_AREA)
        tile = np.full((350, 480, 3), 28, np.uint8)
        ox, oy = (480 - im.shape[1]) // 2, 35 + (310 - im.shape[0]) // 2
        tile[oy : oy + im.shape[0], ox : ox + im.shape[1]] = im
        for quad in row["quads"]:
            q = np.rint(np.array(quad) * scale + [ox, oy]).astype(np.int32)
            cv2.polylines(tile, [q], True, (50, 255, 50), 1, cv2.LINE_AA)
            for i, point in enumerate(q):
                cv2.circle(tile, tuple(point), 3, (0, 80, 255), -1)
                cv2.putText(tile, str(i), tuple(point + [4, -4]), 0, 0.4, (0, 255, 255), 1, cv2.LINE_AA)
        short = str(Path(row["image"]).relative_to(POKEMON))
        cv2.putText(tile, row["split"] + " " + short, (8, 20), 0, 0.34, (240, 240, 240), 1, cv2.LINE_AA)
        if len(nonempty) == 1:
            cv2.putText(
                tile,
                "Source val split is EMPTY; original split preserved",
                (8, 344),
                0,
                0.38,
                (0, 200, 255),
                1,
                cv2.LINE_AA,
            )
        tiles.append(tile)
    sheet = np.concatenate([np.concatenate(tiles[i : i + 4], 1) for i in range(0, 12, 4)])
    if not cv2.imwrite(str(dest), sheet):
        raise OSError(dest)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--index", type=Path, default=paths.work() / "realdet_pokemon.jsonl")
    parser.add_argument("--report-dir", type=Path, default=paths.work() / "realdet-report")
    args = parser.parse_args()
    rows = []
    stats = {
        "images": collections.Counter(),
        "quads_kept": collections.Counter(),
        "dropped": {"pokemon": collections.Counter()},
        "pokemon_raw_points": collections.Counter(),
    }
    for dataset in ("pokemon",):
        for split in ("train", "val"):
            stats["images"][dataset + "/" + split] = 0
            stats["quads_kept"][dataset + "/" + split] = 0
    for split in ("train", "val"):
        for label in sorted((POKEMON / "labels" / split).glob("*.txt")):
            image = POKEMON / "images" / split / (label.stem + ".jpg")
            with Image.open(image) as im:
                w, h = im.size
            quads = []
            for line in label.read_text().splitlines():
                try:
                    p = np.array([float(v) for v in line.split()[1:]]).reshape(-1, 2)
                    stats["pokemon_raw_points"][str(len(p))] += 1
                    q = polygon_quad(p * [w, h], w, h)
                    quads.append(q.tolist())
                except ValueError as exc:
                    stats["dropped"]["pokemon"][str(exc)] += 1
            rows.append(dict(image=str(image), w=w, h=h, dataset="pokemon", split=split, quads=quads))
    for row in rows:
        key = row["dataset"] + "/" + row["split"]
        stats["images"][key] += 1
        stats["quads_kept"][key] += len(row["quads"])
    args.index.parent.mkdir(parents=True, exist_ok=True)
    args.report_dir.mkdir(parents=True, exist_ok=True)
    with args.index.open("w") as f:
        for row in rows:
            f.write(json.dumps(row, separators=(",", ":")) + "\n")
    for dataset in ("pokemon",):
        preview(rows, dataset, args.report_dir / f"check_{dataset}.jpg")
    (args.report_dir / "build_stats.json").write_text(json.dumps(stats, indent=2) + "\n")
    print(json.dumps(stats, indent=2))


if __name__ == "__main__":
    main()
