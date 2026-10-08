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

- `json.ts`: the strict JSON boundary (canonical-json-v1). Microsoft jsonc-parser visits decoded keys and tokens
  with comments and trailing commas disabled; every parser error is rejected. The visitor rejects
  duplicate keys, floating spellings, unsafe integers and lone surrogates before native `JSON.parse`
  builds the value (including own `__proto__` keys and canonical negative zero); `canonical()` writes the exact bytes hashes refer to.
- `sha256.ts`: synchronous SHA-256 and the `sha256-mod-v1` bucket function.
- `schema.ts` + `validator.ts`: the published JSON Schema is loaded from `carddb/` at build time
  (one source of truth, no copy). `scripts/schema/` compiles Ajv2020 standalone validators;
  runtime dispatch never compiles schemas or uses `new Function`, including in the decode Worker.
  Annotation keywords `x-columns`, `x-types`, `x-primary-key`, `x-fragments` and `x-tables` are
  registered explicitly; any other unknown keyword fails the build. Only the seven reader
  entry definitions are bundled; the conformance build also checks every named definition.
  `schema.test.ts` runs shared positive and negative examples through standalone and live Ajv,
  which must agree. Schema failures keep their `schema` code, while Ajv supplies the path and keyword.
- `decode.ts`, `reader.ts`, `semantics.ts`: fixed accessors from `x-columns`/`x-types`,
  manifest and container checks, `row_index`/`face_ordinal` joins, reference closure and the
  cross-row rules JSON Schema cannot express. Every rejection is a `SnapshotError` with a fixed
  `code`; tests match codes, never message text.

`v2-contract.test.ts` runs the shared schema, reader, index and image-URL vectors from
`tests/fixtures/snapshot-contract/v2/`. The current golden manifest and payloads join into
exactly `expected-logical.json`; text-all gives the same view. `contract.test.ts` covers
additional transport errors (missing files, cycles and integrity failures), and
`schema.test.ts` checks synthetic positive samples against standalone and live Ajv.

The client supports only snapshot 2.0.0, including its capabilities and fixed band membership.
The `format-v1/` directory name is historical; its strict JSON, Ajv, decoding and semantic
checks are shared current-format logic, with no 1.0 or 1.1 reader or schema branches.
Index v2 considers only current, previous and the local active snapshot; an incompatible
current does not trigger a search through historical versions. An older usable snapshot has
a visible notice, and having no compatible snapshot prompts an application update.
It verifies and decodes in one Worker; an idle worker releases its parsing heap after one
second. Node fixture scripts and tests use the same decoder without a Worker. Raw tuples
are released after decoding, rather than kept alongside row objects.

Image metadata uses the manifest's exact fragment locator: permanent printing owner and
printing bucket select a home-set media file. At most 24 intersecting image slots own rows
at once; other slots remain text cards until visible. Decoded files are discarded, retaining
up to 64 recently used face results. Overlapping visible faces keep their images while a
debounced viewport change resolves new faces. Browsing does not need the on-demand
`rules_name` or `face_rules_name` tables; any future construction consumer must load their
detail files before claiming a result.

In 2.0, home-set media files provide display state, dimensions and separate card/art version
tokens directly. Image URLs use the permanent printing integer ID and face ordinal, with
the token in `?v=`; source-detail tables are optional on-demand data. Printing/face bootstrap
lookups are indexed once per snapshot. Withdrawn, missing or unapproved media produce no image
URL, and another printing is never used to fill that gap.

The production image Service Worker matches the complete URL, including `v`, and stores only
successful non-opaque responses in CacheStorage. The response cache has a 128-entry FIFO limit
across roots and versions; a hit does not refresh its insertion order. Opaque responses are
never stored, and an opaque entry from an earlier worker is discarded when requested. Each
opaque request calls `fetch`; the browser's HTTP cache can still serve the versioned URL.
Cross-origin images currently use that HTTP cache, avoiding opaque CacheStorage entries whose
quota accounting can be much larger than their bodies. Failures never fall back to an old `v`.

