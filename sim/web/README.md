# sim/web

The web client: card browser, deck builder and game board in one app (React, Vite, Tailwind CSS v4, i18next).
Architecture: [`docs/sim/web-architecture.md`](../../docs/sim/web-architecture.md).

## Setup

Bun and Node.js are pinned in the repo-root `mise.toml` (`mise install`); the git hooks call them
through `mise exec`, so they do not depend on shell activation. Bun is the package manager; `bun run`
executes the Node-shebang CLIs (Vite, Vitest, ESLint, …) with Node, and Vitest's jsdom tests do not
run on the Bun runtime (see Known issues).

```bash
cd sim/web
bun install
bun run dev                     # http://localhost:5173
bun run dev -- --host 0.0.0.0   # also reachable from phones on the LAN
```

## Scripts

| Script                 | What it does                                             |
| ---------------------- | -------------------------------------------------------- |
| `bun run check`        | Everything below, in order; run it before pushing        |
| `bun run format:check` | Prettier (`bun run format` rewrites)                     |
| `bun run lint`         | ESLint (`lint:js`) and Stylelint (`lint:css`)            |
| `bun run typecheck`    | `tsc -b` with TypeScript 6                               |
| `bun run test`         | Vitest, including the ESLint and Stylelint rule fixtures |
| `bun run build`        | Production build into `dist/`                            |

`bun test` is Bun's own test runner and ignores the Vitest config. Always use `bun run test`.

The pre-commit hooks run Prettier, ESLint and Stylelint on changed files; the pre-push hook runs
the full `tsc -b`. Vitest is a manual hook (`pre-commit run --hook-stage manual web-test`); CI runs
the whole `bun run check` on every web change.

## Theme tokens

`src/styles/tokens.css` is the only file allowed colour literals. Every base colour token is
defined in three contexts: bare `:root` (light), `:root:not([data-theme="light"])` inside
`@media (prefers-color-scheme: dark)`, and `:root[data-theme="dark"]`. Accent tokens
(`--accent`, `--accent-ink`, `--accent-text`, `--accent-soft`) are defined per context for
`data-accent="amber|teal|red"`, plus a per-theme default when the attribute is absent (dark →
amber, light → teal). `tests/tokens.test.ts` parses the file and fails when a context misses a
token.

`src/styles/index.css` clears Tailwind's palette and registers only semantic colours
(`bg`, `surface-1..3`, `well`, `side`, `border`, `border-strong`, `text-1..3`, `scrim`, the
semantic `info/success/warning/danger` pairs, `class-*`, `cost`, `def`, `accent*`), the radius,
text-size and breakpoint scales (`md` 600, `lg` 1000, `xl` 1440). Class colours are used in small
areas only: put `data-class="forest"` (or any of the seven class codes) on an element and use
`border-class-current` / `text-class-current` inside it.

Which theme is active is decided by `resolveTheme()` in `src/domain/theme.ts` (a stored `theme`
of `system` and a `null` accent leave `<html>` attribute-less so the CSS falls back to the media
query and the default accent). `src/app/theme-attributes.ts` applies the attributes when the
preferences change.

**Exception to the storage rule:** `public/theme-boot.js` is a plain script loaded first in
`<head>` so a stored dark theme never flashes light. It lives outside `src/`, so the "only
`src/settings/` touches storage" lint does not cover it. It reads only the `theme` and `accent`
fields of `sve-kit:prefs`, validates them against a whitelist and swallows every error;
`src/settings/theme-boot.test.ts` runs it against good, corrupt and blocked storage and checks it
agrees with `applyThemeAttributes`.

## Settings

All user preferences live in one object stored under `localStorage["sve-kit:prefs"]`
(`src/settings/prefs.ts`): UI language, card edition, name/effect display, symbol labels, theme,
accent, ban region, data saver, grid density, view mode. Reading validates every field on its own
and falls back to the default, so a stale or hand-edited value never breaks the app; reading and
writing are wrapped in try/catch because even touching `localStorage` can throw. The object
carries `v: 1`; a missing or different version is treated as another build's data and ignored
(the boot script applies the same rule). Components subscribe with `usePrefs()`;
`prefsStore.reload()` re-reads storage. Recently viewed cards use `sve-kit:recent`
(`src/settings/recent.ts`), capped and de-duplicated on read as well as on write.

## i18n

UI text comes from `t()`. A `label` fails lint when its own fixed text (a string, or the literal
parts of a template) contains letters, or when a `+` chain up to four levels below the label has
such a string as an operand. Pure interpolation such as `` `${a}/${b}` `` passes. Deeper chains,
and text that arrives through a function or variable, are left to review.

