# Comprehensive rules

The official comprehensive rules, parsed from the official PDFs for the website's rules page: the
Japanese rules (総合ルール) under `jp/` and the English Comprehensive Rules under `en/`. The two are
separate documents with their own versions and update dates; each is kept as published, and
neither fills in clauses the other lacks. The text is shown for reading and clause links only; it
is not adoption evidence, and the original PDFs remain preserved in the source archive.

`<region>/<version>/index.json` records the edition, the version, the official last-updated date,
the rules page and PDF URLs, the PDF's SHA-256 and the files in reading order. Each `chNN.json`
holds one chapter and `appendix-a.json` onwards the appendices, as an ordered `nodes` array:

| Field | Meaning |
| --- | --- |
| `key` | Unique per version: the body clause number (`1.3.2.2`), or `<context>:<number>` in appendices |
| `context` | `main` (body), `A` (tokens), `B` / `B-1` / `B-2` (special formats), `C` (update history, Japanese only) |
| `number` | Official number as printed; absent for unnumbered appendix blocks |
| `kind`, `depth` | `chapter`, `section`, `rule` or `block`, and nesting depth |
| `title`, `text` | Heading text and body; `\n` separates printed lines such as `*` bullets |
| `examples` | `例：` / `Example:` notes under the clause, omitted when there are none |
| `table` | `header` and `rows` of an appendix A token table |
| `figure` | A printed figure not reproduced here: its PDF page and labels |
| `changed` | Printed in red, the official marker for content updated in this version |

Both editions number clauses the same way, so a clause number present in both shares its page
anchor; a clause only one edition has exists only there.

Appendix B-1 and B-2 restate body clause numbers (and skip some), so their nodes carry their own
context; a citation inside them resolves to the same appendix first, then to the body.

Inline icons use the site's card-text markers. The Japanese text uses `{起動}`, `{アクト}`,
`{ファンファーレ}`, `{攻撃力}` and so on, and its token tables already print `《…》` markers. The
English text uses the English card text's markers (`{icon_stand|[engage]}`, `{icon_act|[act]}`,
`{icon_fanfare|[fanfare]}`, `{icon_power|[attack]}`…), including for the `{FANFARE}`-style text
markers its token tables print. Icons without a site symbol keep a descriptive marker (`{PP}`,
`{クリティカルトリガー}`, `{Critical Trigger}`).

Japanese text drops the spaces that the PDF places between full-width and half-width characters
(`2 名` → `2名`). English printed lines join with a space, except after a line-final hyphen or
dash, which the PDF prints as part of the word (`non-` + `active`). Inside an English token's text
cell, whether a printed line ends a paragraph follows the official card text, because the PDF
keeps no paragraph structure there. Nothing else is rewritten. Our structure is CC0-1.0; the
official rules text is excluded from the project's licenses, see
[LICENSING.md](../../../LICENSING.md).
