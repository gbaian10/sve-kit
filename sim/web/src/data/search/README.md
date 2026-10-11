# Worker search and complete list views

`SearchSession` owns one Worker generation. Each `createChannel()` has its own last input,
query ID and result. Channels support `search`, `view`, `suggest`, and a batch of at most 20
recent printing summaries. Replies carry generation, channel ID and query ID; old replies
cannot replace current results. Suggestions preserve exact printing identity and name/alias
ranking. Missing recent IDs explicitly return `not-in-edition`.

Search supports card/printing units, text, classes, printing home-set family codes and cost. Types, rarities,
mechanics, alternate-art filtering, the art unit and other sorting return `unsupported-query`.
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
Prefixes sort empty, LD, P, PR, SL in ASCII order within the same set. For V3, pass
`state: { ...DEFAULT_QUERY, unit: "printing", sort: "no", sets: [familyCode] }` to
`channel.view` or `useListView`. Each item key is its printing ID; `summary.cardId`
remains the card identity, and every face/image source belongs to that printing.
Totals and paging count printings, and query/cache identity distinguishes units.
The existing P2a screen retains its card-unit default; V3 can set printing as its default
without a new index layout or a full catalog on the main thread.

The Worker retains scalar index columns, a shared string pool, multilingual class/set
options and image targets (printing ID, face ID, permanent integer ID, ordinal and owner).
Only bounded pages and purpose-specific summaries return to the main thread. Set options
come from product families and use natural family-code ordering, then permanent ID.

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

Cache identity includes root, manifest hash, reader build and local format. View slots also
include the normalizer version, selected-region policy and complete query. Multilingual
names avoid a separate language-specific record. Index format is 3; view format is 1;
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
`useListView`, `useWorkerSuggestions`, `useRecentSummaries` and `useSetOptions`.
`useListView` requests bounded 60-card pages and returns a complete image adapter after all
requested pages belong to the same generation. Pending and unsupported results remain
explicit. `useCardSnapshot` is the temporary full-detail path for the card page.
