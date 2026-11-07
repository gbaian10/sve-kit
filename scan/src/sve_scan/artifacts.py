"""The published scanner artifacts (docs/schema/scan/artifacts.md): the int8 card index and scan/manifest.json.

Kept free of torch and onnxruntime so CI can test it with the light dependencies only.
"""

import hashlib
import json
import shutil
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import numpy as np

Json = dict[str, Any]

FORMAT = 1
DIM = 256
EDITIONS = ("jp", "en")
CARD_FIELDS = ("no", "c", "n", "f", "nm", "a", "k", "y")
# Calibration of the current recogniser and corner finder; they travel with the models in the manifest.
REC_SCORING = {"hubWeight": 0.75, "accept": 0.5, "margin": 0.03}
DET_PARAMS = {"input": 256, "stride": 8, "grow": 1.0}
REC_PARAMS = {"input": [312, 224], "dim": DIM}
SUFFIX = {"vectors": ".i8", "scale": ".scale", "hub": ".hub", "cards": ".json"}


@dataclass(frozen=True)
class Index:
    """One edition's searchable cards: row i of every array belongs to cards[i]."""

    vectors: np.ndarray  # int8 [n, DIM]
    scale: np.ndarray  # float32 [n]
    hub: np.ndarray  # float32 [n]
    cards: list[Json]

    def __post_init__(self) -> None:
        n = len(self.cards)
        if self.vectors.shape != (n, DIM) or self.vectors.dtype != np.int8:
            raise ValueError(f"vectors must be int8 [{n}, {DIM}], got {self.vectors.dtype} {self.vectors.shape}")
        for name in ("scale", "hub"):
            arr = getattr(self, name)
            if arr.shape != (n,) or arr.dtype != np.float32:
                raise ValueError(f"{name} must be float32 [{n}], got {arr.dtype} {arr.shape}")
        for card in self.cards:
            if tuple(card) != CARD_FIELDS:
                raise ValueError(f"card fields must be {CARD_FIELDS}, got {tuple(card)}")


def quantize(embeddings: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """Unit rows -> int8 rows and a per-row scale, so row * scale ~= the unit vector."""
    unit = embeddings / np.linalg.norm(embeddings, axis=1, keepdims=True)
    scale = (np.abs(unit).max(axis=1) / 127).astype(np.float32)
    vectors = np.clip(np.rint(unit / scale[:, None]), -127, 127).astype(np.int8)
    return vectors, scale


def scores(index: Index, query: np.ndarray, hub_weight: float = REC_SCORING["hubWeight"]) -> np.ndarray:
    """Search score of every row for one unit query vector, the same sum the web client computes."""
    return (index.vectors.astype(np.float32) @ query) * index.scale - hub_weight * index.hub


def decode_quad(corners: np.ndarray, gx: int, gy: int, stride: int = DET_PARAMS["stride"]) -> np.ndarray:
    """The four corners predicted at heat cell (gx, gy) in input pixels: offsets are in cells from the cell
    centre. corners is the model output for one image, [8, G, G]. The order is clockwise from any corner."""
    return (corners[:, gy, gx].reshape(4, 2) + [gx + 0.5, gy + 0.5]) * stride


def order_corners(quad: np.ndarray) -> np.ndarray:
    """Clockwise on screen (y grows downwards), starting at the corner with the smallest x + y (top left).

    The corner finder is trained without a fixed start corner, so every client orders its quads like this
    before straightening a card.
    """
    c = quad.mean(0)
    q = quad[np.argsort(np.arctan2(quad[:, 1] - c[1], quad[:, 0] - c[0]))]
    return np.roll(q, -int(np.argmin(q.sum(1))), 0)


def _canonical(value: object) -> bytes:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode()


def write_index(folder: Path, name: str, index: Index) -> dict[str, Path]:
    folder.mkdir(parents=True, exist_ok=True)
    out = {part: folder / f"{name}{suffix}" for part, suffix in SUFFIX.items()}
    out["vectors"].write_bytes(index.vectors.tobytes())
    out["scale"].write_bytes(index.scale.astype("<f4").tobytes())
    out["hub"].write_bytes(index.hub.astype("<f4").tobytes())
    out["cards"].write_bytes(_canonical(index.cards))
    return out


def read_index(folder: Path, name: str) -> Index:
    cards = json.loads((folder / f"{name}.json").read_text())
    n = len(cards)
    vectors = np.frombuffer((folder / f"{name}.i8").read_bytes(), np.int8)
    if vectors.size != n * DIM:
        raise ValueError(f"{name}.i8 holds {vectors.size} values, expected {n} x {DIM}")
    return Index(
        vectors.reshape(n, DIM),
        np.frombuffer((folder / f"{name}.scale").read_bytes(), "<f4").astype(np.float32),
        np.frombuffer((folder / f"{name}.hub").read_bytes(), "<f4").astype(np.float32),
        cards,
    )


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def _publish(src: Path, release: Path, folder: str, suffix: str) -> Json:
    """Copy a file under its content hash (immutable, cacheable forever) and describe it."""
    digest = sha256(src)
    rel = f"{folder}/{digest}{suffix}"
    dst = release / rel
    dst.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(src, dst)
    return {"file": rel, "bytes": src.stat().st_size, "sha256": digest}


def release(
    out: Path,
    det: tuple[int, Path],
    rec: tuple[int, Path],
    indexes: dict[str, tuple[Path, str, str]],
) -> Json:
    """Lay out a release folder (models/, index/, manifest.json) ready to upload under scan/.

    det, rec: (version, onnx path). indexes: edition -> (folder, file stem, card data_version).
    """
    if set(indexes) - set(EDITIONS):
        raise ValueError(f"unknown editions {sorted(set(indexes) - set(EDITIONS))}")
    manifest: Json = {
        "format": FORMAT,
        "det": {"version": det[0], **_publish(det[1], out, "models", ".onnx"), **DET_PARAMS},
        "rec": {"version": rec[0], **_publish(rec[1], out, "models", ".onnx"), **REC_PARAMS, "scoring": REC_SCORING},
        "index": {},
    }
    for edition, (folder, stem, data_version) in sorted(indexes.items()):
        index = read_index(folder, stem)  # validates shapes before anything is published
        entry: Json = {"rec": rec[0], "data_version": data_version, "count": len(index.cards)}
        for part, suffix in SUFFIX.items():
            entry[part] = _publish(folder / f"{stem}{suffix}", out, "index", suffix)
        manifest["index"][edition] = entry
    (out / "manifest.json").write_bytes(json.dumps(manifest, ensure_ascii=False, indent=2).encode() + b"\n")
    return manifest