Use one key per sentence and pass values through interpolation: `t("cost", { n })`, never
`` `${t("a")} ${n}` ``. Lint cannot catch that kind of sentence assembly, so review has to.

`src/i18n/static.test.ts` checks that the three locales have the same keys with the same
`{{placeholders}}`, and that the zh-TW file contains none of the Simplified-only characters
listed in that test (extend the list when a new one slips through review).

### Text length

Japanese and English strings are often longer than the zh-TW ones, so the layout never assumes a
length: text containers have no fixed width, and a setting row (`SettingRow`) lets its control
wrap under the label when both do not fit on one line. Controls that cannot wrap (segmented
pills) get a width budget instead: `static.test.ts` fails when any `options.*` label in any
language is wider than about 6 em, so the fix is always to shorten that language's copy, never
to widen the control. Language names use each language's own name (繁體中文 / 日本語 / English) in
every locale.

Two dev-only tools catch what the tests cannot:

- `src/app/overflow-audit.ts` logs a console error whenever a visible box is narrower than its
  content (after DOM changes, resizes and font loading), and `window.__sveOverflowAudit()` returns
  the same report on demand. The headless self-check runs every screen in all three languages and
  treats any console error as a failure.
- `?pseudo` in the dev URL stretches every string by 35% with accented letters and brackets
  (`[Çáŕð ñáḿéš ~~~ ~]`), which shows cut-off ends and squeezed rows before a real translation
  does. Its findings are advisory (the three real languages are the gate): a segmented group that
  only overflows at +35% is a warning about a fourth language, not a bug in the current three.

## Lint rule fixtures

`tests/lint-fixtures/` is a small fake project full of code that must, or must not, trip the lint
rules. Each case starts with `// case: <what> -> <rule ids | none>`; `// bypass:` marks holes the
rules knowingly cannot close. `tests/eslint-rules.test.ts` lints every file under the fixture
`src/` and checks that each case reports exactly the rules it declares, as many times as it
declares them (list a rule twice when it fires twice). Update the fixtures together with the rules.

## Snapshot reader

`src/data/format-v1/` is the pure core of the card-data reader: no `fetch`, no DOM. It follows
[docs/schema/snapshot-format.md](../../docs/schema/snapshot-format.md),
[snapshot-transport.md](../../docs/schema/snapshot-transport.md) and
[snapshot-contract.md](../../docs/schema/snapshot-contract.md), and mirrors the Python reference
reader in `carddb/src/sve_carddb/snapshot/`.

- `json.ts`: the strict JSON boundary (canonical-json-v1). Bytes are parsed by our own parser, so
  duplicate keys, floats, unsafe integers, a BOM and lone surrogates are rejected instead of being
  silently accepted the way `JSON.parse` would; `canonical()` writes the exact bytes hashes refer to.
- `sha256.ts`: synchronous SHA-256 and the `sha256-mod-v1` bucket function.
- `schema.ts` + `validator.ts`: the published JSON Schema is loaded from `carddb/` at build time
  (one source of truth, no copy) and interpreted by a small validator that supports exactly the
  keywords the schema uses; an unknown keyword fails at load, so the schema cannot quietly mean
  more than the reader checks. `schema.test.ts` runs the shared positive and negative examples
  through both this validator and ajv, which must agree.
- `decode.ts`, `reader.ts`, `semantics.ts`: fixed accessors from `x-columns`/`x-types`,
  manifest and container checks, `row_index`/`face_ordinal` joins, reference closure and the
  cross-row rules JSON Schema cannot express. Every rejection is a `SnapshotError` with a fixed
  `code`; tests match codes, never message text.

`contract.test.ts` is the conformance harness against `tests/fixtures/snapshot-contract/v1/`:
the golden manifest and payloads must join into exactly `expected-logical.json`, `text-all.json`
must give the same view, and every `reader-invalid.json` mutation must fail with its intended
code. The `reader.test.ts` cases cover what the fixture cannot express (missing files, cycles,
reformatted bytes). CI runs these whenever `carddb/src/sve_carddb/snapshot/schema/**` or the
fixture directory changes.

## Development snapshot

`fixtures/snapshot/` is a small synthetic card-data snapshot in the exact layout the CDN will
have (version index, manifest, canonical blobs, WebP placeholders). `bun run fixture:build`
regenerates it from `scripts/fixture/cards.ts` (about two dozen handwritten cards covering
double faces, alternate printings, errata, Q&A, bans, the JP/EN mapping states and image
states) through `scripts/fixture/build.ts`, which emits the column partitions, sorts every
collection the way the reader requires and hashes every blob. The output is deterministic and
the reader must accept it (`scripts/fixture/build.test.ts`), so a change to either the reader or
the generator that breaks the contract fails the tests, not the pages.

