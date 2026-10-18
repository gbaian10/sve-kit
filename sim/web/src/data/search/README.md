# Worker search and complete list views

`SearchSession` owns one Worker generation. Each `createChannel()` has its own last input,
query ID and result. Channels support `search`, `view`, `suggest`, a batch of at most 20
recent printing summaries, set `releases` and `count`. Replies carry generation, channel ID
and query ID; old replies cannot replace current results. Suggestions preserve exact printing
identity and name/alias ranking. Missing recent IDs explicitly return `not-in-edition`.

Search supports card/printing units, text, classes, printing home-set family codes and the cost, attack
and defense value selections (the last button collects every value at or above it), printed types,
deck locations (`main`, `evolve`, `advance`, `leader[:frame]`, `token[:type]`, `ep`, `sep`; special
kinds and traits come from the front face's revision shown in the edition, the leader frame from the
printing; every printing has exactly one parent location, combined special kinds resolving as Token,
leader, EP, SEP, evolve, advance, then main) and traits. Values within one row union; rows intersect. Mechanics and release-date sorting
return `unsupported-query`.
The list shows only cards released in the selected edition. Names remain multilingual, and
UI language is independent of card edition. Card detail retains its on-demand compatibility
path; only the card page starts the full snapshot client.

With no search text, card pages order the existing representative printing's card number;
printing pages order each printing's own card number, without choosing a representative.
Its width/case-folded key compares set-code digit runs numerically, then the letter prefix
(empty first), integer number, and suffix (empty first, edition marker ignored). Leading
zeros do not affect comparison. Ties use permanent card ID, then printing ID. Unknown
number forms follow recognized keys and compare folded literal text. Text queries retain
the existing relevance ranking. This does not change representative-printing selection.
A persisted scalar rank avoids decoding summaries or re-sorting all printings per query.
Prefixes sort empty, LD, P, PR, SL in ASCII order within the same set. For a set view, pass
`state: { ...DEFAULT_QUERY, unit: "printing", sort: "no", sets: [familyCode] }` to
`channel.view` or `useListView`. Each item key is its printing ID; `summary.cardId`
remains the card identity, and every face/image source belongs to that printing.
Totals and paging count printings, and query/cache identity distinguishes units.
The cards page defaults to printing units and card-number sorting in all three views.
Home random-card queries and suggestions explicitly retain card units; their totals count cards.
No new index layout or full catalog on the main thread is needed.

The Worker retains scalar index columns, a shared string pool, multilingual class/set
options and image targets (printing ID, face ID, permanent integer ID, ordinal and owner).
Only bounded pages and purpose-specific summaries return to the main thread. Set options
come from product families and use natural family-code ordering, then permanent ID.
Set releases list each family with its first day-precision product date in the edition; a family
without a linked, dated product is absent, never dated by its code. `count` returns only how many
cards match (each card once, whatever unit) and how many have a name translation in one
language, low confidence counted and reported separately.

A complete view contains its full query, offset, limit, total, ordered summaries, options,
and every printing face's verified publication/availability state, card version, variants
and image sources computed through `mediaImageSource`. A missing descriptor, hash failure,
or missing available-image variants is an error, never a cached unavailable image.
View hits read no printing-image metadata. Index-only hits complete the needed metadata
through the same app request queue.

Startup reads the verified active selection and matching view before awaiting the network
entry. It then restores the index and checks the entry in the background. `searchReady`
is separate from view availability. Failed checks preserve usable local data and expose
its unconfirmed/stale status. Index entries still require capability admission, manifest
field agreement and monotonic revision/content identity. Bodies are written before the
active selection; missing or damaged bodies fall back to verified raw reconstruction.
Switching to another edition takes the same stored path. A later check that finds the
active version unchanged carries the index over to the new generation without rebuilding;
completed channel answers stay until the manifest, root or edition they came from changes.

The initial search index uses bootstrap locators, so initial results do not wait for background
printing downloads. Completion keeps the same index and its name, alias and card columns.
It permutes packed summary/image-target cells into the completed printing order and updates
printing metadata, scalar filters, representative printings, ranks, options and releases.
Completion checks printing counts and locator ownership for cards already in the index.
It relies on the locator contract to cover the edition's full membership.
Both stages can be persisted and restored with the same layout.

Cache identity includes root, manifest hash, reader build and local format. View slots also
include the normalizer version, selected-region policy and complete query. Multilingual
names avoid a separate language-specific record. Index format is 4; view format is 2;
the public snapshot wire format is unchanged. Automatic raw/index/view/metadata caches
share 64 MiB per root and 128 MiB per origin. Views have a 2 MiB raw-body subquota; an
oversized view stays usable in memory. Successful raw plans and verified records retain
at most two generations. Quota failure gets one cleanup/retry, then memory fallback.
CacheStorage has no multi-record transaction or cross-tab lock: eviction may cause a miss,
which requires body verification and reconstruction rather than mixing generations.

Client, Worker and metadata requests share one main-thread queue. It deduplicates body
reads, cancels subscribers independently, promotes queued visible work, and holds permits
until the response body finishes. Total concurrency is at most 32, background at most 24.
Config is independent of bootstrap progress; each verified bootstrap key contributes once
(the fixed real-data batch is 84). Bootstrap completion does not imply a complete view.

React adapters in `app/snapshot.ts` expose `useSearchReadiness`, `useSearch`, `useSearchView`,
`useListView`, `useWorkerSuggestions`, `useRecentSummaries`, `useSetOptions`, `useSetReleases`
and `useSetCounts`.
`useListView` requests bounded 60-card pages and returns a complete image adapter once every
requested page has arrived. Loading more only requests the new page, and the pages on screen
stay until a complete set replaces them. Pending and unsupported results remain explicit.
`useCardSnapshot` is the temporary full-detail path for the card page.

V1 list views share the completed bounded page and image adapter. Art thumbnails retain the
verified `art_version` in view format 2. View selection never changes search identity. Card-page
neighbours return at most three summaries; an originating list caps the order at its loaded
pages. The full snapshot and annotated resolver remain card-page compatibility APIs, including
historical wording candidates.

Near-viewport table rows use independent `preview` channels. The Worker lazily verifies the
active manifest and joins only the selected face revision's detail with its bootstrap base, then
returns one selected effect string (translation or original). It retains at most 48 completed
strings; decoded file promises are shared only while running. Each file is capped at 12 MiB.
These are basic previews, not annotation closures. Pending previews cannot delay independent
list/suggestion channels, and closed or replaced channels never publish their late results.
