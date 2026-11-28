# Card scanner models: model card

What the published card scanner models are, what they were built from, under which terms, and where they fail.
How to rebuild them is in [README.md](README.md); the published file format is in
[docs/schema/scan/artifacts.md](../docs/schema/scan/artifacts.md).

| Published                                            | What it does                                                   | Size                                                |
| ---------------------------------------------------- | -------------------------------------------------------------- | --------------------------------------------------- |
| `det-1` (corner finder)                              | Finds the four corners of every card in a 256×256 view         | 1.1 MB (weights-only int8 ONNX)                     |
| `rec-1` (recogniser)                                 | Turns a straightened card into a 256-dimensional vector        | 4.7 MB (weights-only int8 ONNX)                     |
| Card index (per card edition, per card data version) | One vector per printing face, searched with the `rec-1` vector | jp 3.40 MiB, en 3.32 MiB (gzip ≈ 2.12 and 2.16 MiB) |

## Licensing and provenance

- **Code**: the project's own code and documentation under `scan/` are Apache-2.0; official and other
  third-party material stays excluded as described in [LICENSING.md](../LICENSING.md)
- **Weights and card index**: not licensed by the project. They are computed from official card images and, like
  snapshots and card images, do not acquire the license of the program that made them (see LICENSING.md). They
  are published for the web app only, the same way as the card images
- **Base weights**: both models start from timm ImageNet-1k weights released under Apache-2.0. The published
  weights are derived from them. The upstream Apache-2.0 terms and notices still apply to those components;
  this provenance note records the source and does not replace the notices a release must carry