`bun run dev` and `bun run preview` serve a snapshot root under `/cdn`: `SVE_CDN_DIR` when set
(for a real local export), else the fixture. `SVE_PREVIEW_DIR` is served under `/cdn-preview`.
The files are canonical JSON and are excluded from Prettier.

`bun run fixture:local --limit 300 --sets BP01,BP02` builds a realistic local snapshot from the
private Japanese card list (`SVE_TEST_SNAPSHOT`) and the crawled card images under `SVE_DATA_DIR`,
into `SVE_CDN_DIR` (or `--out`), through the same generator (`scripts/fixture/local.ts`). It is
for demos on this machine only: it never goes into git, and it goes away once real snapshots
ship. Cards get one JP printing, no translations and no engine data; Q&A is shared between the
cards it names; vocabulary is whatever the records use. The output directory must be outside the
repository and the card list's directory, and must not contain `SVE_DATA_DIR`; an existing directory
is only replaced when it carries the `.sve-local-snapshot` marker the tool writes.

## Search and the card list

`/cards` is described by its URL alone (`docs/sim/web-architecture.md` §2.1): `src/domain/query/`
holds the state, the codec (`q class cost type mech set rarity alt unit sort view`; defaults are
never written) and nothing else. Matching lives in `src/domain/search.ts` as pure functions over
`SearchEntry` rows that `src/data/catalog.ts` flattens from the bootstrap: card-number exact/loose
hit (`bp01-51`, `BP01 051`, `BP01-051EN`) > number prefix (`bp01-5` lists BP01-050…) > name prefix

> name contains, names in every shipped language plus `search_alias` rows of kind `card`. The
> suggest list and the results page share that ranking, so the sequence prev/next follows is the same
> one the user saw.

Card aliases: the snapshot's `search_alias.code` must match `Code` (no colon), so the fixture writes
the card id without its `c:` prefix and the catalog prepends it back. This is an assumption raised
in #4; change `createCatalog` if the exporter settles on another form.

The list keeps `pages` and `anchor` on its own history entry through `useListEntryState()`
(router `navigate(..., { replace: true, state })`, never `history.replaceState`). Opening a card
writes the anchor first, then pushes `/cards/:cardNo` with `{ background, source, pages, resultKey }`;
`src/app/listEntryState.test.tsx` pins that replace-then-push keeps both states in the data router.

Each grid cell shows the name with the card number at the right of the same line (user request
2026-09-29), then the translation line when the name display asks for one.

`CardImage` is one fixed 63:88 slot: the text card (name, class frame, cost, number, stats) sits
underneath and the image fades in over it; missing, pending and withdrawn images keep the text card
with a reason, and data saver waits for a tap. Callers set the width (`w-full`, `w-8`).

The class quick bar shows labels only when an off-screen labelled copy fits the row width: the six
classes of the physical game plus neutral. A class code the design does not know yet would use the
neutral colour and an initial.

## Filters, views and sorting

Every facet of `QueryState` applies in `apply` (`src/domain/search.ts`): text, classes, cost range
(7 means 7 and above), types, mechanics, sets, rarities and alt art only. Types, cost and mechanics
are judged per printing in that printing's region, so a card stays listed while any eligible printing matches and its representative is chosen among the printings that pass; `unit` then decides what one result is: a card (the typed printing, else the
edition's default among the eligible ones), an art group (`printing.art_id`, else the printing
itself) or a printing. Sorting is one comparator per `QuerySort` (`COMPARATORS`): the default keeps
the search rank and snapshot order, the others compare cost / attack / defense / name / release
date with unknown values last and fall back to that order on ties.

`src/domain/mechanics.ts` is the tri-state of architecture §4.6: `present` from
`mechanic_projection`, `absent` only when the card's `card_mechanic_coverage` row proves the keyword
was checked (include / exclude modes, `complete_all`), `unknown` otherwise; an EN printing whose
`card_engine_support` has an EN block degrades a shared-scope answer to `unknown`. The filter sheet
prints "annotated N / M cards" from the same coverage rows. The design's "search the text instead"
entry waits for effect-text search (W8).

