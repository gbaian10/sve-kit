# Card labels — traced vector reconstructions

The outer frames and decorative silhouettes are traced from official card images.
The SVGs contain no embedded bitmap data; lettering is derived from OFL fonts.
The official designs remain the property of their respective owners. The bundled
font licenses cover font software and derived glyph outlines, not the official designs.

All glyphs are paths from SIL Open Font License 1.1 fonts:

- **Caudex Version 1.01** — [source](https://github.com/google/fonts/tree/main/ofl/caudex); [OFL-caudex.txt](OFL-caudex.txt).
- **Libre Caslon Text Version 2.000** — [source](https://github.com/google/fonts/tree/main/ofl/librecaslontext); [OFL-librecaslontext.txt](OFL-librecaslontext.txt).
- **Lusitana Version 1.001** — [source](https://github.com/google/fonts/tree/main/ofl/lusitana); [OFL-lusitana.txt](OFL-lusitana.txt).
- **Marcellus Version 1.000** — [source](https://github.com/google/fonts/tree/main/ofl/marcellus); [OFL-marcellus.txt](OFL-marcellus.txt).
- **Philosopher Version 2.001; ttfautohint (v1.8.4.7-5d5b)** — [source](https://github.com/google/fonts/tree/main/ofl/philosopher); [OFL-philosopher.txt](OFL-philosopher.txt).
- **Cinzel Version 2.000**, weight 700 (digits and plus sign in `counters/` and `cost/`) — [source](https://github.com/google/fonts/tree/main/ofl/cinzel); [OFL-cinzel.txt](OFL-cinzel.txt).

| Asset                                              | Typeface / true weight / additional path stroke                         | References                                                                                                                                          |
| -------------------------------------------------- | ----------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `evolve.svg`                                       | Lusitana 400 + 0.18px native-card-px                                    | SP01-U22, black rounded rectangle with thin gold rim                                                                                                |
| `advance.svg`                                      | Philosopher 400 + 0.25px native-card-px                                 | BP18-SP03, dark red rectangle with inward V-shaped notches at both ends and a thin gold rim                                                         |
| `leader.svg`                                       | Libre Caslon Text 600 + 0.00px native-card-px                           | SD06-LD01, BP20-LD01                                                                                                                                |
| `token.svg`                                        | Caudex 700 + 0.00px native-card-px                                      | SD07-T02, SD07-T01, BP21-T01                                                                                                                        |
| `follower.svg`                                     | Philosopher 700 + 0.00px native-card-px                                 | BP01-029, SP01-051, SP01-053                                                                                                                        |
| `spell.svg`                                        | Libre Caslon Text 400 + 0.15px native-card-px                           | BP01-063, BP21-012                                                                                                                                  |
| `amulet.svg`                                       | Marcellus 400 + 0.00px native-card-px                                   | BP01-150, BP21-018                                                                                                                                  |
| `crest.svg`                                        | Caudex 700                                                              | BP20-P64, pale ice-blue rounded label                                                                                                               |
| `equipment.svg`                                    | Marcellus 400, light fill with a dark outline                           | CP04-PR15, grey metal rounded label                                                                                                                 |
| `ep.svg`                                           | No lettering                                                            | PR-445, gold evolve-point orb                                                                                                                       |
| `sep.svg`                                          | No lettering                                                            | Official SEP card art: outline and feathers traced, colours measured from the card; centre disc and border are drawn geometry; flat (no highlights) |
| `main-deck.svg`                                    | No lettering                                                            | Official card back: black card, white corner frame, cyan magic circle; logo reduced to lines                                                        |
| `cost-blank.svg`                                   | No lettering                                                            | Blank green cost crystal matching the official text icons                                                                                           |
| `counters/counter-{atk,hp}-{0..9,10plus}-flat.svg` | Digits 0–9 and the plus sign: Cinzel 700 outlines, flat (no highlights) | Attack (blue spear) and health (red shield) counters of the official Special Starter Set 2025 dice, photographed by the maintainer                  |
| `cost/cost-{00..10,x}.svg`                         | Digits: Cinzel 700 outlines; the X is drawn                             | `cost-blank.svg` with the number on top, for the cost icon inside card text                                                                         |

Leader includes only the central plaque. EVOLVE and ADVANCE use the simple
rectangular labels printed in the card's type line. EVOLVE shares Follower's
67:23 aspect ratio; ADVANCE is slightly wider at 78:23. Lettering fills the inner
plate for legibility at 18–24 px. No font loading or raster embedding is needed.
The lettering on these two labels is condensed horizontally to fit the frame,
with an offset dark shadow behind the glyphs.
Stroke widths scale with the viewBox. Gradients and textures are vector approximations.

The orbs and the card back carry no lettering. The filter shows the main deck's name beside its
card back, writes the cost on the blank cost crystal and uses the finished attack and health
counters for those two stats. The EP glyphs are redrawn by hand.

The counters and the new SEP are traced from the maintainer's photographs of the official
dice in the Special Starter Set 2025 and from the official SEP card art; they are pure vector with no
bitmap. The digits 0–9 and the plus sign are Cinzel 700 glyph outlines converted to paths (SIL OFL 1.1);
the X on the cost icon is drawn to match. Counter digits are set at 80% of the full face size,
except the health counter's 10+, which stays at 100%. The official designs remain the property of their
original rights holders.

The vector reconstructions were produced with Codex gpt-6.1-sol (EVOLVE to Amulet) and Claude Opus 5.5
(Crest, Equipment, EP, SEP, main deck, blank cost, counters, cost digits) and are distributed under
Apache-2.0. The imitated official designs are excluded from the project license
grants; see [LICENSING.md](../../../../../LICENSING.md). Font copyright and
license notices above are included with trailing whitespace removed.