| Model   | Base weights                                                                                          | License    | Pretraining data |
| ------- | ----------------------------------------------------------------------------------------------------- | ---------- | ---------------- |
| `rec-1` | [`timm/mobilenetv3_large_100.ra_in1k`](https://huggingface.co/timm/mobilenetv3_large_100.ra_in1k)     | Apache-2.0 | ImageNet-1k      |
| `det-1` | [`timm/mobilenetv3_small_100.lamb_in1k`](https://huggingface.co/timm/mobilenetv3_small_100.lamb_in1k) | Apache-2.0 | ImageNet-1k      |

### Training data

| Data                                                                                                     | Used for                                                        | Terms                                            | Distributed                               |
| -------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------- | ------------------------------------------------ | ----------------------------------------- |
| Official card images (`card_l` of a public export)                                                       | Both models (synthetic scenes and queries), the card index      | Official content, outside the project's licenses | As card images, by the web app            |
| [Poly Haven](https://polyhaven.com/) table textures and indoor panoramas                                 | Corner finder backgrounds                                       | CC0                                              | No (fetched by `fetch-bg`, `fetch-rooms`) |
| [`antokun/pokemon-card-segmentation`](https://huggingface.co/datasets/antokun/pokemon-card-segmentation) | Corner finder, real photos of cards                             | CC0-1.0                                          | No                                        |
| Second-hand marketplace listing photos                                                                   | Foil texture layers (recogniser and corner finder augmentation) | Not ours; private                                | No; only the code that builds the layers  |
| Players' social media posts (deck photos)                                                                | Sleeve glare layers; corner labels for the corner finder        | Not ours; private                                | No; only the code                         |
| The maintainer's own phone photos                                                                        | Evaluation only                                                 | —                                                | No                                        |

Deck photos from the held-out events in `sve_scan/splits.py` never feed training, labels or texture layers.

## Intended use

- Identifying Shadowverse: EVOLVE cards in photos in the `sim/web` scanner: one card aimed at by the camera, or
  every card of a laid-out deck photo
- The answer is a candidate list for the user to confirm; it is not a ruling on which card a physical card is

Out of scope: other card games, card grading or condition, telling printings apart that share the same official
image.

## In the web client

The models run in `sim/web` (`src/domain/scan/`, `src/pages/scan/`); the Python code here only evaluates them.

1. **Corner finder**: around the aiming frame, or on overlapping tiles of a deck photo; each card is straightened
   to 224×312
2. **Recogniser**: one vector for the card upright and one turned 180°. The deck reader also reads a card that
   looks like it lies on its side turned a quarter, and takes that reading only when its similarity is at
   least 0.02 higher
3. **Card index search**: an int8 dot product with every row, computed with WebAssembly SIMD (`dots.wat`; plain
   JavaScript where the browser lacks SIMD). Score = similarity − `hubWeight` × hub; a card counts as recognised
   when its similarity reaches `accept` and it leads the next card by `margin` (artifact format)
4. **Printing check**: the recogniser treats every printing of one art as one card, and the client shows the
   plain printing, the one with a plain numeric card number such as `BP01-001`. It then compares
   the frame colours of the straightened card with each printing's official small image (`finish.ts`). It uses
   hue after a gray-world white balance, in six frame regions, only where the printings differ by more than 25°.
   Another printing must lead the plain one by 10°; the live camera smooths this over frames. On official images
   (each printing's large image as the photo, 2,684 same-art groups), SL printings picked themselves 788 of 804
   times and plain printings 906 of 910; groups whose frames match the plain one stay plain

## Evaluation

The real photos in this section are held out (never used in training). Card level: different printings of the
same card count as the same answer.

| Set                                                                                                                                       | Metric                                                                                   | Result                |
| ----------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------- | --------------------- |
| Marketplace listing photos, three sets (206, 44, 40)                                                                                      | Recogniser top-1 (centre crop)                                                           | .951, 1.0, .875       |
| Foil cards (100)                                                                                                                          | Recogniser top-1 (centre crop)                                                           | .920                  |
| Video frames (39)                                                                                                                         | Recogniser top-1 (centre crop)                                                           | .821                  |
| Maintainer's photos of new Japanese cards (22 photos; 165 cards with an estimated 40% or more visible, cut at their SIFT-located corners) | Recogniser top-1: exact corners; each corner moved up to 4% of the card width and height | .964; .958            |
| Deck photos of 6 held-out events (35 photos, 894 SIFT-confirmed cards)                                                                    | Named right; accepted (of those, wrong)                                                  | 861 (.963); 825 (2)   |
| Synthetic deck photo, 12 large cards in 2 rows                                                                                            | Cards found                                                                              | 12/12, no false boxes |
| Maintainer's single-card phone saves (63), before the last recogniser stage                                                               | Card named right                                                                         | .857                  |
| Side-seat bursts, cards in view (86 frames, 146 cards)                                                                                    | Cards found at threshold 0.35                                                            | .918                  |

Two trainings with the same recipe got 392 and 399 of the 429 single-card photos, 7 apart; the published
`rec-1` is the 399 run. The new-card photos
show cards spread on a table, partly overlapping; the corner finder does not box those well (see
"Limitations"), so the cards are cut where SIFT matching against the official image puts them. The visible share
is estimated as the area of the SIFT matches' convex hull over the card's area.

Synthetic queries made from official images (nine degraded copies per printing) serve as a regression check.
The recogniser was trained on most of these images, so they are not a held-out test. Top-1: new Japanese
printings (CP05, SCS02 and new PR cards; 3,519 queries) .905, new English printings (5,121 queries) .919, and a
sample of 400 older Japanese printings held out in the previous training stage (3,600 queries) .918; most of
those 400 are on the training side of the last stage's split.

The deck numbers count SIFT-confirmed cards, which lean towards easy cards, and cards the corner finder missed
are not in the denominator, so they are optimistic. The 894 cards are the union of the SIFT-confirmed cards
from three runs (this model, the previous stage and the other training with the same recipe; boxes whose
confirmed cards disagreed are dropped), so every model is scored on the same cards; a single `eval-deck` run
uses only its own confirmed cards as the denominator. The web client's deck reader is the end-to-end reference.

## Limitations

- **Same image, different printing**: SL printings are told apart by their frame colours (see "In the web
  client"); printings whose official images are pixel-identical (for example P cards) cannot be, and the answer
  is the plain printing of the same art. Glare or a colour cast makes the check less reliable; when no other
  printing leads the plain one by at least 10°, the plain printing is shown
- **Small, far cards**: from a side seat, cards far away in the frame are found less often than with the
  previous corner finder (.918 against .973), in exchange for no false boxes on large cards
- **One row or a single card filling a deck photo**: the deck reader's largest tile is the photo's short side, so
  a card taller than that is cut at every scale and missed. A reader change, not yet in the web reader: in an
  offline test of the Python twin, an extra pass over the whole photo (padded to a square) found 48 of 53 cards
  of synthetic one-row layouts instead of 25
- **Layouts**: shingled or stacked cards, upright magnetic card displays and large logos on playmats are boxed
  wrongly or missed. In the maintainer's photos of the newest set, overlapping GBF-frame cards got boxes on the
  art window only
- **Glare and blur**: heavy sleeve glare, strong blur and very oblique shots lower the recogniser's confidence;
  the scanner then shows candidates instead of an answer
- **New cards**: cards added to the card data after the index was built are not recognised until the index is
  rebuilt (usually without retraining; see README, "Common tasks"). Of the cards added in the last training stage
  (see README, "Recogniser"), the new Japanese cards were checked on synthetic queries and the maintainer's 22
  photos, and the new English cards on synthetic queries only. The Leader card CP05-LD09 (sparkle finish)
  appears twice in those photos: the correct card ranked first in one crop but failed the offline acceptance
  check, and another Leader ranked first in the other (about 31% visible)
- **Held-out split**: about 8% of cards are held out of training, drawn from the gallery of each training run.
  The last stage ran on a larger gallery than the stages before it, so some of its held-out cards were trained
  on earlier and its synthetic held-out numbers are not a clean test. The real-photo sets never feed training
  and are unaffected
