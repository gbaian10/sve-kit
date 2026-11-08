"""Turn a phone photo of a physical card into a card image like the official ones.

For cards with no official image (event prizes, shop collaborations, unnumbered cards). The photo shows
one card lying flat on a plain table, taken from roughly straight above; see scan/README.md for the
shooting setup.

`process` writes two files and returns a quality report:

- `<name>-raw.jpg`: the photo turned so the card stands upright, kept as the source of record
- `<name>.png`: the card straightened to 63:88 with transparent rounded corners like the official
  images (the card image pipeline takes PNG sources)

What applies to which card:

- every card: finding and straightening, orientation, size, rounded corners, the tilt, size and
  highlight checks
- bordered cards only (`sve_scan.card_photo.colour`): white balance, shadow removal, tone range and
  the uneven-light check, all measured on the pale outer border
- one frame design (`--template`): matching parts that design shares, such as the EP name bar
"""

import os
from dataclasses import dataclass
from typing import Any

import cv2
import numpy as np
from PIL import Image, ImageOps

from sve_scan.card_photo.colour import TEMPLATES, border_mask, correct, has_pale_border, uneven_light
from sve_scan.card_photo.locate import find_card, photo_turns, stand_upright, warp

# The official corner: 3 mm on a 63 mm card, anti-aliased, with a slightly see-through outer ring.
CORNER_RADIUS = 3 / 63
RIM_ALPHA = 242

MAX_UNEVEN_LIGHT = 0.15  # border brightness spread before correction
MAX_TILT = 0.06  # opposite sides' length difference: the phone was not parallel to the card
MIN_WIDTH = 800  # card width in the photo
MAX_HIGHLIGHTS = 0.03  # share of clipped pixels on the card: glare from a lamp or a foil


@dataclass(frozen=True)
class Report:
    width: int  # card width in the photo, pixels
    tilt: float
    highlights: float
    uneven_light: float | None  # None for full-art cards: no border to measure
    full_art: bool

    def problems(self) -> list[str]:
        out = []
        if self.uneven_light is not None and self.uneven_light > MAX_UNEVEN_LIGHT:
            out.append(f"uneven light {self.uneven_light:.0%} (over {MAX_UNEVEN_LIGHT:.0%})")
        if self.tilt > MAX_TILT:
            out.append(f"phone tilted {self.tilt:.1%} (over {MAX_TILT:.0%})")
        if self.width < MIN_WIDTH:
            out.append(f"card only {self.width}px wide (under {MIN_WIDTH}): move closer or zoom 2x")
        if self.highlights > MAX_HIGHLIGHTS:
            out.append(f"glare on {self.highlights:.1%} of the card (over {MAX_HIGHLIGHTS:.0%})")
        return out


def rounded_alpha(h: int, w: int) -> np.ndarray:
    r = w * CORNER_RADIUS
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32) + 0.5
    cover = np.clip(r - np.hypot(xx - np.clip(xx, r, w - r), yy - np.clip(yy, r, h - r)) + 0.5, 0, 1)
    for edge in (cover[0, :], cover[-1, :], cover[:, 0], cover[:, -1]):
        edge *= RIM_ALPHA / 255
    return np.round(cover * 255).astype(np.uint8)


def _save(image: Image.Image, path: str, **options: Any) -> None:
    # write then rename, so an image viewer watching the folder never shows a half-written file
    image.save(path + ".tmp", **options)
    os.replace(path + ".tmp", path)


def process(
    photo: str,
    out_dir: str,
    name: str,
    template: str | None = None,
    size: tuple[int, int] | None = None,
    extra_turns: int = 0,
) -> Report:
    """Write `<name>-raw.jpg` and `<name>.png` to `out_dir`.

    `template` names a frame design in `colour.TEMPLATES`; `size` resizes the output (the card ratio is
    kept by the caller); `extra_turns` turns the card clockwise when the darker-end guess for the
    bottom is wrong.
    """
    img = cv2.cvtColor(np.array(ImageOps.exif_transpose(Image.open(photo)).convert("RGB")), cv2.COLOR_RGB2BGR)
    corners = stand_upright(img, find_card(img), extra_turns)

    os.makedirs(out_dir, exist_ok=True)
    rotation = [None, cv2.ROTATE_90_COUNTERCLOCKWISE, cv2.ROTATE_180, cv2.ROTATE_90_CLOCKWISE][photo_turns(corners)]
    upright = img if rotation is None else cv2.rotate(img, rotation)
    _save(
        Image.fromarray(cv2.cvtColor(upright, cv2.COLOR_BGR2RGB)),
        os.path.join(out_dir, f"{name}-raw.jpg"),
        format="JPEG",
        quality=97,
        subsampling=0,
    )

    width = int(round(max(np.linalg.norm(corners[1] - corners[0]), np.linalg.norm(corners[2] - corners[3]))))
    card = warp(img, corners, width).astype(np.float32)
    full_art = not has_pale_border(card)
    inner = card[int(card.shape[0] * 0.03) : -int(card.shape[0] * 0.03), int(width * 0.03) : -int(width * 0.03)]
    highlights = float((inner.max(2) >= 253).mean())
    light = None if full_art else uneven_light(card)
    if not full_art:
        card = correct(card, TEMPLATES[template] if template else None)

    if size:
        card = cv2.resize(card, size, interpolation=cv2.INTER_AREA if size[0] < width else cv2.INTER_CUBIC)
    h, w = card.shape[:2]
    alpha = rounded_alpha(h, w)
    # under the clear corners put the border colour, so later resizing does not bleed the table in
    card[alpha < 255] = np.median(card[border_mask(h, w)], axis=0)
    rgba = np.dstack([cv2.cvtColor(np.clip(card, 0, 255).astype(np.uint8), cv2.COLOR_BGR2RGB), alpha])
    _save(Image.fromarray(rgba, "RGBA"), os.path.join(out_dir, f"{name}.png"), format="PNG", optimize=True)

    sides = [float(np.linalg.norm(corners[(i + 1) % 4] - corners[i])) for i in range(4)]
    tilt = max(abs(sides[0] - sides[2]) / max(sides[0], sides[2]), abs(sides[1] - sides[3]) / max(sides[1], sides[3]))
    return Report(width=width, tilt=tilt, highlights=highlights, uneven_light=light, full_art=full_art)
