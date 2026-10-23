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
bun run dev:lan                 # HTTPS on the LAN (self-signed by default); plain HTTP by IP has no WebCrypto or cache
```

Set `SVE_DEV_TLS_CERT` and `SVE_DEV_TLS_KEY` to absolute paths to use a certificate issued by a locally trusted CA, such as a mkcert certificate for a LAN IP. For example: `SVE_DEV_TLS_CERT=/path/to/cert.pem SVE_DEV_TLS_KEY=/path/to/key.pem bun run dev:lan`.

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

| Script                 | What it does                                              |
| ---------------------- | --------------------------------------------------------- |
| `bun run check:all`    | Everything below, in order; run it before pushing         |
| `bun run format:check` | `bun fmt --check` (`bun run format` rewrites)             |
| `bun run lint:all`     | ESLint (`lint:js`) and Stylelint (`lint:css`)             |
| `bun run typecheck`    | `bun check --threads=4`, Bun's built-in type checker      |
| `bun run test:all`     | Vitest, including the ESLint and Stylelint rule fixtures  |
| `bun run build:all`    | Production build into `dist/`                             |
| `bun run shots`        | Manual: real-browser screenshots and geometry (see below) |

Scripts never take the name of a Bun built-in command (`test`, `build`, `check`, `lint`, `fmt`):
`bun test` runs Bun's own test runner, which ignores the Vitest config, and a `check` script would
replace the type checker behind `bun check`. A script that runs a whole group is `<name>:all`.

ESLint keeps a content-based cache in `node_modules/.cache/eslint/`, so a repeated run only lints
changed files. Type-aware results of an unchanged file can go stale when another file's types change;
delete the cache if an error looks unrelated to your change. CI starts without it.

The pre-commit hooks run bun fmt and Stylelint on changed files. bun fmt reads the root
`bunfig.toml`; `.prettierrc.mjs` only serves the editor's Prettier extension, so change both
together. The pre-push hook runs ESLint over all of sim/web, as CI does: type-aware rules load the
whole TypeScript project however few files they check. It also runs the `typecheck` script that CI
shares, which uses `--threads=4` to limit the thread count for `bun check`. Vitest is a
manual hook (`pre-commit run --hook-stage manual web-test`); CI runs the steps of `check:all` on
every web change.

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

Adopted snapshots retain two complete manifest hash-reference plans in CacheStorage.
The plans include every file in each manifest, regardless of the search or download subset,
and the manifest hash itself. GC removes unreferenced raw bytes first. Necessary raw and
derived records for those two manifests survive capacity and quota trimming; a batch that
cannot fit reports partial completion. A failed adoption does not replace the retained plans.
Plan bytes count toward the shared data budget and are removed by clearing automatic caches.
Cross-tab collection has no lock: readers verify cache hits and recover removed records as
ordinary misses. Retention cannot promise atomic offline availability across tabs or browser eviction.

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
Service Worker continues through normal HTTP loading; CacheStorage is optional. The bundled
image worker uses classic registration so its script request includes the authentication cookie
in Chromium.

After the catalog is ready, background work downloads **metadata bytes**, not all image
blobs, only when persistent storage is available. Storage loss
stops queued background work; without persistent storage only visible faces are fetched.
When background work completes, its session cost includes every file containing `printing_image` rows.
The progress surface offers pause/retry and distinguishes download completion from persistent
cache availability; neither means all images or offline text are ready. Snapshot replacement
aborts old metadata requests. Bytes live in the shared raw cache keyed by root and content hash,
so manifests reuse one copy. Startup removes only this root's legacy per-manifest metadata
namespaces; unrelated application caches and other roots are preserved.

Visible work takes priority, with p-queue limiting metadata work to four requests including
bodies and a second queue admitting no more than three background requests. Queued work
can be promoted without duplicating its shared promise; cancellation removes queued background
work while running requests finish, and snapshot replacement aborts the active transport. Verified persistent bytes are rechecked before decoding; returning
to an evicted face can reparse without another external request; recently used faces do not
need re-decoding. CacheStorage failures visibly degrade
to on-demand verified network reads; no settled bytes are kept in RAM. The page parsing workset is capped at 12 MiB raw;
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
The files are canonical JSON and are excluded from formatting.

## Search and the card list

`/cards` is described by its URL alone (`docs/sim/web-architecture.md` §2.1): `src/domain/query/`
holds the state, the codec (`q class cost atk def type mech set rarity alt unit sort order view`; defaults are
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
a two-line effect preview) or the list rows (52px, class-coloured edge);
the result count sits beside the search bar's filter button; the control row holds the sort
control and the view switch.

Type and deck-location labels use SVG outlines traced from official card images,
with glyph paths derived from OFL fonts and no embedded bitmap data. Their sources and exclusions are
in [the card-label README](src/assets/card-labels/README.md). The list places Deck
and Type after Rarity; both columns can be hidden or reordered. Cards retain every
matching badge in location-filter order (evolve, advance, leader, Token).
The existing search counts and selections still use one primary location per printing.

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

`bun run knip` (part of `bun run check:all`) reports files, exports, types and dependencies that
nothing uses. Delete what it reports together with the UI that stopped needing it; do not keep
components or helpers "for later" — they come back from git history when their consumer lands.
The only configured exceptions are in `knip.jsonc`: the lint rule fixtures (input data, never
imported) and the font package, which only CSS `@font-face` rules reference. The dependency-only
PR that added Playwright listed it there too; the screenshot script now imports it, so that entry
is gone.
`src/i18n/usage.test.ts` does the same for locale keys: every key must appear in a source file,
either as a string literal or under a template-literal prefix such as ``t(`nav.${key}`)``.

## Known issues

- **Vitest on the Bun runtime** (Bun 1.4.2, Vitest 5.0.2, jsdom 30.1.1): `bun --bun vitest run`
  cannot start jsdom test files; jsdom throws "addEventListener called on an object that is not a
  valid instance of EventTarget". Node-environment test files pass. `bun run test:all` uses Node and
  passes, so do not add `--bun`.
- `bun install`, `vite build` (with and without `--bun`), DOM and async Vitest tests on Node: no
  problems found.
- `eslint-plugin-jsx-a11y` 6.10.2 declares ESLint `^9` as its peer, but works with ESLint 10 in
  the fixture tests. Its community `@types` package pulls in a second ESLint, so a local
  declaration lives in `types/`.

## Screenshots and first-screen geometry

`bun run shots` (`scripts/screenshots/main.ts`) drives a running dev server in headless Chromium
through Playwright. It is a manual dev tool: not part of `bun run check:all`, never run in CI, never
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

### Card drawer geometry

`bun run check:drawer -- --base https://127.0.0.1:5793 [--out /tmp/drawer]` is a manual tool like
`shots`: run by hand against a running dev server with real data, not part of `bun run check:all` and
never run in CI, so a local pass is not a CI guarantee. It needs real data with several printings
of the first card (the printing case) and at least two pages of results (the second-page case). It
checks what jsdom cannot: the card drawer pushes the frame (main content and the avatar end where
the drawer starts, frame padding equals the drawer width, no horizontal overflow), a saved width
wider than a narrower window is cut to what the window leaves and falls back to the modal below
1000px, choosing another printing keeps the result cell and previous/next, a sort change keeps the
card while back closes it and back again restores the old list, and a card opened from the second
page (after the drawer was opened and closed once) returns to the same viewport position with
focus on its cell. It exits non-zero on any failed check and writes `drawer-check.json` when
`--out` is given.

