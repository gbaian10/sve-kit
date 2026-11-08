"""python -m sve_scan card-photo PHOTO --out DIR --name NAME [--template ep] [--size 918x1282] [--flip]"""

import argparse
import sys

from sve_scan.card_photo import process
from sve_scan.card_photo.colour import TEMPLATES
from sve_scan.card_photo.locate import CARD_RATIO


def card_size(text: str) -> tuple[int, int]:
    """WxH at the card's 63:88 ratio (within one pixel)."""
    try:
        w, h = (int(v) for v in text.lower().split("x"))
    except ValueError:
        raise argparse.ArgumentTypeError(f"expected WxH such as 918x1282, got {text!r}") from None
    if w <= 0 or abs(h - w * CARD_RATIO) > 1:
        raise argparse.ArgumentTypeError(f"{text} is not 63:88; for width {w} use {w}x{round(w * CARD_RATIO)}")
    return w, h


def main() -> None:
    ap = argparse.ArgumentParser(
        prog="sve_scan card-photo", description="photo of one card -> official-style card image"
    )
    ap.add_argument("photo")
    ap.add_argument("--out", required=True, help="output folder")
    ap.add_argument("--name", required=True, help="output file name without extension")
    ap.add_argument("--template", choices=sorted(TEMPLATES), help="frame design whose shared parts to match")
    ap.add_argument("--size", type=card_size, help="output WxH at 63:88, e.g. 918x1282 (twice the official 459x641)")
    ap.add_argument("--flip", action="store_true", help="turn the result 180 degrees if it comes out upside down")
    a = ap.parse_args()
    try:
        report = process(a.photo, a.out, a.name, a.template, a.size, a.flip)
    except ValueError as e:
        sys.exit(f"{a.name}: {e}")
    light = "n/a (full art)" if report.uneven_light is None else f"{report.uneven_light:.0%}"
    print(
        f"{a.name}: card {report.width}px wide · uneven light {light} · tilt {report.tilt:.1%}"
        f" · glare {report.highlights:.1%}"
    )
    problems = report.problems()
    print("RETAKE: " + "; ".join(problems) if problems else "OK")


if __name__ == "__main__":
    main()
