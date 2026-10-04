"""Apply current name conditions to complete catalogues from this build's inputs."""

from collections import defaultdict
from dataclasses import dataclass
from functools import cached_property
from types import MappingProxyType
from typing import TYPE_CHECKING

from sve_carddb.build_inputs import uses_sorted
from sve_carddb.catalog.adoption_models import Batch, ReviewContext, SourceRef
from sve_carddb.digital_links.catalogue import complete_inventory
from sve_carddb.digital_links.importer import review_context
from sve_carddb.digital_name_policies.evaluate import FrozenName, _parents
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build_inputs import SourceUse
    from sve_carddb.digital_name_policies.current_models import Policy
    from sve_carddb.translations.sources import Sources


@dataclass(frozen=True)
class Catalogue:
    policy_hash: str
    purpose: str
    names: tuple[FrozenName, ...]
    uses: tuple[SourceUse, ...]
    whitespace: frozenset[int]
    kana: tuple[tuple[int, int], ...]
    excluded_names: frozenset[str]
    excluded_targets: frozenset[tuple[str, str, str]] = frozenset()

    @cached_property
    def japanese(self) -> Mapping[tuple[str, str], tuple[FrozenName, ...]]:
        """Exact comparisons follow hash lookup; same hashes do not merge names."""
        groups: dict[tuple[str, str], list[FrozenName]] = defaultdict(list)
        for name in self.names:
            if name.lang == "ja" and name.text:
                groups[name.game, digest(name.text.encode())].append(name)
        return MappingProxyType({key: tuple(rows) for key, rows in groups.items()})

    @cached_property
    def translated(self) -> Mapping[tuple[str, str, str], FrozenName]:
        """Every actual phase must have its own target, including missing values."""
        return MappingProxyType(
            {
                (n.game, n.official_id, n.phase): n
                for n in self.names
                if n.lang == "zh-Hant"
            }
        )

    def matching(self, text: str, game: str) -> tuple[FrozenName, ...]:
        """Rules use exact whole names and never trim or normalize."""
        return tuple(
            n
            for n in self.japanese.get((game, digest(text.encode())), ())
            if text and n.text == text
        )


def catalogue(policy: Policy, sources: Sources) -> Catalogue:
    """Verify the current frozen closure once, without approval-time program replay."""
    config = object_value(parse(sources.build.configuration.encode()))
    if "digital_link_sources" in config:
        review = review_context(sources)
    else:
        refs = tuple(
            SourceRef.model_validate_json(canonical(r))
            for r in array(object_value(config.get("digital_evidence"))["refs"])
        )
        batches = sorted({(r.store_id, r.batch_id) for r in refs})
        review = ReviewContext(
            context=sources.build,
            source_batches=tuple(Batch(store_id=s, batch_id=b) for s, b in batches),
        )
    names: list[FrozenName] = []
    for game in policy.content.game_priority:
        full = complete_inventory(sources, review, game)
        _parents(full, game)
        names.extend(
            FrozenName(n.game, n.official_id, n.phase, n.lang, n.text, n.ref)
            for _, n in sorted(full.items())
        )
    minimum = policy.content.target_minimum_check
    return Catalogue(
        digest(canonical(policy.model_dump(mode="json", exclude={"note"}))),
        "names",
        tuple(names),
        uses_sorted(sources.uses),
        frozenset(minimum.whitespace_codepoints),
        minimum.kana_ranges,
        frozenset(
            e.source_name_hash
            for e in policy.content.excluded_names
            if e.source_lang == policy.content.scope.source_lang
        ),
    )
