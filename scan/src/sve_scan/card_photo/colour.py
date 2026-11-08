"""Colour correction anchored on the card's pale outer border.

Every step here needs that border: it is the one part printed the same on every bordered card, so it
serves as the white reference and the light meter. Full-art cards have no such reference and skip
this module.

A frame template additionally pulls parts shared by one frame design (for EP cards: the name bar and
the Evolve badge) to values measured on official images, so a series of photos matches.
"""

from dataclasses import dataclass

import cv2
import numpy as np

# Lab lightness (0-255) of the border on official card images.
OFFICIAL_BORDER_L = 230.0


@dataclass(frozen=True)
class FrameTemplate:
    """Parts one frame design prints the same on every card, with their official values.

    Regions are fractions of the straightened card (x0, y0, x1, y1).
    """

    name_bar: tuple[float, float, float, float]
    name_bar_l: float  # 40th percentile of Lab L in the name bar
    badge_area: tuple[float, float, float, float]
    badge_chroma: float  # median Lab chroma of the yellow Evolve badge


# Measured on official EP card images (the 2024 Nagoya-A EP and the basic EP share this frame).
TEMPLATES = {
    "ep": FrameTemplate(
        name_bar=(0.30, 0.885, 0.92, 0.935),
        name_bar_l=92.0,
        badge_area=(0.03, 0.88, 0.30, 0.97),
        badge_chroma=82.0,
    ),
}

# Fallback without a template: the darkest 1% of the card goes to this lightness.
FALLBACK_BLACK_L = 20.0
MAX_CHROMA_GAIN = 1.4  # a faded photo is brightened at most this much, so print flaws are not exaggerated


def border_mask(h: int, w: int) -> np.ndarray:
    """A band just inside the card's outer edge, skipping the rounded corners."""
    inset, band, corner = int(w * 0.010), max(1, int(w * 0.016)), int(w * 0.06)
    m = np.zeros((h, w), bool)
    m[corner : h - corner, inset : inset + band] = m[corner : h - corner, w - inset - band : w - inset] = True
    m[inset : inset + band, corner : w - corner] = m[h - inset - band : h - inset, corner : w - corner] = True
    return m


def has_pale_border(card: np.ndarray) -> bool:
    """True when the card has a pale, plain outer border (most cards); False for full art."""
    hsv = cv2.cvtColor(np.clip(card, 0, 255).astype(np.uint8), cv2.COLOR_BGR2HSV)
    band = border_mask(*card.shape[:2])
    # bordered cards measure ~100% pale here; full-art edges, even with light patches, well under half
    pale = (hsv[..., 1][band] < 60) & (hsv[..., 2][band] > 120)
    return bool(pale.mean() > 0.8)


def uneven_light(card: np.ndarray) -> float:
    """How much darker the dimmest part of the border is than the brightest (0 = perfectly even)."""
    border = card.mean(2)[border_mask(*card.shape[:2])]
    return float(1 - np.percentile(border, 5) / np.percentile(border, 95))


def _region(h: int, w: int, box: tuple[float, float, float, float]) -> np.ndarray:
    m = np.zeros((h, w), bool)
    m[int(h * box[1]) : int(h * box[3]), int(w * box[0]) : int(w * box[2])] = True
    return m


def _even_out_light(img: np.ndarray, known: np.ndarray) -> np.ndarray:
    # the border is white everywhere, so its brightness is the light falling on the card; fill it
    # inward (Laplace) to estimate the light everywhere and divide it out
    s = 8
    # A narrow border can fall entirely between the downsampled pixels.
    while s > 1 and not known[::s, ::s][: img.shape[0] // s, : img.shape[1] // s].any():
        s //= 2
    h, w = img.shape[0] // s, img.shape[1] // s
    small = cv2.resize(img, (w, h), interpolation=cv2.INTER_AREA)
    known = known[::s, ::s][:h, :w]
    field = np.empty_like(small)
    for ch in range(3):
        v = small[..., ch].copy()
        v = np.where(known, np.maximum(v, cv2.medianBlur(np.clip(v, 0, 255).astype(np.uint8), 5)), v)
        f = np.where(known, v, v[known].mean())
        for _ in range(4000):
            f = np.where(known, v, cv2.blur(f, (3, 3)))
        field[..., ch] = cv2.GaussianBlur(f, (0, 0), 2)
    field = cv2.resize(field, (img.shape[1], img.shape[0]), interpolation=cv2.INTER_CUBIC)
    target = np.array([np.percentile(small[..., ch][known], 95) for ch in range(3)], np.float32)
    return np.clip(img * (target / np.clip(field, 1, None)), 0, 255)


def _lab(img: np.ndarray) -> np.ndarray:
    return cv2.cvtColor(np.clip(img, 0, 255).astype(np.uint8), cv2.COLOR_BGR2LAB).astype(np.float32)


def _bgr(lab: np.ndarray) -> np.ndarray:
    return cv2.cvtColor(np.clip(lab, 0, 255).astype(np.uint8), cv2.COLOR_LAB2BGR).astype(np.float32)


def correct(card: np.ndarray, template: FrameTemplate | None) -> np.ndarray:
    """White-balance on the border, even out soft shadows, then set the tone range.

    With a template the lightness curve runs through the border white and the name bar, and chroma is
    scaled so the Evolve badge matches (hue unchanged). Without one, the border goes to official white
    and the darkest 1% to a fixed black, which depends on the art and is less consistent.
    """
    h, w = card.shape[:2]
    known = border_mask(h, w)
    wb = card[known].mean(0)
    card = _even_out_light(card * (wb.max() / wb), known)
    lab = _lab(card)
    lightness = lab[..., 0]
    white = float(np.median(lightness[known]))
    if template is None:
        black = float(np.percentile(lightness, 1))
        lab[..., 0] = np.interp(lightness, [black, white], [FALLBACK_BLACK_L, OFFICIAL_BORDER_L])
        return _bgr(lab)
    dark = float(np.percentile(lightness[_region(h, w, template.name_bar)], 40))
    if dark > white - 60:
        raise ValueError("the name bar is not dark: this card does not have the template's frame")
    lab[..., 0] = np.interp(lightness, [0, dark, white, 255], [0, template.name_bar_l, OFFICIAL_BORDER_L, 255])
    hsv = cv2.cvtColor(np.clip(card, 0, 255).astype(np.uint8), cv2.COLOR_BGR2HSV)
    yellow = (hsv[..., 0] >= 18) & (hsv[..., 0] <= 35) & (hsv[..., 1] > 120) & (hsv[..., 2] > 120)
    badge = _region(h, w, template.badge_area) & yellow
    if badge.sum() > 0.0006 * h * w:
        chroma = float(np.median(np.hypot(lab[..., 1][badge] - 128, lab[..., 2][badge] - 128)))
        gain = float(np.clip(template.badge_chroma / max(chroma, 1), 1.0, MAX_CHROMA_GAIN))
        lab[..., 1:] = (lab[..., 1:] - 128) * gain + 128
    return _bgr(lab)
