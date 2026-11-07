# scan: card scanner models and card index

The two models and the card index behind phone card scanning (aiming at one card, or reading a laid-out deck
photo) are produced here: gallery, training, export, evaluation and release files. Inference itself runs in
`sim/web` (ONNX Runtime in the browser); the Python inference here only serves evaluation and labelling, and the
web client is the reference.

- **Corner finder** (det): 256×256 input, finds the four corners of every card in view
- **Recogniser** (rec): a straightened card → a 256-dimensional vector, searched in the card index
- **Card index**: vectors of every card image (int8) + hub correction values + the card list, one per card
  edition (jp / en)

The artifact format is specified in [docs/schema/scan/artifacts.md](../docs/schema/scan/artifacts.md).

`scan/` is its own uv project, outside the root workspace (PyTorch stays out of carddb's environment). It reads
exports only through `sve_carddb.export.read_api`, as `publish/` does.

## Setup

```bash
uv --directory scan sync                  # light: index, release, evaluation of exported models, tests
uv --directory scan sync --group train    # adds PyTorch (CUDA 13.0 wheels) for training and export
uv --directory scan run pytest            # what CI runs; needs no GPU and no private data
```

- An old system cuBLAS leaking in through `LD_LIBRARY_PATH` crashes torch convolutions; train under
  `env -u LD_LIBRARY_PATH`
- Cap CPU and memory for long jobs (for example `systemd-run --user --scope -p CPUQuota=300% -p MemoryMax=12G`)

## Where data lives

Every location comes from the environment; the repo holds no machine paths.

| Variable | Contents |
| --- | --- |
| `SVE_SCAN_EXPORT` | A public export root: `snapshots/` and `images/<size>/<int_id>.webp` (the gallery source) |
| `SVE_SCAN_WORK` | Generated files: gallery, banks, checkpoints, ONNX models, indexes, evaluation results, logs |
| `SVE_SCAN_DATA` | Private photos and material derived from them; never published, never in the repo |

Layout under `SVE_SCAN_DATA`:

| Path | Contents |
| --- | --- |
| `real/<folder>/manifest.jsonl` | Labelled real photos, one row per photo: `{file, region, card_no, [conditions], [exclude]}` |
| `real/phone-2026-10-18/`, `real/det-train/` | The maintainer's own aim-frame saves and side-seat bursts, evaluation only |
| `web-x/sample-events-orig/<event>/` | Deck photos from players' social media posts; `auto-labels/` holds classes and labels |
| `web-det/pokemon-card-segmentation/` | An open CC0 card segmentation set, real training data for the corner finder |

Data sources are described only as "second-hand marketplace listing photos" and "players' social media posts".
Texture banks and real photos are not distributed; only the code that builds the banks is published.

### Held-out evaluation

The six events listed in `sve_scan/splits.py` (posts within 8 days from the start) are the evaluation set: they
never feed training, corner labels or any texture bank. Photo folders used as texture sources never count as
evaluation. Run `labelcheck` on every new photo set first (hand labels run about 3% wrong).

## Pipeline

Every step is `uv --directory scan run python -m sve_scan <command>` (no argument lists the commands). The tags
below are only file names in the work folder; the published versions (`rec-1`, `det-1`, …) are given at release.

### 1. Gallery

```bash
python -m sve_scan gallery          # gallery.json, gallery.npy, synthetic queries, gallery-meta.json
python -m sve_scan fetch-bg         # CC0 table textures
python -m sve_scan fetch-rooms      # CC0 indoor panorama crops
```

- The gallery is every approved, available printing face image of the export's snapshot (both editions); alt-art
  groups come from the snapshot's `art_id`
- `gallery-meta.json` records the snapshot's `data_version` and the identity of the image array; `gallery.npy` is
  rebuilt whenever a card, an image version or the order changes, and later steps refuse a mismatched array
- The asset downloads keep at least 2 seconds between requests and record each file's URL, fetch time, ETag and
  SHA-256 in the folder's `manifest.json`; a rerun keeps the earlier records

### 2. Texture banks (private data)

```bash
python -m sve_scan deck-label <rec tag> <det tag> labels.json <SVE_SCAN_DATA>/web-x/sample-events-orig/auto-labels/cards
python -m sve_scan texbank texbank.npz <folder> ...     # foil textures (second-hand listing photos)
python -m sve_scan sleeve-bank <out dir>                # sleeve glare layers (deck photos of non-evaluation events)
python -m sve_scan banks texbank.npz <out dir>/sleeve_material.npz realbank.npz procbank.npz
```

### 3. Recogniser

The current model was trained in two stages:

```bash
# stage 1: from ImageNet weights, 40 epochs, foil textures only
SIBLINGS=art FRAME_P=0.6 FRAME_SMIN=0.4 HOLO_P=0.2 DEFOCUS_P=0.3 FOIL_P=0.3 REAL_P=0.4 TEXBANK=texbank.npz \
  python -m sve_scan rec-train mobilenetv3_large_100.ra_in1k 40 rec-base

# stage 2: continue stage 1 at 0.25x learning rate for 12 epochs; foil textures keep their share, sleeve glare
# and procedural layers come on top
INIT=rec-base LR_SCALE=0.25 SIBLINGS=art FRAME_P=0.6 FRAME_SMIN=0.4 HOLO_P=0.2 DEFOCUS_P=0.3 FOIL_P=0.3 \
  REAL_P=0.4 TEXBANK=texbank.npz REAL2_P=0.3 REALBANK=realbank.npz PROCBANK=procbank.npz WR_P=0.2 HS_P=0.2 MISC_P=0.1 \
  python -m sve_scan rec-train mobilenetv3_large_100.ra_in1k 12 rec-ft

python -m sve_scan rec-export mobilenetv3_large_100.ra_in1k rec-ft   # rec-ft.w8.onnx (weights-only int8)
python -m sve_scan rec-hub mobilenetv3_large_100.ra_in1k rec-ft      # hub correction values
python -m sve_scan index rec-ft                                       # index/rec-ft/{jp,en}.* and build.json
```

- Training once from scratch with every bank mixed in regresses (the foil share gets diluted). A new version must
  not be worse than the previous one on any evaluation set
- Static int8 (activations quantised too) dropped real-photo top-1 from 91% to 69%, so only weights are quantised
- Recompute the hub values and the index whenever the recogniser or the gallery changes; a stale hub file lets a
  few cards win every uncertain photo. The index step refuses hub values made for another gallery
- A new set: rerun `gallery`, `rec-hub` and `index`; the models need no retraining (about 8% of cards are held out
  of training to keep checking that)

### 4. Corner finder

The current model was trained in four stages, each continuing the previous one:

```bash
python -m sve_scan det train det-a 15000 48                                    # synthetic scenes only
python -m sve_scan det-real-build                                              # realdet_pokemon.jsonl
INIT=det-a REAL=realdet_pokemon.jsonl REAL_FRAC=0.3 LR=1e-3 python -m sve_scan det train det-b 4000 48
INIT=det-b REAL=realdet_pokemon.jsonl REAL_FRAC=0.3 LR=1e-3 REAL_P=0.3 TEXBANK=texbank.npz \
  REAL2_P=0.5 REALBANK=realbank.npz PROCBANK=procbank.npz WR_P=0.25 HS_P=0.35 MISC_P=0.15 \
  python -m sve_scan det train det-c 4000 48
DETS=det-c:1.04,det-b python -m sve_scan det-label <rec tag> photos.txt det-label.jsonl   # non-evaluation events
python -m sve_scan det-real-index realdet_deck.jsonl realdet_pokemon.jsonl det-label.jsonl
INIT=det-c REAL=realdet_deck.jsonl REAL_FRAC=0.4 LR=5e-4 REAL_P=0.3 TEXBANK=texbank.npz \
  REAL2_P=0.5 REALBANK=realbank.npz PROCBANK=procbank.npz WR_P=0.25 HS_P=0.35 MISC_P=0.15 \
  python -m sve_scan det train det-d 4000 48
python -m sve_scan det export det-d det_det-d.w8.onnx
```

- Stage 3 boxes run about 4% inside the card edge, so labelling grows them by 1.04 (the `:1.04` in `DETS`); stage
  4 learned the exact SIFT-projected corners and is not grown (the published `grow` is 1.0)
- The current deck-photo labels came from the union of three corner finders, one of them a version that was tried
  and not adopted; any detector set can make labels
- With `REC_TAG=<rec tag>` training ends with end-to-end checks on the maintainer's phone saves and bursts
- The best offline corner error is not the best model on the web page; judge a new model by the web deck reader
- The model's four corners are clockwise from an arbitrary start; every client orders them (artifact format)

### 5. Evaluation

```bash
python -m sve_scan eval-rec rec-ft <evaluation folder> ...          # single card: card-level top-1 / top-5
python -m sve_scan deck-label rec-ft det-d labels.json deck-out     # then eval-deck for deck recall
python -m sve_scan eval-deck deck-out
python -m sve_scan eval-det rec-ft det.json det-d:global            # corner finder: useful boxes, corner error
```

- Results go to `SVE_SCAN_WORK/eval/`; the same inputs and models give the same results
- Deck recall counts SIFT-confirmed cards, which lean towards easy cards, so it is optimistic
- The web deck reader's end-to-end result is the reference; the Python versions are quick checks during training

### 6. Release files

```bash
python -m sve_scan release <out dir> 1 det-d 1 rec-ft
```

Writes `models/<sha256>.onnx`, `index/<sha256>.*` and `manifest.json` for the publish step, which uploads them under
`scan/` on R2 (uploads need the maintainer's approval). The index's card data version comes from its own
`build.json`, and the release refuses an index built with another recogniser.
