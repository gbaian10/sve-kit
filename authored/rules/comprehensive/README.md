# Comprehensive rules

The official Japanese comprehensive rules (総合ルール), parsed from the official PDF for the
website's rules page. The text is shown for reading and clause links only; it is not adoption
evidence, and the original PDF remains preserved in the source archive.

`jp/<version>/index.json` records the version, the official last-updated date, the rules page and
PDF URLs, the PDF's SHA-256 and the files in reading order. Each `chNN.json` holds one chapter and
`appendix-a.json`–`appendix-c.json` the appendices, as an ordered `nodes` array:

| Field | Meaning |
| --- | --- |
| `key` | Unique per version: the body clause number (`1.3.2.2`), or `<context>:<number>` in appendices |
| `context` | `main` (body), `A` (tokens), `B` / `B-1` / `B-2` (special formats), `C` (update history) |
| `number` | Official number as printed; absent for unnumbered appendix blocks |
| `kind`, `depth` | `chapter`, `section`, `rule` or `block`, and nesting depth |
| `title`, `text` | Heading text and body; `\n` separates printed lines such as `*` bullets |
| `examples` | `例：` notes under the clause, omitted when there are none |
| `table` | `header` and `rows` of an appendix A token table |
| `figure` | A printed figure not reproduced here: its PDF page and labels |
| `changed` | Printed in red, the official marker for content updated in this version |

Appendix B-1 and B-2 restate body clause numbers (and skip some), so their nodes carry their own
context; a citation inside them resolves to the same appendix first, then to the body.

Inline icons use the site's card-text markers: `{起動}`, `{アクト}`, `{ファンファーレ}`, `{攻撃力}`
and so on. The PDF's own token tables already print `《…》` markers. Icons without a site symbol
keep a descriptive marker (`{PP}`, `{クリティカルトリガー}`).

Japanese text drops the spaces that the PDF places between full-width and half-width characters
(`2 名` → `2名`); nothing else is rewritten. Our structure is CC0-1.0; the official rules text is
excluded from the project's licenses, see [LICENSING.md](../../../LICENSING.md).
