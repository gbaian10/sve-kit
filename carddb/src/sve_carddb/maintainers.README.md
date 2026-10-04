# Maintainer identities

`maintainers.toml` is the repository-controlled list of human reviewer identities.
It is shipped inside the carddb package. Add or remove an identity through a
reviewed PR, then restart the process to load the new list. Environment variables,
crawler settings and caller-supplied receipts cannot extend it. The initial list
preserves the previous reviewer policy.

The list must be nonempty, contain unique ASCII account names, and have no other
fields. Missing, malformed or invalid configuration fails closed. Receipt checks
require exact identity strings; whitespace, case changes and tool names are not
aliases. Being listed verifies identity eligibility, not that an approval event
actually happened: exact membership, source evidence, decision state and delegated
review checks still apply.

`maintainers.py` supplies the shared predicate and Pydantic reviewer annotation.
SQLite's `sve_is_maintainer` function uses the same predicate for name-override
cross-table checks. Approval loaders, actual human sample membership and policy
application priority all use this list. The list is loaded once per process and
has no fallback to an embedded account name.
