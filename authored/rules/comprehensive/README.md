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

## Updating an edition

Use absolute paths for the command inputs and outputs below.

Check the official rules version when adding a card set. If an edition changed, save its PDF
in the source archive and use that copy as the input. Keep the PDF outside Git. Update the
Japanese and English editions independently.

1. **Parse.** From the repository root, generate a new version directory. Supply the rules page
   and PDF URLs from the official site. The parser reads the version and date from the cover
   and records the SHA-256 of the PDF it used.

   ```bash
   uv run --locked sim/web/scripts/rules/parse_pdf.py "$pdf" "$new_rules" \
     --region jp --page-url "$page_url" --source-url "$source_url"
   bun fmt "$new_rules"
   ```

   The command expects the official, text-based Word PDF layouts. Check any new image marker
   against the PDF and map it to the site's card-text marker in `parse_pdf.py`. Review token
   paragraph boundaries against the official card text when a line can be either a wrap or a
   paragraph break. If a previous rules directory was replaced in Git, restore it to a scratch
   directory from the corresponding commit.

2. **Compare.** Review the readable report and save the JSON when another tool needs the list.
   Changes are matched by node key. A red update marker or a figure's pagination alone leaves a
   node unchanged for translation. Changed token tables also list rows by card name. Appendix A
   and C keys use block order, so inserting a block can shift the keys of later blocks.

   ```bash
   bun run --cwd sim/web rules:revision diff "$old_rules" "$new_rules"
   bun run --cwd sim/web rules:revision diff "$old_rules" "$new_rules" --json > changes.json
   ```

3. **Carry and translate.** For Japanese rules, use the previous Traditional Chinese translation
   to start the new version as described in the [translation workflow][translations]. The
   carry command takes a new output directory and reports the node keys to translate.
4. **Stamp and test.** Stamp the translated entries and run the rules data tests. Commit the new
   source JSON and the reviewed translations for the new version, and update the applicable rules
   bundle in `sim/web/src/data/rules/` so the site loads it. Include the changes found by the
   comparison in the PR.

[translations]: ../../translations/comprehensive-rules/README.md#updating-a-translation