The sheet (`components/filters/FilterSheet.tsx`) edits a draft of the applied state: the count in
its footer is computed live from the draft, "show" commits it and keeps the URL's text and `view`,
"reset" clears the facets but keeps that text and view (so the count stays what "show" applies),
closing keeps what was applied. The sheet has its own history entry (architecture §7): opening
pushes one so the browser's back closes it, closing goes back, and applying replaces that entry
with the new list URL. `chips.ts` turns the applied state into the summary chips (four shown, then
"N more"); each chip removes only its own condition.

`ControlBar` holds the result count in the unit's word, the sort select, the grid density (2 or 3
columns, phones only, a preference) and the view control. `view` follows architecture §2.1: the
URL wins, else `Prefs.viewMode`; switching writes both and keeps the conditions and the anchor.
The table view fetches the effect preview of its rows on demand from the set's detail file
(`src/app/effectPreviews.ts`) renders it through `CardText` in its compact form (small icons, keyword names
without explanations) and labels it as an incomplete preview; the list view is the 52 px row
of design §6.2. `useOnline` drives the offline notice, and a card image that fails to load falls
back to the text card with a "no image" tag.

## Card page

`/cards/:cardNo/:slug?` and `/cards/_provisional/:intId` resolve through `src/domain/route.ts`:
the number (or provisional id) decides the printing, `card_route_alias` rows redirect old numbers,
`route_override` picks the printing a shared number opens as, and a missing or wrong slug is
replaced with the canonical one (`/cards/{number}/{original name}`, design decision D7). Unknown
numbers show the 404 page with a search box.

The page is an overlay only when `location.state` carries `{ background, source, pages, resultKey }`
(architecture §2.2): the list renders underneath from that background string (`CardsPage` with
`search`, `pages`, `inert`) and the bottom bar becomes previous / add to deck (disabled until M4)
/ next. `src/app/cardSequence.ts` rebuilds the sequence prev/next walks (the list's results within
the loaded pages, or the suggestions for a suggest-opened card) and finds the current card by
result key, else by card id. Without state (direct link, new tab) it is a full page with a
"back to cards" link and no prev/next.

Text: `src/domain/cardText.ts` splits `{記號}`, `{コスト2}`, `【關鍵字】` and `\n` into segments
using the snapshot's `text_symbol` spellings and `keyword` names per language (nothing hard-coded);
`components/card/CardText.tsx` draws official icons (mapped by `text_symbol.code` in
`symbolIcons.ts`) and keyword chips that explain themselves in place and link to
`/cards?mech=…`. `src/domain/textLanguage.ts` is the language matrix of snapshot-format §5: the
printing's region gives the original, the UI language picks the translation row (official >
project > community > machine, reviewed before draft), Japanese and English UIs never fall back to
Traditional Chinese, and a missing translation shows the original with a notice. Detail files load
through `src/data/detail.ts` (global texts, icons, route tables; one file per home set for the
full revisions), once per snapshot.

Real card text writes icons as `{alt}` of the official site's images; the local fixture emits
`text_symbol` rows for the common ones so the demo renders them. Keywords come from the snapshot
only, so the local demo shows `【…】` as text until the real snapshot ships keywords.

## Dead code

`bun run knip` (part of `bun run check`) reports files, exports, types and dependencies that
nothing uses. Delete what it reports together with the UI that stopped needing it; do not keep
components or helpers "for later" — they come back from git history when their consumer lands.
The only configured exceptions are in `knip.json`: the lint rule fixtures (input data, never
imported) and the font package, which only CSS `@font-face` rules reference.
`src/i18n/usage.test.ts` does the same for locale keys: every key must appear in a source file,
either as a string literal or under a template-literal prefix such as ``t(`nav.${key}`)``.

## Known issues

- **Vitest on the Bun runtime** (Bun 1.4.2, Vitest 5.0.2, jsdom 30.1.1): `bun --bun vitest run`
  cannot start jsdom test files; jsdom throws "addEventListener called on an object that is not a
  valid instance of EventTarget". Node-environment test files pass. `bun run test` uses Node and
  passes, so do not add `--bun`.
- `bun install`, `vite build` (with and without `--bun`), DOM and async Vitest tests on Node: no
  problems found.
- `eslint-plugin-jsx-a11y` 6.10.2 declares ESLint `^9` as its peer, but works with ESLint 10 in
  the fixture tests. Its community `@types` package pulls in a second ESLint, so a local
  declaration lives in `types/`.

## Not set up yet

Added once pages exist:

- Playwright (size matrix, themes, keyboard) and `@axe-core/playwright`
- knip
- First-load size budget script
- Component tests that the jsx-a11y component mapping (`CardImage`, `Link`, `Button`) matches the rendered DOM
