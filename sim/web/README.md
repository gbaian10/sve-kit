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
bun run dev                     # http://localhost:5173, synthetic fixture under /cdn
bun run dev:lan                 # HTTPS on the LAN (self-signed); plain HTTP by IP has no WebCrypto or cache
```

### Real data (4.0.0 preview export)

Screens are developed and accepted against a real `4.0.0` preview export with real WebP images,
never against the synthetic fixture. Point `SVE_PREVIEW_DIR` at the preview root (the directory
that holds `snapshots/` and `images/`); its entry is `snapshots/preview/current.json`, not the
formal version index. The dev server serves it under `/cdn-preview` and the dev badge shows
「預覽資料」 with the data version.

```bash
SVE_EXPORT_DIR= SVE_PREVIEW_DIR=/path/to/preview-root bun run dev:lan
```

In dev the app starts on the preview root on its own when a preview is configured **and** `/cdn`
is still the fixture (`src/app/dev-root.ts`); an empty `SVE_EXPORT_DIR` forces the fixture when a
shell profile sets a real export directory. With a real `/cdn` export the app starts on the formal
root and the badge switches roots by hand. Only two derived flags reach browser code
(`SVE_PREVIEW_CONFIGURED`, `SVE_CDN_IS_FIXTURE`); paths stay in the Vite server.

## Scripts

| Script                    | What it does                                                 |
| ------------------------- | ------------------------------------------------------------ |
| `bun run check`           | Everything below, in order; run it before pushing            |
| `bun run format:check`    | Prettier (`bun run format` rewrites)                         |
| `bun run lint`            | ESLint (`lint:js`) and Stylelint (`lint:css`)                |
| `bun run typecheck`       | `tsc -b` with TypeScript 6                                   |
| `bun run typecheck:local` | Pre-push only, not in `check`: `bun --check` (TypeScript-Go) |
| `bun run test`            | Vitest, including the ESLint and Stylelint rule fixtures     |
| `bun run build`           | Production build into `dist/`                                |
| `bun run shots`           | Manual: real-browser screenshots and geometry (see below)    |

`bun test` is Bun's own test runner and ignores the Vitest config. Always use `bun run test`.

The pre-commit hooks run Prettier, ESLint and Stylelint on changed files; the pre-push hook checks
types with `typecheck:local` (under a second), while CI keeps `tsc -b` as the gate. That script caps
`UV_THREADPOOL_SIZE`, libuv's pool size, which `bun --check` uses as its thread count. Write
`bun --check`, not `bun check`: the latter runs this package's `check` script. Vitest is a manual
hook (`pre-commit run --hook-stage manual web-test`); CI runs the whole `bun run check` on every web
change.

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
(`src/settings/prefs.ts`): UI language, card edition, name/effect display, symbol labels, term
emphasis, theme, accent, ban region, grid density, view mode. Fields a build no longer has (the
removed `dataSaver`) are ignored on read without a version bump. Reading validates every field on its own
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

`src/data/format-v3/` is the pure core of the card-data reader: no `fetch`, no DOM. It follows
[docs/schema/export/snapshot-format.md](../../docs/schema/export/snapshot-format.md),
[snapshot-transport.md](../../docs/schema/export/snapshot-transport.md) and
[snapshot-contract.md](../../docs/schema/export/snapshot-contract.md), and decodes the format validated offline by the Python reader in `carddb/src/sve_carddb/export/`.

- `json.ts`: native JSON parsing of published snapshots, plus canonical encoding for lookup keys
  and synthetic fixtures. Format checks run offline in carddb and before publication.
- `integrity.ts`: WebCrypto SHA-256 for downloaded and cached bytes; the decoder does not hash again.
- `sha256.ts`: synchronous `sha256-mod-v1` lookup buckets and fixture production.
- `schema.ts`: fixed column/type accessors from carddb's published schema; no runtime schema validator.
- `decode.ts`, `reader.ts`: version admission, nested tuple decoding and detail joins. Parsing and
  required-field errors become a failed load; no canonical, placement or cross-table validation runs.

The client supports only snapshot 4.0.0, including its version and capabilities. Index v2
considers only current, previous and the local active snapshot. Constant-cost index checks still
reject a revision that goes back or changes content within a session, a current/previous pair out
of order and an entry that differs from its manifest. An older usable snapshot has
a visible notice, and having no compatible snapshot prompts an application update.
Reloads first read a digest-checked selection record and its verified bootstrap record from
CacheStorage, isolated by root, build identity and record format. Local data becomes `ready`
with `unconfirmed: true` without waiting for the mutable index response; `load()` resolves at
that point. The background check preserves the revision high-water mark and six manifest-field
comparisons. An unchanged manifest keeps the catalog and detail caches; a different manifest
replaces them only after verification. Offline or failed checks keep local data with an
`outdated` notice and a classified `updateError`; successful checks clear `unconfirmed`.
The selection records distinguish permanent indexes from preview pointers. Missing, damaged
or unavailable storage uses the usual index-first load. `reload()` waits for an active
background check, so callers can explicitly await confirmation. A reload requested while local
records are being restored starts a fresh check after restoration and waits for its result.
Unconfirmed local data whose check fails with a network error while the browser is online gets a
notice that updates could not be checked; offline keeps the offline notice. Incompatible and
corrupt errors keep their existing notices.

The search Worker's persistent index still resolves its own pointer before restoring. It is
not currently connected to the cards-page list, which uses the main-thread catalog; giving
that Worker the same confirmation lifecycle requires extending its generation/status protocol.

Downloaded bytes are hashed before transfer to the decode Worker; an idle worker releases
its parsing heap after one second. Tests use the same decoder without a Worker. Raw tuples
are released after decoding, rather than kept alongside row objects.

Image metadata uses the manifest's exact fragment locator: permanent printing owner and
printing bucket select a home-set media file. The viewport hook prefetches half a viewport
height above and one and a half below, using pixel margins rebuilt on resize. Faces nearest
the viewport go first: at most 60 candidates, 12 faces per batch and two concurrent batches
shared by viewers of the same snapshot. Started batches finish across navigation; an unmounted
viewer cannot schedule more. Only on-screen images are eager; prefetched images remain lazy.

Decoded face metadata and URLs accumulate in the data layer for the lifetime of a snapshot.
Scrolling or remounting a viewer can read them synchronously, without a loading placeholder or
another fragment read. Replacement snapshots start with an empty index, and a late old batch
cannot populate the new one. Errors belong only to the affected faces. A failed face can retry
when its slot re-enters the near range at least ten seconds after failure; remaining in range
or receiving the initial callbacks of a resized observer does not trigger another attempt.
Each batch has a thirty-second deadline: expiry marks its faces failed, releases its shared
slot and cancels acceptance of late results. Started batches retain this deadline across
viewer unmounts.

Retention admits only printing/face pairs whose bootstrap rows belong to the same card,
including known missing-media results. Its entry limit is therefore the finite number of
such pairs in the active snapshot (roughly fifteen thousand), rather than arbitrary queried
IDs. Each entry holds one media row and one set of URL/size values; it retains no image bytes
or whole media fragments. Scheduling sets have the same identity bound. WeakMap ownership
allows an old snapshot and its metadata to be collected once its started batches finish.
The separate metadata byte cache retains its existing 12 MiB / 64-file RAM limit.
Browsing does not need the on-demand `rules_name` or `face_rules_name` tables; any future
construction consumer must load their detail files before claiming a result.

In 4.0, home-set media files provide display state, dimensions and separate card/art version
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
blobs, only when persistent storage is available. Storage loss
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
have (version index, manifest, canonical blobs, WebP placeholders). It exists for Vitest and for
a dev server without real data; screens are never judged on it. `bun run fixture:build`
regenerates it from `scripts/fixture/cards.ts` (about two dozen handwritten cards covering
double faces, alternate printings, errata, Q&A, bans, the JP/EN mapping states and image
states) through `scripts/fixture/build.ts`, which emits snapshot 4.0.0 with index format 2, 64 buckets,
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
writes the anchor first, then pushes `/cards/:cardNo` with `{ background, source, pages, resultKey, route }`; `route` carries the printing and card IDs, each owner, and the region.
`src/app/listEntryState.test.tsx` pins that replace-then-push keeps both states in the data router.

Each grid cell shows the name with the card number at the right of the same line (user request
2026-09-29), then the translation line when the name display asks for one. The `view` URL
parameter (else the `viewMode` preference) picks the grid, the table rows (48×67 thumbnail, stats,
a two-line effect preview labelled as incomplete) or the list rows (52px, class-coloured edge);
the control row holds the result count, a sort placeholder and the view switch.

List views become usable after the selected region's whole catalog is verified and indexed in
the Worker. `useListView` publishes names and numbers independently of image metadata;
`useVisibleResultImages` requests metadata for slots reported visible by IntersectionObserver.
Only those slots get image URLs, and the browser loads their picture files lazily. Image failures
leave searchable, clickable cards in place. The card page retains the bounded `useImageIndex`
queue (12 faces per batch, two batches in flight, at most 60 faces per pass).

`CardImage` is one fixed 63:88 slot: the app's own placeholder mark (`CardPlaceholder`, not the
official card back) sits underneath and the image fades in over it. Loading, missing, pending,
unfetched and failed images all keep the placeholder with their own label; the name and number
stay visible next to the slot. A landscape image is shown whole (`object-contain`) rather than
cropped. Callers set the width (`w-full`, `w-8`).

### Readiness and search hooks

`src/app/snapshot.ts` defines what pages call: `useSearchReadiness(edition)` (phase
`downloading | ready | error`, generation, root, edition, the loading step, data version,
manifest hash, `retry`) and `useSearch(query)` (the last input and a result that is only
`pending`, `error` or `complete` with a page). The shapes follow the Worker index's
`SearchSession` status and `SearchChannel` result, so the call sites stay when the data layer
swaps the implementation in. The Worker builds compact indexes from the regional catalog and
retains complete answers while a replacement snapshot downloads. It discards late responses from
older generations and query IDs. Inputs stay editable during download; results, suggestions and
counts wait for the complete selected region.

Rarity and alternate-art queries wait for the regional printing closure. `q=effect:…` requests an
effect search over every regional detail/history file and its dictionary dependencies; progress is
reported while the closure loads, and the Worker returns only the complete answer. The effect
search includes supplied original and translated effects and readable wording candidates.
Unsupported mechanics and release-date sorting produce explicit `unsupported-query` errors.
`StatusStrip` renders download, update and retry states. Image readiness never gates search.

List and set URLs default to JP; `?ed=en` selects EN before the first request. UI language stays
out of the URL. `/sets/:publicCode` resolves the public family code and applies its archive code as
the default set filter. Normal card links preserve raw card numbers; provisional links use the
permanent integer. The route reader checks permanent aliases and variant overrides and replaces
missing or incorrect slugs with the original name. Cold links reuse catalog locator triples;
provisional lookup reads authoritative printing integers. No additional card-number wire is
produced. An EN spelling is only a first-download hint; exact rows determine the region, with a
second regional catalog required when the first cannot resolve an external key.

The class quick bar shows labels only when an off-screen labelled copy fits the row width: the six
classes of the physical game plus neutral. A class code the design does not know yet would use the
neutral colour and an initial.

## Dead code

`bun run knip` (part of `bun run check`) reports files, exports, types and dependencies that
nothing uses. Delete what it reports together with the UI that stopped needing it; do not keep
components or helpers "for later" — they come back from git history when their consumer lands.
The only configured exceptions are in `knip.json`: the lint rule fixtures (input data, never
imported) and the font package, which only CSS `@font-face` rules reference. The dependency-only
PR that added Playwright listed it there too; the screenshot script now imports it, so that entry
is gone.
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

## Screenshots and first-screen geometry

`bun run shots` (`scripts/screenshots/main.ts`) drives a running dev server in headless Chromium
through Playwright. It is a manual dev tool: not part of `bun run check`, never run in CI, never
given synthetic data. It captures the card list and the given card pages for every theme ×
UI language × width (dark/light × zh-TW/ja/en × 390/820/1180/1440 by default, filterable) and
writes `report.json` next to the PNGs with the data version, manifest hash, browser, and per shot
the viewport y of the first effect-text line (design 06: ≈300 at 390), horizontal page overflow,
the overflow audit and the console error count. Card numbers and geometry only; no card text.
`--home` adds the home page (with `--recent <printing id>` stored as recently viewed and
`--home-query` typed into its search for a second shot); `--edition en` switches the card edition.

```bash
bun run shots -- --base http://localhost:5173 --out /tmp/shots --query bp01 \
  --card ECP02-SP11 --card BP09-070 --themes dark --langs zh-TW --widths 390
```

Run `bunx playwright install chromium` once to download the browser. Linux screenshots do not
stand in for the maintainer's phone: CJK font size and wrapping are judged on the device.

## Not set up yet

Added once pages exist:

- `@axe-core/playwright`
- First-load size budget script
- Component tests that the jsx-a11y component mapping (`CardImage`, `Link`, `Button`) matches the rendered DOM

Production builds include `third-party-licenses.md` from Vite's dependency license reporting.
The dependency report includes lru-cache's BlueOak-1.0.0 terms and the MIT notices for p-queue,
p-timeout and eventemitter3; these third-party terms are not replaced by the project license.
