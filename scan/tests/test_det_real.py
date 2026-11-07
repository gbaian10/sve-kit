import json

import cv2
import numpy as np

from sve_scan.det_real import RealDet

CARDS = [(40, 40), (220, 40), (400, 40)]  # top-left corners of three 120x168 white cards on a dark 600x300 photo
W, H = 120, 168
S = 256


def make_index(tmp_path):
    photo = np.full((300, 600, 3), 30, np.uint8)
    for x, y in CARDS:
        photo[y : y + H, x : x + W] = 255
    cv2.imwrite(str(tmp_path / "deck.png"), photo)
    quads = [[[x, y], [x + W, y], [x + W, y + H], [x, y + H]] for x, y in CARDS]
    row = {"image": str(tmp_path / "deck.png"), "w": 600, "h": 300, "split": "train", "quads": quads}
    (tmp_path / "index.jsonl").write_text(json.dumps(row) + "\n")
    return str(tmp_path / "index.jsonl")


def long_side(q):
    return float(np.linalg.norm(np.roll(q, -1, axis=0) - q, axis=1).max())


def sample(tmp_path, n=400, **shares):
    images, labels = RealDet(make_index(tmp_path), size=S, seed=3, **shares).batch(n)
    return images, labels


def test_close_ups_make_a_card_fill_the_view(tmp_path):
    _, labels = sample(tmp_path, zoom_p=1.0)
    biggest = [max(long_side(l["corners"]) for l in labs) for labs in labels if labs]
    assert biggest and min(biggest) >= 0.7 * S - 1


def test_by_default_cards_stay_within_the_earlier_sizes(tmp_path):
    _, labels = sample(tmp_path)
    assert max(long_side(l["corners"]) for labs in labels for l in labs) <= 0.7 * S + 1


def test_inside_card_negatives_show_card_art_and_no_card(tmp_path):
    def negatives_on_a_card(**shares):
        images, labels = sample(tmp_path, **shares)
        empty = [im for im, labs in zip(images, labels) if not labs]  # negatives: no card centre in view
        assert empty
        return np.mean([im.mean() > 200 for im in empty])  # mostly white = cut from inside a card

    assert negatives_on_a_card(inside_p=1.0) > 0.6
    assert negatives_on_a_card() < 0.3
