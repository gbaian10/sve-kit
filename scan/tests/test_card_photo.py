import cv2
import numpy as np
import pytest
from PIL import Image

from sve_scan.card_photo import process
from sve_scan.card_photo.colour import TEMPLATES, border_mask

CARD_W, CARD_H = 820, 1145  # 63:88
PHOTO_W, PHOTO_H = 1500, 1300


def drawn_card(full_art=False):
    """A made-up card: pale border, coloured art, dark name bar with a yellow badge (EP-like layout)."""
    rng = np.random.default_rng(1)
    card = np.zeros((CARD_H, CARD_W, 3), np.uint8)
    card[:] = (205, 215, 220)  # slightly warm-lit white border (BGR)
    inset = 0 if full_art else int(CARD_W * 0.047)
    art = card[inset : CARD_H - inset, inset : CARD_W - inset]
    for _ in range(60):
        x, y = rng.integers(0, art.shape[1]), rng.integers(0, art.shape[0])
        colour = tuple(int(c) for c in rng.integers(40, 230, 3))
        cv2.circle(art, (int(x), int(y)), int(rng.integers(20, 120)), colour, -1)
    if full_art:  # keep the full-art edge busy: no pale frame anywhere
        art[:6, :] = art[-6:, :] = art[:, :6] = art[:, -6:] = (150, 40, 160)
    bar = slice(int(CARD_H * 0.86), int(CARD_H * 0.95))
    card[bar, inset : CARD_W - inset] = (55, 55, 60)
    tpl = TEMPLATES["ep"]
    bx0, by0 = int(CARD_W * (tpl.badge_area[0] + 0.03)), int(CARD_H * (tpl.badge_area[1] + 0.02))
    card[by0 : by0 + 30, bx0 : bx0 + 90] = (40, 170, 200)  # yellow-ish badge, a little faded
    return card


def photo_of(card, corners, light=None):
    """The card laid on a wooden-looking table at the given corners (TL, TR, BR, BL of the card)."""
    rng = np.random.default_rng(2)
    table = np.zeros((PHOTO_H, PHOTO_W, 3), np.float32)
    table[:] = (60, 115, 175)
    table += (np.sin(np.arange(PHOTO_W) / 23.0) * 12)[None, :, None] + rng.normal(0, 4, table.shape)
    src = np.float32([[0, 0], [CARD_W - 1, 0], [CARD_W - 1, CARD_H - 1], [0, CARD_H - 1]])
    m = cv2.getPerspectiveTransform(src, np.float32(corners))
    placed = cv2.warpPerspective(card.astype(np.float32), m, (PHOTO_W, PHOTO_H), flags=cv2.INTER_CUBIC)
    mask = cv2.warpPerspective(np.ones((CARD_H, CARD_W), np.float32), m, (PHOTO_W, PHOTO_H))[..., None]
    out = table * (1 - mask) + placed * mask
    if light is not None:
        out *= light[..., None]
    return np.clip(out, 0, 255).astype(np.uint8)


def save(tmp_path, img, name="photo.png"):
    path = tmp_path / name
    cv2.imwrite(str(path), img)
    return str(path)


def output(tmp_path, name="card"):
    rgba = np.array(Image.open(tmp_path / "out" / f"{name}.png"))
    return cv2.cvtColor(rgba[..., :3], cv2.COLOR_RGB2BGR), rgba[..., 3]


def same_picture(a, b):
    """Correlation of the two cards' brightness patterns, ignoring colour correction."""
    ga = cv2.cvtColor(a, cv2.COLOR_BGR2GRAY).astype(float)
    gb = cv2.cvtColor(cv2.resize(b, a.shape[1::-1], interpolation=cv2.INTER_AREA), cv2.COLOR_BGR2GRAY).astype(float)
    inner = (slice(40, -40), slice(40, -40))
    return float(np.corrcoef(ga[inner].ravel(), gb[inner].ravel())[0, 1])


# a card lying sideways (its top to the left), slightly rotated
SIDEWAYS = [(180, 1110), (190, 290), (1330, 300), (1320, 1120)]


