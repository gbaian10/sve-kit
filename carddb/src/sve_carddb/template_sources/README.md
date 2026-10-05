# Current template source inventory

`inventory.scan_current()` enumerates every current JP card page in an explicit
sealed batch, validates frozen source identity and retains parsed documents for
reuse in the same build. Source fields are partitioned by the installed normalizer.
Every position has an exact source reference, code-point spans, role and hash.
`coverage()` reports missing or unknown fields independently of the enumerated
positions. Unknown presence never becomes an absent effect.

`normalizer` retains the deterministic body/reminder/token-header/layout recipe
and the established body fingerprint algorithm. Flavor text is not inventoried; it is translated directly by source hash. `inventory.replay()`
checks one current position against an already verified projected field; it is not
historical producer replay.

The old diagnostic CLI, catalog comparison, frozen recipe/environment pins and
v1 inventory writers have been removed. Current builds do not require a legacy
catalog file. Older implementations are available only in Git history.
