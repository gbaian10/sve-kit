# Data domains

Each domain keeps its authored inputs, validation and database projection together.
Registry, products, routes, catalog, source corrections, text observations, card
extras and construction retain their own packages. Translation code is grouped
into glossary, names, templates, parameters and source inventory; digital code is
grouped into links and name policies.

Domains may use core, contracts, parsers, archive readers and build infrastructure.
They do not import workflows or CLI entry points. Parse, ingest and build have no
reverse dependency on domains. Physical region literals and acquisition source
namespaces are defined in `core.regions`; pure relative path validation is defined
in `core.paths`.
