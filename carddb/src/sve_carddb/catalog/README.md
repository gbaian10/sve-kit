# Vocabulary and construction-name staging

`populate_text_preview` derives construction names and accepts an optional
`catalog_config`. `populate_catalog` can also run inside a caller-owned build
transaction. Use `catalog_configuration(config, published)` together with
`text_configuration` in `BuildContext`; the exact catalog and historical text
union must be pinned. The transaction verifies all polymorphic alias targets
through a fixed SQL union before commit. Disabled target tables cannot be
substituted with a vocabulary entry using their reserved kind.

The caller supplies explicit `Term` labels, codes, UI `Language` fallbacks,
`Alias` normalization results/version, and adopted `Symbol` declarations.
Existing rows may be reused only when identical. There is no implicit language
configuration, transliteration, alias normalizer, or advanced search grammar.
Fixed enum terms use the same typed term input; this API does not assign new
vocabulary kinds to arbitrary enum columns. UI fallback never changes card
region or the language of source card text.

These are staging inputs, not a finalized authored YAML format or an adoption
loader. Decision-backed aliases, symbols and special construction names always
fail closed, even for a confirmed decision with authored evidence. Such a decision
could concern another category or member, or stale reviewed inputs. Reopening
this gate requires a finalized envelope and loader that validate the adoption
category, complete exact member set and freshness against the pinned inputs.
There is no caller flag that bypasses this gate. Synthetic tests of downstream
projection use isolated fixtures; their acceptance is not adoption evidence.

契約與 loader 定案前不經這條路徑放任何真實詞彙／記號／別名／特殊名稱資料。

Terms and aliases without decisions remain staging projections only. Discovery
results are candidates and must not be fed back as adopted inputs. Automatic
exact regional names derived from official observations do not use this gate.

`resolve_alias` prefers exact canonical codes and returns every normalized alias
target, sorted. Multiple results require a choice; they are never selected by
row order. Unknown normalizer versions fail explicitly. Keyword aliases use
keyword IDs, family/stamp aliases use their codes, card aliases use card IDs,
and other registered kinds refer to vocabulary `(kind, code)`.

`parse_symbol` accepts only literal, ASCII unsigned integer, or exact declared
variable spellings. The existing public parameter contract permits only `X`
as a variable. `Q` can remain literal text; it does not extend that whitelist.
The match retains `raw` exactly, including leading zeros. Unknown or ambiguous
spellings return the original text with no symbol ID. Symbol occurrence alone
does not infer a card's own mechanics or engine support.

Construction names use `(region, exact official name)`. Adopted current names
take precedence; when effects are pending but every observed name is identical,
that name can still form the construction group. Conflicting observed names
without current remain unresolved. Derived IDs are `rn:v1:` followed by the full
SHA-256 of canonical `[region, official_name]`; reuse must match exact content.
No trimming, Unicode folding, translation, card merging, or deck copy counting
occurs here. The special-name projector requires a physical face in the requested
region and exact reuse of its decision binding; its decision-backed catalog
import remains closed as described above. Physical double-face counting belongs
to the pinned construction rules.
