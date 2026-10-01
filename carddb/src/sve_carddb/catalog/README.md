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

These are typed build inputs, not a new authored YAML format. Human adoption
of vocabulary mappings, localization, and symbol spellings remains a caller
responsibility. Symbols, authored aliases, and special construction names must
reference a confirmed decision with authored evidence already loaded in the DB.
This API does not load authored envelopes or certify their membership/freshness;
the upstream adoption loader must validate those before supplying the catalog.
Discovery results are candidates and must not be fed back as adopted inputs.

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
occurs here. Explicit `collab`/`treated_as` bindings require a confirmed authored
decision and a physical face in that region. Physical double-face counting
belongs to the pinned construction rules.