def test_a_sideways_card_comes_out_straight_and_upright(tmp_path):
    card = drawn_card()
    report = process(save(tmp_path, photo_of(card, SIDEWAYS)), str(tmp_path / "out"), "card")
    out, alpha = output(tmp_path)
    assert out.shape[0] / out.shape[1] == pytest.approx(88 / 63, abs=0.005)
    assert same_picture(card, out) > 0.95
    raw = Image.open(tmp_path / "out" / "card-raw.jpg")
    assert raw.height > raw.width  # the photo was turned with the card
    assert alpha[0, 0] == 0 and alpha[out.shape[0] // 2, out.shape[1] // 2] == 255
    assert report.problems() == []


def test_an_upside_down_card_is_turned_by_its_dark_name_bar(tmp_path):
    card = drawn_card()
    flipped = [SIDEWAYS[2], SIDEWAYS[3], SIDEWAYS[0], SIDEWAYS[1]]
    process(save(tmp_path, photo_of(card, flipped)), str(tmp_path / "out"), "card")
    out, _ = output(tmp_path)
    assert same_picture(card, out) > 0.95


def test_a_soft_shadow_is_reported_and_evened_out(tmp_path):
    yy, xx = np.mgrid[0:PHOTO_H, 0:PHOTO_W]
    shadow = 1 - 0.45 * np.clip((xx / PHOTO_W + yy / PHOTO_H) - 0.6, 0, 1)
    report = process(save(tmp_path, photo_of(drawn_card(), SIDEWAYS, shadow)), str(tmp_path / "out"), "card")
    assert report.uneven_light is not None and report.uneven_light > 0.15
    assert any("uneven light" in p for p in report.problems())
    out, _ = output(tmp_path)
    border = out.mean(2)[border_mask(*out.shape[:2])]
    assert 1 - np.percentile(border, 5) / np.percentile(border, 95) < 0.06


def test_the_ep_template_pulls_the_shared_frame_parts_to_official_values(tmp_path):
    process(save(tmp_path, photo_of(drawn_card(), SIDEWAYS)), str(tmp_path / "out"), "card", template="ep")
    out, _ = output(tmp_path)
    h, w = out.shape[:2]
    lab = cv2.cvtColor(out, cv2.COLOR_BGR2LAB).astype(float)
    tpl = TEMPLATES["ep"]
    x0, y0, x1, y1 = tpl.name_bar
    bar = lab[int(h * y0) : int(h * y1), int(w * x0) : int(w * x1), 0]
    assert np.percentile(bar, 40) == pytest.approx(tpl.name_bar_l, abs=4)
    assert np.median(lab[..., 0][border_mask(h, w)]) == pytest.approx(230, abs=4)


def test_a_full_art_card_is_found_without_a_border_and_skips_border_steps(tmp_path):
    card = drawn_card(full_art=True)
    report = process(save(tmp_path, photo_of(card, SIDEWAYS)), str(tmp_path / "out"), "card", template="ep")
    assert report.full_art and report.uneven_light is None
    out, _ = output(tmp_path)
    assert same_picture(card, out) > 0.9


def test_a_tilted_phone_and_a_small_card_are_reported(tmp_path):
    # the far side is 12% shorter: the phone leaned toward one end of the card
    tilted = [(500, 760), (520, 330), (1090, 360), (1070, 730)]
    report = process(save(tmp_path, photo_of(drawn_card(), tilted)), str(tmp_path / "out"), "card")
    problems = " ".join(report.problems())
    assert "tilted" in problems and "wide" in problems


def test_the_requested_size_and_official_corner_shape(tmp_path):
    process(save(tmp_path, photo_of(drawn_card(), SIDEWAYS)), str(tmp_path / "out"), "card", size=(918, 1282))
    out, alpha = output(tmp_path)
    assert out.shape[:2] == (1282, 918)
    radius = 918 * 3 / 63
    assert alpha[0, 0] == 0 and alpha[0, int(radius * 2)] == 242  # see-through outer ring like official images
    assert alpha[int(radius), int(radius)] == 255
    clear = alpha < 255
    assert np.ptp(out[clear].reshape(-1, 3), axis=0).max() < 40  # border colour under the corners, not table
