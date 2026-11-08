import argparse

import cv2
import numpy as np
import pytest
from PIL import Image

from sve_scan.card_photo import Report, process
from sve_scan.card_photo.__main__ import card_size
from sve_scan.card_photo.colour import FrameTemplate, correct

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
    card[1030:1060, 50:140] = (40, 170, 200)
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


def blank_border(out):
    h, w = out.shape[:2]
    return np.concatenate(
        [
            out[int(h * 0.1) : int(h * 0.8), int(w * x0) : int(w * x1)].reshape(-1, 3)
            for x0, x1 in [(0.015, 0.025), (0.975, 0.985)]
        ]
    )


# a card lying sideways (its top to the left), slightly rotated
SIDEWAYS = [(180, 1110), (190, 290), (1330, 300), (1320, 1120)]


def test_a_sideways_card_comes_out_straight_and_upright(tmp_path):
    card = drawn_card()
    photo = photo_of(card, SIDEWAYS)
    report = process(save(tmp_path, photo), str(tmp_path / "out"), "card")
    out, alpha = output(tmp_path)
    assert out.shape[0] / out.shape[1] == pytest.approx(88 / 63, abs=0.005)
    assert same_picture(card, out) > 0.95
    raw = Image.open(tmp_path / "out" / "card-raw.jpg")
    assert raw.height > raw.width
    assert np.abs(np.array(raw).astype(float) - np.rot90(photo[..., ::-1], k=3)).mean() < 3
    assert alpha[0, 0] == 0 and alpha[out.shape[0] // 2, out.shape[1] // 2] == 255
    assert report.problems() == []


def test_an_upside_down_card_is_turned_by_its_dark_name_bar(tmp_path):
    card = drawn_card()
    flipped = [SIDEWAYS[2], SIDEWAYS[3], SIDEWAYS[0], SIDEWAYS[1]]
    process(save(tmp_path, photo_of(card, flipped)), str(tmp_path / "out"), "card")
    out, _ = output(tmp_path)
    assert same_picture(card, out) > 0.95


def test_flip_turns_the_result_half_way_and_keeps_the_card_shape(tmp_path):
    card = drawn_card()
    process(save(tmp_path, photo_of(card, SIDEWAYS)), str(tmp_path / "out"), "card", flip=True)
    out, _ = output(tmp_path)
    assert out.shape[0] / out.shape[1] == pytest.approx(88 / 63, abs=0.005)
    assert same_picture(cv2.rotate(card, cv2.ROTATE_180), out) > 0.95


def test_a_card_turned_45_degrees_is_still_found(tmp_path):
    c = np.array([750, 650])
    angle = np.radians(45)
    rotation = np.array([[np.cos(angle), -np.sin(angle)], [np.sin(angle), np.cos(angle)]])
    corners = np.array([[-315, -440], [315, -440], [315, 440], [-315, 440]]) @ rotation.T + c
    card = drawn_card()
    process(save(tmp_path, photo_of(card, corners)), str(tmp_path / "out"), "card")
    out, _ = output(tmp_path)
    assert out.shape[0] / out.shape[1] == pytest.approx(88 / 63, abs=0.005)
    assert same_picture(card, out) > 0.95


def test_a_soft_shadow_is_reported_and_evened_out(tmp_path):
    yy, xx = np.mgrid[0:PHOTO_H, 0:PHOTO_W]
    shadow = 1 - 0.45 * np.clip((xx / PHOTO_W + yy / PHOTO_H) - 0.6, 0, 1)
    report = process(save(tmp_path, photo_of(drawn_card(), SIDEWAYS, shadow)), str(tmp_path / "out"), "card")
    assert report.uneven_light is not None and report.uneven_light > 0.15
    assert any("uneven light" in p for p in report.problems())
    out, _ = output(tmp_path)
    border = blank_border(out).mean(1)
    assert 1 - np.percentile(border, 5) / np.percentile(border, 95) < 0.06


def lab_of(out):
    return cv2.cvtColor(out, cv2.COLOR_BGR2LAB).astype(float)


def badge_chroma(out):
    h, w = out.shape[:2]
    badge = lab_of(out)[int(h * 0.902) : int(h * 0.92), int(w * 0.075) : int(w * 0.16)]
    return float(np.median(np.hypot(badge[..., 1] - 128, badge[..., 2] - 128)))


def test_the_ep_template_pulls_the_shared_frame_parts_to_official_values(tmp_path):
    photo = save(tmp_path, photo_of(drawn_card(), SIDEWAYS))
    process(photo, str(tmp_path / "plain"), "card")
    process(photo, str(tmp_path / "out"), "card", template="ep")
    out, _ = output(tmp_path)
    plain = cv2.cvtColor(np.array(Image.open(tmp_path / "plain" / "card.png"))[..., :3], cv2.COLOR_RGB2BGR)
    h, w = out.shape[:2]
    bar = lab_of(out)[int(h * 0.89) : int(h * 0.93), int(w * 0.4) : int(w * 0.8), 0]
    # Accepted EP images measure L=90–92 at the name bar and L=229–230 at the border.
    assert 87 <= np.percentile(bar, 40) <= 96
    border_lab = cv2.cvtColor(blank_border(out)[None, ...], cv2.COLOR_BGR2LAB)
    assert 225 <= np.median(border_lab[..., 0]) <= 234
    assert badge_chroma(out) > badge_chroma(plain)
    assert 70 <= badge_chroma(out) <= 85


def test_a_template_caps_badge_chroma_gain():
    card = drawn_card().astype(np.float32)
    regions = dict(name_bar=(0.4, 0.89, 0.8, 0.93), name_bar_l=100, badge_area=(0.04, 0.89, 0.2, 0.94))
    base = correct(card, FrameTemplate(**regions, badge_chroma=1))
    saturated = correct(card, FrameTemplate(**regions, badge_chroma=1000))
    before, after = badge_chroma(base.astype(np.uint8)), badge_chroma(saturated.astype(np.uint8))
    assert before < after <= 1.4 * before + 4


def test_a_template_for_another_frame_is_refused(tmp_path):
    card = (drawn_card() * 0.35 + 150).astype(np.uint8)  # a light frame: nothing dark where the name bar would be
    with pytest.raises(ValueError, match="frame"):
        process(save(tmp_path, photo_of(card, SIDEWAYS)), str(tmp_path / "out"), "card", template="ep")
    assert not (tmp_path / "out").exists()


def test_a_full_art_card_is_found_without_a_border_and_skips_border_steps(tmp_path):
    card = drawn_card(full_art=True)
    report = process(save(tmp_path, photo_of(card, SIDEWAYS)), str(tmp_path / "out"), "card", template="ep")
    assert report.full_art and report.uneven_light is None
    out, _ = output(tmp_path)
    assert same_picture(card, out) > 0.9
    reference = cv2.resize(card, out.shape[1::-1], interpolation=cv2.INTER_CUBIC)
    assert np.median(np.abs(out[40:-40, 40:-40].astype(float) - reference[40:-40, 40:-40]), axis=(0, 1)).max() < 3


def test_a_tilted_phone_and_a_small_card_are_reported(tmp_path):
    # the far side is 12% shorter: the phone leaned toward one end of the card
    tilted = [(500, 760), (520, 330), (1090, 360), (1070, 730)]
    report = process(save(tmp_path, photo_of(drawn_card(), tilted)), str(tmp_path / "out"), "card")
    problems = " ".join(report.problems())
    assert "tilted" in problems and "wide" in problems


def test_the_requested_size_and_rounded_corners(tmp_path):
    process(save(tmp_path, photo_of(drawn_card(), SIDEWAYS)), str(tmp_path / "out"), "card", size=(918, 1282))
    out, alpha = output(tmp_path)
    assert out.shape[:2] == (1282, 918)
    assert alpha[0, 0] == 0
    assert alpha[0, 459] == 242
    interior_alpha = alpha[1:-1, 1:-1]
    assert np.any((interior_alpha > 0) & (interior_alpha < 255))
    assert alpha[100, 100] == 255 and alpha[641, 459] == 255
    clear = alpha < 255
    assert np.ptp(out[clear].reshape(-1, 3), axis=0).max() < 40  # border colour under the corners, not table


def test_the_size_option_takes_card_shaped_sizes_only():
    assert card_size("918x1282") == (918, 1282)
    for bad in ("918", "918x918", "axb"):
        with pytest.raises(argparse.ArgumentTypeError, match="918|axb"):
            card_size(bad)


@pytest.mark.parametrize("width", [20, 30, 50, 100, 200, 300])
@pytest.mark.filterwarnings("error::RuntimeWarning")
def test_small_photographed_cards_return_a_retake_report(tmp_path, width):
    card = cv2.resize(drawn_card(), (width, round(width * 88 / 63)))
    photo = cv2.copyMakeBorder(card, 30, 30, 30, 30, cv2.BORDER_CONSTANT, value=(60, 115, 175))
    report = process(save(tmp_path, photo), str(tmp_path / "out"), "card")
    assert np.isfinite(report.highlights)
    assert any("wide" in problem for problem in report.problems())
    out, _ = output(tmp_path)
    assert out.mean() > 30


@pytest.mark.parametrize("size", [(1, 1), (20, 28)])
@pytest.mark.filterwarnings("error::RuntimeWarning")
def test_tiny_outputs_keep_the_border_colour_under_the_soft_edge(tmp_path, size):
    process(save(tmp_path, photo_of(drawn_card(), SIDEWAYS)), str(tmp_path / "out"), "card", size=size)
    out, alpha = output(tmp_path)
    assert out.shape[:2] == size[::-1]
    assert np.all(out[alpha < 255] > 180)


def test_clipped_highlights_are_reported(tmp_path):
    card = drawn_card(full_art=True)
    card[350:650, 250:550] = 255
    report = process(save(tmp_path, photo_of(card, SIDEWAYS)), str(tmp_path / "out"), "card")
    assert report.highlights > 0.03
    assert any("glare" in problem for problem in report.problems())


@pytest.mark.parametrize(
    ("field", "limit", "over", "problem"),
    [
        ("width", 800, 799, "wide"),
        ("tilt", 0.06, 0.061, "tilted"),
        ("uneven_light", 0.15, 0.151, "uneven light"),
        ("highlights", 0.03, 0.031, "glare"),
    ],
)
def test_the_documented_quality_limits(field, limit, over, problem):
    values = dict(width=800, tilt=0.0, highlights=0.0, uneven_light=0.0, full_art=False)
    values[field] = limit
    assert Report(**values).problems() == []
    values[field] = over
    assert any(problem in message for message in Report(**values).problems())
