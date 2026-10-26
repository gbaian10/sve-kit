# Official assets

Images in this directory are © Cygames, Inc. / © bushiroad, taken from the official
Shadowverse: EVOLVE website (downloaded 2026-09-27), or, for the class icons, from the official
rules PDF listed in the table below and traced by hand into SVG. They are **not covered by this
repository's licence**; they are reproduced here only so the site can show the same icons the
official card pages use. Do not reuse them outside sve-kit.

| Path        | What                                                                                             | Source                                                                                                                                                                            |
| ----------- | ------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `texticon/` | 32 card-text symbols, 52×52 PNG (`icon_fanfare` …)                                               | `<img>` elements inside card text on official card pages, `https://shadowverse-evolve.com/wordpress/wp-content/images/texticon/<name>.png` (the English site uses the same files) |
| `class/`    | 7 class icons, the official designs traced by us into SVG                                        | [Official comprehensive rules 1.28.0 PDF, page 3](https://shadowverse-evolve.com/wordpress/wp-content/uploads/2026/09/28121513/ShadowverseEVOLVE_cr_1.28.0_260928.pdf)            |
| `logo/`     | Site logo, 290×290 PNG; `favicon-32.png` and `apple-touch-icon.png` (180×180) are it scaled down | **Cropped by us** from the official "今すぐ始める" video thumbnail `https://shadowverse-evolve.com/wordpress/wp-content/uploads/2022/04/26183024/thumbnail-1024x576.png`          |

The official site has no icons for keyword abilities such as 守護 or 疾走; card text renders those
as text chips, never as drawn icons. Class icon file names use the project's class codes
(`forest sword rune dragon abyss haven neutral`, see `GLOSSARY.md`).
