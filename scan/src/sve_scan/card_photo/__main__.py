"""python -m sve_scan card-photo PHOTO --out DIR --name NAME [--template ep] [--size 918x1282] [--turn N]"""

import argparse

from sve_scan.card_photo import process
from sve_scan.card_photo.colour import TEMPLATES


def main() -> None:
    ap = argparse.ArgumentParser(
        prog="sve_scan card-photo", description="photo of one card -> official-style card image"
    )
    ap.add_argument("photo")
    ap.add_argument("--out", required=True, help="output folder")
    ap.add_argument("--name", required=True, help="output file name without extension")
    ap.add_argument("--template", choices=sorted(TEMPLATES), help="frame design whose shared parts to match")
    ap.add_argument("--size", help="output WxH, e.g. 918x1282 (twice the official 459x641)")
    ap.add_argument(
        "--turn", type=int, default=0, help="extra clockwise quarter turns if the card comes out upside down"
    )
    a = ap.parse_args()
    size = None
    if a.size:
        w, h = (int(v) for v in a.size.lower().split("x"))
        size = (w, h)
    report = process(a.photo, a.out, a.name, a.template, size, a.turn)
    light = "n/a (full art)" if report.uneven_light is None else f"{report.uneven_light:.0%}"
    print(
        f"{a.name}: card {report.width}px wide · uneven light {light} · tilt {report.tilt:.1%}"
        f" · glare {report.highlights:.1%}"
    )
    problems = report.problems()
    print("RETAKE: " + "; ".join(problems) if problems else "OK")


if __name__ == "__main__":
    main()