## Not set up yet

Added once pages exist:

- `@axe-core/playwright`
- First-load size budget script
- Component tests that the jsx-a11y component mapping (`CardImage`, `Link`, `Button`) matches the rendered DOM

Production builds include `third-party-licenses.md` from Vite's dependency license reporting.
The dependency report includes the MIT notices for p-queue,
p-timeout and eventemitter3; these third-party terms are not replaced by the project license.

## Catalog performance benchmark

`bun run bench:catalog` is a manual development tool for
[#670](https://github.com/gbaian10/sve-kit/issues/670). Let the machine become idle
before collecting a baseline. Keep the browser version, source revision, CPU
quota, input manifests, sampling interval and settling interval identical when
comparing changes. This is not a required CI check.

Install dependencies with `bun install` and the matching browser with
`bunx playwright install chromium`. Linux with a working systemd user manager
and cgroup CPU/memory controllers is required. The tool starts its entire process
tree (build, preview, controller and Chromium including Workers) in a systemd
user scope with `CPUQuota=200%`, `MemoryMax=3G` and process nice level 9. These are
resource caps, not a mobile CPU simulation; competing processes still affect
results. A systemd failure stops the run rather than silently running uncapped.
Use `--cpu-quota` and `--memory-max` to change these settings.

Pass two **public export roots**, each containing
`snapshots/versions/index.json` and its referenced manifests and blobs. For
preview exports with `snapshots/preview/current.json`, add `--entry preview`
to use the existing preview reader on both sides. All path
arguments must be literal absolute paths; use a report file outside the source
and export roots. The old and new manifests must differ for update scenarios.
The tool reads inputs without modifying them and builds into a temporary
directory. Preview binds only `127.0.0.1:5791` and fails if the port is occupied;
it closes its own server and browser on completion or failure.

```bash
bun run bench:catalog \
  --old /absolute/old-public \
  --new /absolute/new-public \
  --out /absolute/reports/catalog-before.json
```

The default scenarios are `cold-jp,cold-en,update-jp,update-en`, each repeated
three times. Use `--runs 1`, or individual `--cold-jp-runs`, `--cold-en-runs`,
`--update-jp-runs`, `--update-en-runs` overrides. `--scenarios cold-jp,update-jp`
selects a subset. Run `bun run bench:catalog --help` for all options.

This builds a dedicated HTML entry with the same Vite production configuration
and production data modules, then runs Vite **preview**, never a dev server. The
entry concurrently drives the existing main `createSnapshotClient` and search
`SearchSession` loaders, as the card detail route does. It does not mount React,
render cards, load details or load image bitmaps. It isolates catalog costs; UI
rendering and device acceptance require separate measurements. The benchmark
entry and controller are not included in the normal application build.

Each repetition uses a fresh browser context, empty CacheStorage and no HTTP
cache (Playwright routing disables it). The Chromium process, preview server,
OS file cache and server compression cache are shared across repetitions; this
is a browser-cold load, not a machine-cold or CDN/network test. Cold loads use
the new snapshot. Updates first load the old snapshot, wait for the search
Worker's printing supplement, then change the served snapshot at the **same
origin and base path** and call `client.reload()` and `session.load()` on the
existing instances. Both old active catalogs stay alive during replacement.
Both loaders must adopt the same new manifest. Each run waits for the background
printing index and another `--settle-ms 2000` before taking its final sample.
A load or background completion timeout is three minutes; failures do not count
as successful samples.

For a quick tool check without real card data:

```bash
bun run bench:catalog --smoke --runs 1 --out /absolute/reports/catalog-smoke.json
```

Smoke mode copies the committed synthetic fixture into temporary roots and
changes only the new manifest's data version and pointer revision. Its update
exercises replacement with cached unchanged payloads. It checks tool wiring and
report shape; its values cannot be extrapolated to a full catalog. Use actual
different releases for a baseline.

### Measurements and interpretation

Permanent User Timing measures use the `sve:catalog:` prefix. `decode:manifest`
and `decode:file` include JSON.parse and reader validation after verified bytes
are available. The main loader decodes in its own dedicated decoder Worker;
the search Worker decodes its own copy. `ingest`, `seal`, `SearchIndex`, main
`validate` and main `catalogOf` measure synchronous work. `catalogOf` records
only cache misses. `background:*` measures printing supplements separately.
Updates reuse the same stage names in a separate report window. Marks and
measures are cleared immediately after observation is queued, so normal use
does not accumulate timeline entries. Record a DevTools Performance trace to
inspect them; `getEntriesByType('measure')` after loading will not retain them.

The report separates main, decoder Worker and search Worker stage totals.
Raw entries also retain each Worker identity, start time and duration. The
benchmark build prepends an observer to each production Worker bundle, queues
early entries until CDP attaches, and then streams entries to the controller.
This retains first-manifest timings and measurements from Workers that later
exit. The observer prelude is absent from normal application builds. Main Long Tasks
strictly greater than 50 ms are recorded throughout loading and the settling
window, including background work. Their count and per-run longest duration
are in JSON; the table shows the worst task across all repetitions so one slow
run cannot disappear behind a median. This observes the dedicated entry's main
thread, not React rendering or a whole-application performance trace.

The table references the
[mobile parsing and memory budgets](../../docs/schema/export/size-budget.md#手機記憶體與解析):
1000 ms cumulative basic work, 50 ms main tasks, 80 MiB **update** peak and
48 MiB steady state. `OVER` marks observed exceedances. Other numeric columns
are medians of per-run totals/maxima, rather than medians of individual fragments.
Background work remains visible but is excluded from basic work totals. These
stage totals omit uninstrumented orchestration, hashing, browser decompression
and structured cloning; they are an effective-work lower bound, not a full
acceptance result. Wall time includes both loaders reaching basic readiness,
fetch/cache operations and waits; it is not CPU time. Resource Timing durations
and transferred/decoded sizes are also saved (durations may overlap; do not
subtract their sum from wall time to infer scheduling wait).

Memory is sampled every `--sample-ms 50` via
[CDP Runtime.getHeapUsage](https://chromedevtools.github.io/devtools-protocol/tot/Runtime/#method-getHeapUsage):
`usedSize + backingStorageSize` for the page and **all dedicated Workers**.
Backing storage includes ArrayBuffers and external strings; TypedArray backing
buffers are not added again. Each round sums current isolate samples, not each
isolate's independently observed maximum. JSON retains per-isolate values,
round start times, round durations (`spanMs`) and completeness. Rounds interrupted
by Worker creation/exit or failed replies are marked incomplete and excluded
from summaries. Update samples include the old active data before replacement.

CDP replies across isolates are not atomic and can be delayed by busy JS, so
short peaks may be missed and sampling skew can overstate a concurrent sum.
Check `maxSampleSpanMs` and raw rounds before interpreting a peak. No forced GC
is run. Gross memory includes the small harness and observer overhead, without
subtracting an app estimate; the final sample is not proof of GC-stable steady
state. The proxy excludes browser native/decompression/process memory and does
not replace device measurement or exact simultaneous peak analysis. The `*`
marker means only that the observed proxy is within a referenced limit; it is
not a mobile budget pass. CPU quota, observer/CDP overhead and background work
can also affect wall time.

The tool writes the selected `--out` JSON file after each completed repetition,
then a sibling `.txt` table after all scenarios. The JSON contains conditions,
Git revision/dirty status, reader build ID, browser version, old/new manifest
hashes, per-scenario medians and raw measures/tasks/memory rounds. An interrupted
run may leave a partial JSON; inspect scenario counts before comparison. Output
contains metadata and performance values, not card wording. Keep reports outside
the repository. Compare matching `summaries` fields between before/after JSON,
and retain raw samples to investigate outliers.
