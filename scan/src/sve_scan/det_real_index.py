"""Real-photo training index for the corner finder: the Pokémon train rows (det-real-build) plus the deck
photos labelled by det-label with at least 3 confirmed cards (held-out evaluation events are never labelled).

usage: python -m sve_scan det-real-index <out.jsonl> <pokemon.jsonl> <det-label.jsonl> [...]
"""

import json
import sys


def main() -> None:
    out, poke, *xfiles = sys.argv[1:]
    rows = [r for r in map(json.loads, open(poke)) if r["split"] == "train"]
    x = []
    for f in xfiles:
        for r in map(json.loads, open(f)):
            if len(r["quads"]) >= 3:
                x.append({k: r[k] for k in ("image", "w", "h", "dataset", "split", "max_side", "quads", "ignore")})
    with open(out, "w") as fh:
        for r in rows + x:
            fh.write(json.dumps(r) + "\n")
    print(
        "pokemon",
        len(rows),
        "x images",
        len(x),
        "x cards",
        sum(len(r["quads"]) for r in x),
        "ignore",
        sum(len(r["ignore"]) for r in x),
    )


if __name__ == "__main__":
    main()
