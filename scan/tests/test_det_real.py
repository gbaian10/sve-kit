import json

import cv2
import numpy as np

from sve_scan.det_real import RealDet

CARDS = [(40, 40), (220, 40), (400, 40)]  # top-left corners of three 120x168 cards on a 600x300 photo
W, H = 120, 168


def make_index(tmp_path):
    photo = tmp_path / "deck.jpg"
    cv2.imwrite(str(photo), np.full((300, 600, 3), 90, np.uint8))
    quads = [[[x, y], [x + W, y], [x + W, y + H], [x, y + H]] for x, y in CARDS]
    index = tmp_path / "index.jsonl"
    row = {"image": str(photo), "w": 600, "h": 300, "split": "train", "quads": quads}
    index.write_text(json.dumps(row) + "\n")
    return str(index)


def test_inside_card_negatives_never_hold_a_card_centre(tmp_path):
    sampler = RealDet(make_index(tmp_path), size=256, seed=3, inside_p=1.0)
    for _ in range(200):
        crop, rotation = sampler._geometry(0, negative=True)
        m = rotation @ np.vstack([crop, [0, 0, 1]])
        centres = sampler.centers[0] @ m[:, :2].T + m[:, 2]
        assert not np.any(np.all((centres >= [-1, 0]) & (centres < [256, 256]), axis=1))
        # the crop is cut from inside a card: its side is at most 60% of the card's short side
        assert 256 / crop[0, 0] <= 0.6 * W + 1e-6


def test_close_ups_make_the_target_card_fill_the_view(tmp_path):
    sampler = RealDet(make_index(tmp_path), size=256, seed=3, zoom_p=1.0)
    for _ in range(200):
        crop, _rotation = sampler._geometry(0, negative=False)
        assert H * crop[0, 0] >= 0.7 * 256 - 1e-6  # the card's long side in view pixels


def test_defaults_keep_the_earlier_sizes(tmp_path):
    sampler = RealDet(make_index(tmp_path), size=256, seed=3)
    for _ in range(200):
        crop, _rotation = sampler._geometry(0, negative=False)
        assert H * crop[0, 0] <= 0.7 * 256 + 1e-6
