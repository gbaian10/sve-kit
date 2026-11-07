# Card scanner models: model card

What the published card scanner models are, what they were built from, under which terms, and where they fail.
How to rebuild them is in [README.md](README.md); the published file format is in
[docs/schema/scan/artifacts.md](../docs/schema/scan/artifacts.md).

| Published | What it does | Size (weights-only int8 ONNX) |
| --- | --- | --- |
| `det-1` (corner finder) | Finds the four corners of every card in a 256×256 view | 1.1 MB |
| `rec-1` (recogniser) | Turns a straightened card into a 256-dimensional vector | 4.7 MB |
| Card index (per card edition, per card data version) | One vector per printing face, searched with the `rec-1` vector | jp ≈ 3.4 MB, en ≈ 3.2 MB (≈ 2.1 MB each compressed) |

## Licensing and provenance

- **Code**: everything under `scan/` is Apache-2.0 ([LICENSING.md](../LICENSING.md))
- **Weights and card index**: not licensed by the project. They are computed from official card images and, like
  snapshots and card images, do not acquire the license of the program that made them (see LICENSING.md). They
  are published for the web app only, the same way as the card images
- **Base weights**: both models start from timm ImageNet-1k weights released under Apache-2.0. The published
  weights are derived from them; this notice keeps that attribution

| Model | Base weights | License | Pretraining data |
| --- | --- | --- | --- |
| `rec-1` | [`timm/mobilenetv3_large_100.ra_in1k`](https://huggingface.co/timm/mobilenetv3_large_100.ra_in1k) | Apache-2.0 | ImageNet-1k |
| `det-1` | [`timm/mobilenetv3_small_100.lamb_in1k`](https://huggingface.co/timm/mobilenetv3_small_100.lamb_in1k) | Apache-2.0 | ImageNet-1k |

### Training data

| Data | Used for | Terms | Distributed |
| --- | --- | --- | --- |
| Official card images (`card_l` of a public export) | Both models (synthetic scenes and queries), the card index | Official content, outside the project's licenses | As card images, by the web app |
| [Poly Haven](https://polyhaven.com/) table textures and indoor panoramas | Corner finder backgrounds | CC0 | No (fetched by `fetch-bg`, `fetch-rooms`) |
| [`antokun/pokemon-card-segmentation`](https://huggingface.co/datasets/antokun/pokemon-card-segmentation) | Corner finder, real photos of cards | CC0-1.0 | No |
| Second-hand marketplace listing photos | Foil texture layers (recogniser and corner finder augmentation) | Not ours; private | No; only the code that builds the layers |
| Players' social media posts (deck photos) | Sleeve glare layers; corner labels for the corner finder | Not ours; private | No; only the code |
| The maintainer's own phone photos | Evaluation only | — | No |

Deck photos from the held-out events in `sve_scan/splits.py` never feed training, labels or texture layers.

## Intended use

- Identifying Shadowverse: EVOLVE cards in photos in the `sim/web` scanner: one card aimed at by the camera, or
  every card of a laid-out deck photo
- The answer is a candidate list for the user to confirm; it is not a ruling on which card a physical card is

Out of scope: other card games, card grading or condition, telling printings apart that share the same official
image.

## Evaluation

Held-out data only (never used in training). Card level: different printings of the same card count as the
same answer.

| Set | Metric | Result |
| --- | --- | --- |
| Marketplace listing photos, two sets | Recogniser top-1 (centre crop) | .951, 1.0 |
| Video frames | Recogniser top-1 (centre crop) | .846 |
| Foil cards | Recogniser top-1 (centre crop) | .910 |
| Deck photos of 6 held-out events (35 photos, 889 SIFT-confirmed cards) | Cards named right after both models | 854 (.961) |
| Synthetic deck photo, 12 large cards in 2 rows | Cards found | 12/12, no false boxes |
| Maintainer's single-card phone saves (63) | Card named right | .857 |
| Side-seat bursts, cards in view (86 frames, 146 cards) | Cards found at threshold 0.35 | .918 |

The deck numbers count SIFT-confirmed cards, which lean towards easy cards, so they are optimistic. The web
client's deck reader is the end-to-end reference.

## Limitations

- **Same image, different printing**: printings whose official images are pixel-identical (for example P cards)
  cannot be told apart; the answer is the plain printing of the same art
- **Small, far cards**: from a side seat, cards far away in the frame are found less often than with the
  previous corner finder (.918 against .973), in exchange for no false boxes on large cards
- **One row or a single card filling a deck photo**: the deck reader's largest tile is the photo's short side, so
  a card taller than that is cut at every scale and missed. An extra whole-photo pass in the reader fixes most of
  it; that is a reader change, not a model change
- **Layouts**: shingled or stacked cards, upright magnetic card displays and large logos on playmats are boxed
  wrongly or missed
- **Glare and blur**: heavy sleeve glare, strong blur and very oblique shots lower the recogniser's confidence;
  the scanner then shows candidates instead of an answer
- **New cards**: cards added to the card data after the index was built are not recognised until the index is
  rebuilt (the models need no retraining; see README, "Common tasks")
