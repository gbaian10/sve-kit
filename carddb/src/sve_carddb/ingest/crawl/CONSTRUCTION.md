# Construction source skeleton

`construction_sources` classifies an explicit JSON array of JP/EN official HTML
URLs as limit pages, news indexes/notices or rules pages. It does not discover or
authorize URLs. Hrefs in an HTML outline remain observations, including unrelated
formats, PDFs and non-news links; none are fetched automatically.

There is deliberately no live CLI. The only execution evidence for this module
is synthetic HTML and MockTransport. Official DOM selectors, parser recipes,
discovery range evidence and restriction semantics still need an approved pilot.
`parse_construction_outline` returns `semantic_state="unreviewed"`; its generic
`main`/`article` shape is not a verified official-site adapter or adoption evidence.
Blocks cover only h1/h2/h3/p/li, not table cells, definition lists or deeper
headings. The required terminal HTML marker rejects obvious truncation; it does
not prove complete HTTP delivery or semantic coverage. Explicit trailing-slash
variants remain distinct requested identities until an approved adapter verifies
the official site's URL forms.

The future caller must use Settings' browser UA and >=2-second throttle, hold the
data-root lock before opening the manifest, perform quiet/preflight checks and
supply a create-only Writer. The batch uses the existing Client/Writer, records
normal fetch metadata, allows at most one request per pending selected URL and
refuses redirects and retries. Existing trusted sources are skipped; untrusted
sources fail the entire preflight. It neither refreshes nor repairs sources.
An interrupted write can leave a started request; the caller must inspect and
resolve interrupted state before resuming. The batch does not silently repair it.

Successful raw acquisition is not adoption: seal the source inventory and SQLite
backup, independently back up and restore-check, then obtain construction-adoption
reviews. This skeleton does not run any of those steps or write public snapshots.
