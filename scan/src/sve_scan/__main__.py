"""python -m sve_scan <command> [args]: one entry point for every step (scan/README.md has the pipeline).

Each command runs its module as a script with the remaining arguments, so heavy modules (torch) load only
for the command that needs them.
"""

import runpy
import sys

COMMANDS = {
    # data
    "gallery": ("gallery", "training gallery + synthetic queries from SVE_SCAN_EXPORT"),
    "fetch-bg": ("fetch_bg", "CC0 table textures for synthetic scenes"),
    "fetch-rooms": ("fetch_rooms", "CC0 room views for synthetic scenes"),
    "labelcheck": ("labelcheck", "flag likely label errors in a photo folder"),
    "deck-label": ("deck_label", "SIFT-confirmed card labels for deck photos"),
    "texbank": ("texbank", "foil texture layers from labelled listing photos"),
    "sleeve-bank": ("sleeve_bank", "sleeve glare layers from labelled deck photos"),
    "banks": ("banks", "merge real layers and render procedural ones"),
    "det-real-build": ("det_real_build", "real-photo quad index (open card set)"),
    "det-label": ("det_label", "corner labels for deck photos"),
    "det-real-index": ("det_real_index", "merge real-photo indexes for corner-finder training"),
    # recogniser
    "rec-train": ("rec_train", "train the recogniser"),
    "rec-export": ("rec_export", "phone model (weights-only int8 ONNX)"),
    "rec-hub": ("rec_hub", "hub correction values"),
    "index": ("rec_index", "card index per edition"),
    # corner finder
    "det": ("det", "corner finder: sheet | train | eval | realval | export"),
    "tiledet": ("tiledet", "draw the tiled deck detection on one photo"),
    # evaluation and release
    "eval-rec": ("eval_rec", "recogniser on labelled photo folders and synthetic sets"),
    "eval-det": ("eval_det", "corner finder on held-out deck photos"),
    "eval-deck": ("eval_deck", "deck recall from deck-label output"),
    "release": ("release", "hashed release folder + scan/manifest.json"),
}


def main() -> None:
    if len(sys.argv) < 2 or sys.argv[1] not in COMMANDS:
        width = max(map(len, COMMANDS))
        print("usage: python -m sve_scan <command> [args]\n")
        for name, (_, help_) in COMMANDS.items():
            print(f"  {name:{width}}  {help_}")
        raise SystemExit(0 if len(sys.argv) < 2 else 2)
    module, _ = COMMANDS[sys.argv[1]]
    sys.argv = [f"sve_scan {sys.argv[1]}", *sys.argv[2:]]
    runpy.run_module(f"sve_scan.{module}", run_name="__main__", alter_sys=True)


if __name__ == "__main__":
    main()
