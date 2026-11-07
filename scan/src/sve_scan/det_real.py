"""CPU real-photo sampler for cornerdet's clockwise, cyclic-start labels.

Images in the requested split are eagerly cached in RGB, at most 960 pixels on
the long side. Constructor I/O is separate from batch latency (about 3 GiB for
the training split). No torch import, GPU work, or background workers.
"""

import argparse
import json
import time
from pathlib import Path

import cv2
import numpy as np

from sve_scan import paths

cv2.setNumThreads(3)


def signed_area(q):
    return float(np.sum(q[:, 0] * np.roll(q[:, 1], -1) - q[:, 1] * np.roll(q[:, 0], -1)) / 2)


class RealDet:
    def __init__(
        self,
        index: str,
        split: str = "train",
        size: int = 256,
        seed: int = 0,
        zoom_p: float = 0.0,
        inside_p: float = 0.0,
    ):
        # zoom_p: share of positive crops where the target card is 70-160% of the view (close-ups)
        # inside_p: share of negative crops cut from inside one card, away from its centre (card art)
        self.zoom_p, self.inside_p = zoom_p, inside_p
        if not isinstance(size, int) or size < 1:
            raise ValueError("size must be a positive integer")
        self.size = size
        self.rng = np.random.default_rng(seed)
        with open(index) as f:
            self.rows = [row for line in f if (row := json.loads(line))["split"] == split]
        if not self.rows:
            raise ValueError(f"No images for split {split!r}")
        self.images, self.quads, self.centers, self.ignores = [], [], [], []
        self.positive_ids = []
        for i, row in enumerate(self.rows):
            w, h = row["w"], row["h"]
            # JPEG decoder reduction avoids decoding a full 4000px Pokémon image.
            cap = row.get("max_side", 960)  # deck photos keep more pixels: their cards are small
            reduction = next(k for k in (8, 4, 2, 1) if max(w, h) / k >= min(cap, max(w, h)))
            flag = {
                1: cv2.IMREAD_COLOR,
                2: cv2.IMREAD_REDUCED_COLOR_2,
                4: cv2.IMREAD_REDUCED_COLOR_4,
                8: cv2.IMREAD_REDUCED_COLOR_8,
            }[reduction]
            image = cv2.imread(row["image"], flag | cv2.IMREAD_IGNORE_ORIENTATION)
            if image is None:
                raise OSError(f"Cannot read {row['image']}")
            scale = min(1.0, cap / max(image.shape[:2]))
            if scale < 1:
                image = cv2.resize(
                    image, (round(image.shape[1] * scale), round(image.shape[0] * scale)), interpolation=cv2.INTER_AREA
                )
            image = cv2.cvtColor(image, cv2.COLOR_BGR2RGB)
            q = np.asarray(row["quads"], np.float32).reshape(-1, 4, 2)
            # Match OpenCV's pixel-center convention when shrinking originals.
            q = (q + 0.5) * [image.shape[1] / w, image.shape[0] / h] - 0.5
            q = q.astype(np.float32)
            for quad in q:
                if signed_area(quad) <= 0:
                    raise ValueError(f"Index contains a non-clockwise quad: {row['image']}")
            ig = np.asarray(row.get("ignore", []), np.float32).reshape(-1, 4, 2)
            ig = ((ig + 0.5) * [image.shape[1] / w, image.shape[0] / h] - 0.5).astype(np.float32)
            self.images.append(image)
            self.quads.append(q)
            self.ignores.append(ig)
            self.centers.append(q.mean(axis=1))
            if len(q):
                self.positive_ids.append(i)
        if not self.positive_ids:
            raise ValueError("No quads available for positive sampling")
        self.cache_bytes = sum(im.nbytes for im in self.images)

    def _geometry(self, i, negative):
        rng, s = self.rng, self.size
        h, w = self.images[i].shape[:2]
        angle = float(rng.uniform(0, 360))
        rotation = cv2.getRotationMatrix2D((s / 2, s / 2), angle, 1.0)
        if negative:
            # Negative crops are background-sized, with rejection AFTER rotation.
            # The positive 8–70% object-size constraint has no meaning here.
            inside = len(self.quads[i]) > 0 and rng.random() < self.inside_p
            for _ in range(128):
                if inside:
                    q = self.quads[i][int(rng.integers(len(self.quads[i])))]
                    short = float(np.linalg.norm(np.roll(q, -1, axis=0) - q, axis=1).min())
                    side = float(rng.uniform(0.25, 0.6) * short)
                    a, b = rng.uniform(0.15, 0.85, 2)
                    top = q[0] + a * (q[1] - q[0])
                    bottom = q[3] + a * (q[2] - q[3])
                    center = top + b * (bottom - top)
                else:
                    side = float(rng.uniform(0.08, 0.4) * min(w, h))
                    center = rng.uniform([0, 0], [w, h])
                crop = np.array(
                    [[s / side, 0, s / 2 - center[0] * s / side], [0, s / side, s / 2 - center[1] * s / side]]
                )
                m = rotation @ np.vstack([crop, [0, 0, 1]])
                centers = (
                    np.concatenate([self.centers[i], self.ignores[i].mean(axis=1).reshape(-1, 2)]) @ m[:, :2].T
                    + m[:, 2]
                )
                # One extra x pixel covers the x -> size-1-x flip convention.
                if not np.any(np.all((centers >= [-1, 0]) & (centers < [s, s]), axis=1)):
                    return crop, rotation
            # Guaranteed center-free crop beyond the frame, even for crowded data.
            side = 0.1 * min(w, h)
            crop = np.array(
                [[s / side, 0, s / 2 + (side + w) * s / side], [0, s / side, s / 2 + (side + h) * s / side]]
            )
            return crop, rotation
        target = self.quads[i][int(rng.integers(len(self.quads[i])))]
        longest = float(np.linalg.norm(np.roll(target, -1, axis=0) - target, axis=1).max())
        side = longest / float(rng.uniform(0.7, 1.6) if rng.random() < self.zoom_p else rng.uniform(0.08, 0.70))
        center = target.mean(axis=0) + rng.uniform(-0.12, 0.12, 2) * side
        return np.array(
            [[s / side, 0, s / 2 - center[0] * s / side], [0, s / side, s / 2 - center[1] * s / side]]
        ), rotation

    def _crop(self, image, crop):
        """Area prefilter shrinkage, then fractional crop into a black square.

        Never allocate a potentially huge padded source square. Pixel-center
        compensation keeps image coordinates aligned with the analytical affine.
        """
        s = self.size
        k = crop[0, 0]
        m = crop.copy()
        if k < 1:
            h, w = image.shape[:2]
            nw, nh = max(1, round(w * k)), max(1, round(h * k))
            image = cv2.resize(image, (nw, nh), interpolation=cv2.INTER_AREA)
            scale = np.array([nw / w, nh / h])
            m[:, 2] += m[:, :2] @ (0.5 / scale - 0.5)
            m[:, :2] /= scale
        return cv2.warpAffine(image, m, (s, s), flags=cv2.INTER_LINEAR, borderMode=cv2.BORDER_CONSTANT, borderValue=0)

    def batch(self, b: int) -> tuple[np.ndarray, list[list[dict]]]:
        """Return uint8 RGB [b,size,size,3] and clockwise float32 quad labels."""
        if not isinstance(b, int) or b < 0:
            raise ValueError("b must be a nonnegative integer")
        s = self.size
        images = np.empty((b, s, s, 3), np.uint8)
        labels = []
        for j in range(b):
            negative = self.rng.random() < 0.1
            i = (
                int(self.rng.integers(len(self.rows)))
                if negative
                else self.positive_ids[int(self.rng.integers(len(self.positive_ids)))]
            )
            crop, rotation = self._geometry(i, negative)
            canvas = self._crop(self.images[i], crop)
            canvas = cv2.warpAffine(
                canvas, rotation, (s, s), flags=cv2.INTER_LINEAR, borderMode=cv2.BORDER_CONSTANT, borderValue=0
            )
            matrix = rotation @ np.vstack([crop, [0, 0, 1]])
            crop_centers = self.centers[i] @ crop[:, :2].T + crop[:, 2]
            in_crop = np.all((crop_centers >= 0) & (crop_centers < s), axis=1)
            quads = self.quads[i] @ matrix[:, :2].T + matrix[:, 2]
            igq = self.ignores[i] @ matrix[:, :2].T + matrix[:, 2]
            if self.rng.random() < 0.5:
                canvas = canvas[:, ::-1]
                quads[:, :, 0] = s - 1 - quads[:, :, 0]
                quads = quads[:, ::-1]
                igq[:, :, 0] = s - 1 - igq[:, :, 0]
                igq = igq[:, ::-1]
            images[j] = canvas
            labs = []
            for q, present in zip(quads, in_crop):
                center = q.mean(axis=0)
                # A center discarded by the initial square cannot reappear as
                # a positive in a black triangle introduced by rotation.
                if present and np.all(center >= 0) and np.all(center < s):
                    q = np.ascontiguousarray(q, dtype=np.float32)
                    labs.append({"corners": q, "vis": 1.0, "area": signed_area(q)})
            for q in igq:  # unsure boxes: vis 0.2 = neither asked for nor punished (cornerdet.targets)
                c = q.mean(axis=0)
                if np.all(c >= 0) and np.all(c < s):
                    q = np.ascontiguousarray(q, dtype=np.float32)
                    labs.append({"corners": q, "vis": 0.2, "area": abs(signed_area(q))})
            if negative and any(l["vis"] > 0.3 for l in labs):
                raise AssertionError("Negative sampling admitted a quad center")
            labels.append(labs)
        return images, labels


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--index", default=str(paths.work() / "realdet_deck.jsonl"))
    parser.add_argument("--report-dir", type=Path, default=paths.work() / "realdet-report")
    args = parser.parse_args()
    start = time.perf_counter()
    sampler = RealDet(args.index)
    init_seconds = time.perf_counter() - start
    start = time.perf_counter()
    first, first_labels = sampler.batch(32)
    first_seconds = time.perf_counter() - start
    # Reset only the generator: sample sheet is reproducible without reloading.
    sampler.rng = np.random.default_rng(0)
    start = time.perf_counter()
    images, labels = sampler.batch(24)
    sample_seconds = time.perf_counter() - start
    tiles = []
    for i, (im, labs) in enumerate(zip(images, labels)):
        tile = cv2.copyMakeBorder(cv2.cvtColor(im, cv2.COLOR_RGB2BGR), 24, 0, 0, 0, cv2.BORDER_CONSTANT)
        for lab in labs:
            q = np.rint(lab["corners"] + [0, 24]).astype(np.int32)
            cv2.polylines(tile, [q], True, (50, 255, 50), 1, cv2.LINE_AA)
            for k, p in enumerate(q):
                cv2.circle(tile, tuple(p), 2, (0, 100, 255), -1)
                cv2.putText(tile, str(k), tuple(p + [3, -3]), 0, 0.3, (0, 255, 255), 1, cv2.LINE_AA)
        cv2.putText(tile, f"{i:02d} labels={len(labs)}", (5, 16), 0, 0.4, (240, 240, 240), 1)
        tiles.append(tile)
    args.report_dir.mkdir(parents=True, exist_ok=True)
    sheet = np.concatenate([np.concatenate(tiles[i : i + 6], 1) for i in range(0, 24, 6)])
    if not cv2.imwrite(str(args.report_dir / "batch_sample.jpg"), sheet):
        raise OSError("Cannot write sample sheet")
    timings, empty, total_labels = [], 0, 0
    for _ in range(20):
        start = time.perf_counter()
        batch, labs = sampler.batch(32)
        timings.append(time.perf_counter() - start)
        assert batch.shape == (32, 256, 256, 3) and batch.dtype == np.uint8
        for group in labs:
            empty += not group
            for lab in group:
                q = lab["corners"]
                assert q.dtype == np.float32 and q.shape == (4, 2)
                assert np.isfinite(q).all() and lab["vis"] == 1.0
                assert signed_area(q) > 0 and lab["area"] == signed_area(q)
                assert np.all(q.mean(0) >= 0) and np.all(q.mean(0) < 256)
                total_labels += 1
    stats = dict(
        init_seconds=init_seconds,
        cache_bytes=sampler.cache_bytes,
        first_batch32_seconds=first_seconds,
        sample_batch24_seconds=sample_seconds,
        batch32_seconds=timings,
        mean_seconds=float(np.mean(timings)),
        max_seconds=max(timings),
        tested_images=640,
        empty_labels=empty,
        tested_labels=total_labels,
        opencv_threads=cv2.getNumThreads(),
    )
    (args.report_dir / "batch_stats.json").write_text(json.dumps(stats, indent=2) + "\n")
    print(json.dumps(stats, indent=2), flush=True)
    assert max(timings + [first_seconds]) < 0.5, "Batch latency exceeds brief target"


if __name__ == "__main__":
    main()
