# Battle markers

Art for the markers that sit on cards during a battle. Everything here is pure vector, with no
embedded bitmap data.

| File                                               | What it is                                                                                    |
| -------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| `counter-atk-blank.svg`, `counter-hp-blank.svg`    | The attack (blue spear) and defense (red shield) faces without a number                       |
| `numerals.json`                                    | Outlines of the digits drawn on those faces: Cinzel 700, SIL OFL 1.1                          |
| `kw-{guard,intimidate,bane,drain,aura,assail}.svg` | The icons of the keyword dice                                                                 |
| `sep.svg`                                          | The super-evolve point marker, with highlights (the flat one is in `src/assets/card-labels/`) |

The faces and icons are traced from the maintainer's photographs of the dice in the official
Special Starter Set 2025, and the SEP marker from the official SEP card art, in the same way as the
counters documented in [`src/assets/card-labels/README.md`](../../../assets/card-labels/README.md);
the font notice for Cinzel is there too ([OFL-cinzel.txt](../../../assets/card-labels/OFL-cinzel.txt)).
The official designs remain the property of their respective owners and are excluded from the
project's license grants; see [LICENSING.md](../../../../../../LICENSING.md).

`Dice.tsx` draws a number on a blank face at any value, lays the face on a die and turns the
keyword icons into dice with a caption in the UI language.
