# Official assets

Images in this directory are © Cygames, Inc. / © bushiroad, taken from the official
Shadowverse: EVOLVE website (downloaded 2026-09-27). They are **not covered by this repository's
licence**; they are reproduced here only so the site can show the same icons the official card
pages use. Do not reuse them outside sve-kit.

| Path        | What                                               | Source                                                                                                                                                                            |
| ----------- | -------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `texticon/` | 32 card-text symbols, 52×52 PNG (`icon_fanfare` …) | `<img>` elements inside card text on official card pages, `https://shadowverse-evolve.com/wordpress/wp-content/images/texticon/<name>.png` (the English site uses the same files) |
| `class/`    | 7 class icons, 90×90 PNG                           | **Cropped by us** from `HPクラス_re.png` on the official rules page `https://shadowverse-evolve.com/rules/ver_0324/`                                                              |
| `logo/`     | Site logo, 290×290 PNG                             | **Cropped by us** from the official "今すぐ始める" video thumbnail `https://shadowverse-evolve.com/wordpress/wp-content/uploads/2022/04/26183024/thumbnail-1024x576.png`          |

The official site has no icons for keyword abilities such as 守護 or 疾走; card text renders those
as text chips, never as drawn icons. Class icon file names use the project's class codes
(`forest sword rune dragon abyss haven neutral`, see `docs/terminology.md`).
