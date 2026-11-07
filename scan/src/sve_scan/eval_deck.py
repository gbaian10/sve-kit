"""Deck-photo scores from deck-label output, split into held-out evaluation events and the rest.

Per photo: boxes found, SIFT-confirmed ('sure') cards, and how many of those the recogniser named right
(card-level top-1, and within top 5). 'Deck recall' = sure cards named right / sure cards. The sure set leans
towards easy cards, so this is optimistic; the end-to-end reference stays the web client's deck reader.

usage: python -m sve_scan eval-deck <deck-label out dir> [<out.json>]
"""

import json
import sys
from pathlib import Path

from sve_scan.splits import is_eval


def main() -> None:
    folder = Path(sys.argv[1])
    sums = {split: {"photos": 0, "boxes": 0, "sure": 0, "top1": 0, "top5": 0} for split in ("eval", "other")}
    for f in sorted(folder.glob("*.json")):
        d = json.loads(f.read_text())
        s = sums["eval" if is_eval(d["label"]["event"]) else "other"]
        s["photos"] += 1
        s["boxes"] += len(d["cards"])
        for c in d["cards"]:
            if c["sure"]:
                s["sure"] += 1
                s["top1"] += int(c["sift_rank"] == 1)
                s["top5"] += int(c["sift_rank"] is not None and c["sift_rank"] <= 5)
    for s in sums.values():
        s["recall_top1"] = round(s["top1"] / max(s["sure"], 1), 4)
        s["recall_top5"] = round(s["top5"] / max(s["sure"], 1), 4)
    print(json.dumps(sums, indent=1))
    if len(sys.argv) > 2:
        Path(sys.argv[2]).write_text(json.dumps(sums, indent=1))


if __name__ == "__main__":
    main()
