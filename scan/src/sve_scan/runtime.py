"""The shipped recogniser and card index as the web client uses them: onnxruntime on CPU, no torch.

Evaluation and photo labelling go through here, so they measure what users get.
"""

from dataclasses import dataclass
from pathlib import Path

import cv2
import numpy as np
import onnxruntime as ort

from sve_scan import artifacts, paths

IN_H, IN_W = 312, 224


def session(path: Path, threads: int = 3) -> ort.InferenceSession:
    opts = ort.SessionOptions()
    opts.intra_op_num_threads = threads
    return ort.InferenceSession(str(path), opts, providers=["CPUExecutionProvider"])


def prep(arr: np.ndarray) -> np.ndarray:
    """uint8 RGB crops [N,H,W,3] -> float32 [N,3,312,224] in 0..1."""
    out = np.empty((len(arr), 3, IN_H, IN_W), np.float32)
    for i, im in enumerate(arr):
        out[i] = cv2.resize(np.ascontiguousarray(im), (IN_W, IN_H), interpolation=cv2.INTER_AREA).transpose(2, 0, 1)
    return out / 255.0


@dataclass
class Recogniser:
    sess: ort.InferenceSession

    def embed(self, arr: np.ndarray, bs: int = 32) -> np.ndarray:
        if len(arr) == 0:
            return np.zeros((0, artifacts.DIM), np.float32)
        return np.concatenate(
            [self.sess.run(None, {"image": prep(arr[i : i + bs])})[0] for i in range(0, len(arr), bs)]
        )

    def embed_both(self, arr: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
        """As seen and turned 180 degrees (an opponent's card faces the other way)."""
        return self.embed(arr), self.embed(np.ascontiguousarray(arr[:, ::-1, ::-1]))


@dataclass
class Gallery:
    """Index rows of one or more editions as float vectors, with their cards."""

    vectors: np.ndarray  # float32 [n, 256]
    hub: np.ndarray  # float32 [n]
    cards: list[artifacts.Json]

    def scores(
        self, e0: np.ndarray, e1: np.ndarray, hub_weight: float = artifacts.REC_SCORING["hubWeight"]
    ) -> np.ndarray:
        """[q, n] scores, the max over both orientations, hub-corrected."""
        return np.maximum(e0 @ self.vectors.T, e1 @ self.vectors.T) - hub_weight * self.hub[None, :]


def index_dir(tag: str) -> Path:
    return paths.work() / "index" / tag


def load(tag: str, editions: tuple[str, ...] = artifacts.EDITIONS) -> tuple[Recogniser, Gallery]:
    vecs, hubs, cards = [], [], []
    for ed in editions:
        idx = artifacts.read_index(index_dir(tag), ed)
        vecs.append(idx.vectors.astype(np.float32) * idx.scale[:, None])
        hubs.append(idx.hub)
        cards += [{**c, "r": ed} for c in idx.cards]
    rec = Recogniser(session(paths.work() / f"{tag}.w8.onnx"))
    return rec, Gallery(np.concatenate(vecs), np.concatenate(hubs), cards)


def card_level(order: np.ndarray, cards: list[artifacts.Json], k: int) -> list[str]:
    """First k distinct card ids along a ranked list of rows (printings of a card score alike)."""
    got: list[str] = []
    for gi in order:
        c = cards[gi]["c"]
        if c not in got:
            got.append(c)
            if len(got) == k:
                break
    return got