Using `crossOrigin="anonymous"` together with CDN CORS headers is a follow-up: the CDN must
first serve the required headers, otherwise adding the attribute would break image loading.
The application currently leaves the attribute unset. A browser that cannot register the
module Service Worker continues through normal HTTP loading; CacheStorage is optional.

After the catalog is ready, background work downloads **metadata bytes**, not all image
blobs, only when persistent storage is available. Data saver delays that work. Storage loss
stops queued background work; without persistent storage only visible faces are fetched.
When background work completes, its session cost includes every file containing `printing_image` rows.
The progress surface offers pause/retry and distinguishes download completion from persistent
cache availability; neither means all images or offline text are ready. Snapshot replacement
aborts old metadata requests and isolates CacheStorage by manifest hash and file content path.
After successful adoption, old metadata namespaces are pruned, retaining the active and
immediately previous manifest only; unrelated application caches are preserved.

Visible work takes priority, with p-queue limiting metadata work to four requests including
bodies and a second queue admitting no more than three background requests. Queued work
can be promoted without duplicating its shared promise; cancellation removes queued background
work while running requests finish, and snapshot replacement aborts the active transport. Verified persistent bytes are rechecked before decoding; returning
to an evicted face can reparse without another external request; recently used faces do not
need re-decoding. CacheStorage failures visibly degrade
to a 12 MiB / 64-file RAM byte LRU provided by lru-cache. Oversized files are not retained;
empty values count as one byte for the library's size accounting. The page parsing workset is capped at 12 MiB raw;
this is not a heap measurement or a physical-phone acceptance claim.

## Development snapshot

`fixtures/snapshot/` is a small synthetic card-data snapshot in the exact layout the CDN will
have (version index, manifest, canonical blobs, WebP placeholders). `bun run fixture:build`
regenerates it from `scripts/fixture/cards.ts` (about two dozen handwritten cards covering
double faces, alternate printings, errata, Q&A, bans, the JP/EN mapping states and image
states) through `scripts/fixture/build.ts`, which emits snapshot 2.0 with index format 2, 64 buckets,
fixed bands, five display sizes and permanent-ID image URLs, emits the column partitions, sorts every
collection the way the reader requires and hashes every blob. The output is deterministic and
the reader must accept it (`scripts/fixture/build.test.ts`), so a change to either the reader or
the generator that breaks the contract fails the tests, not the pages.

`bun run dev` and `bun run preview` serve a snapshot root under `/cdn`: `SVE_EXPORT_DIR` when set
(for a real local export), else the fixture. `SVE_PREVIEW_DIR` is served under `/cdn-preview`.
Unset or empty `SVE_EXPORT_DIR` uses the fixture; unset or empty `SVE_PREVIEW_DIR`
leaves `/cdn-preview` unconfigured. Non-empty roots must be absolute paths. Local
paths stay in the Vite server configuration; only the derived preview flag reaches browser code.
The files are canonical JSON and are excluded from Prettier.

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
underneath and the image fades in over it; missing and pending images keep the text card with a
reason, and data saver waits for a tap. Callers set the width (`w-full`, `w-8`).

The class quick bar shows labels only when an off-screen labelled copy fits the row width: the six
classes of the physical game plus neutral. A class code the design does not know yet would use the
neutral colour and an initial.

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

The package scripts compile schemas before dev, build, preview, typecheck, test, knip and fixture
commands. Generated JavaScript lives only in `node_modules/.cache/sve-schema/`. Fixed Vite/TypeScript
aliases use the committed `compiled-validators.d.ts`, so clean-checkout lint and type checking
do not need generated files. Run `bun run schema:compile` before invoking Vite, Vitest or a
fixture script directly.
Generation uses Bun, Node APIs and Vite, without shell utilities or OS-specific dependencies.

Production builds include `third-party-licenses.md` from Vite's dependency license reporting,
and `snapshot-validator-LICENSE.md` for Ajv and its helpers already bundled into generated code.
The dependency report includes lru-cache's BlueOak-1.0.0 terms and the MIT notices for p-queue,
p-timeout and eventemitter3; these third-party terms are not replaced by the project license.
