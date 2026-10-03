"""Enumerate every physical flavor field from one independently pinned sealed JP batch."""

from collections import Counter
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.catalog.adoption_models import SourceRef
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.template_parameter_rules.repository import ancestor
from sve_carddb.template_parameters.models import Range
from sve_carddb.template_sources.flavor import partition
from sve_carddb.template_sources.pins import PARSER
from sve_carddb.template_translations.flavor_models import (
    FlavorCandidate,
    FlavorEntry,
    FlavorInputs,
    FlavorSpan,
)
from sve_carddb.template_translations.flavor_owners import FlavorOwners
from sve_carddb.template_translations.flavor_pins import verify
from sve_carddb.template_translations.sources import Reconstructed, SourceReplay
from sve_carddb.translations.sources import Sources, project

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_inputs import BuildContext
    from sve_carddb.catalog.adoption_sources import PinnedRepository
    from sve_carddb.template_sources.models import Recipe


def reconstruct(  # ruff: ignore[too-many-locals,complex-structure] -- one batch shares its independent runtime and immutable identity closure
    repository: PinnedRepository,
    stores: dict[str, Path],
    pins: tuple[Recipe, ...],
    inputs: FlavorInputs | None,
    main_revision: str,
) -> SourceReplay:
    """Unknown fields stay missing; whitespace fields stay exact and outside templates."""
    build = verify(repository, pins)
    if inputs is None:
        raise ValueError("Flavor replay requires an explicit available frozen batch")
    batch = inputs.source_batch.store_id, inputs.source_batch.batch_id
    basis = inputs.identity_basis
    identity_batches = tuple((b.store_id, b.batch_id) for b in inputs.identity_batches)
    if batch[0] not in stores:
        raise ValueError("Flavor replay requires an explicit available frozen batch")
    frozen = FrozenSources(stores[batch[0]], *batch)
    if [(s.provider, s.kind) for s in frozen.inventory.scope] != [("jp", "card")]:
        raise ValueError("Flavor replay requires an exclusively JP card batch")
    owners = _owners(repository, stores, build, inputs, main_revision)
    members = []
    fields: list[dict[str, JsonValue]] = []
    units: dict[str, bytes] = {}
    for current in frozen.inventory.current:
        source, raw, descriptor = frozen.read(
            current.source_version_id, parser_version=PARSER
        )
        if (descriptor.provider, descriptor.kind, descriptor.url, source.kind) != (
            "jp",
            "card",
            current.url,
            "official_page",
        ):
            raise ValueError("Flavor frozen source identity or media mismatch")
        lang, document = project(raw, source.url, "jp")
        assert lang == "ja"
        faces = array(object_value(document).get("faces"))
        if not faces:
            raise ValueError("Flavor projection must contain physical faces")
        for index, face in enumerate(faces):
            locator = f"/faces/{index}/flavor"
            value = object_value(face).get("flavor")
            if value is not None and not isinstance(value, str):
                raise ValueError("Flavor field must be exact text or unknown")
            result = partition(value)
            fields.append(
                {
                    "source_version_id": source.id,
                    "locator": locator,
                    "state": result.state,
                    "text_hash": None if value is None else digest(value.encode()),
                }
            )
            if result.part is None:
                continue
            part = result.part
            ref = SourceRef(
                store_id=batch[0],
                batch_id=batch[1],
                source_version_id=source.id,
                parser=PARSER,
                locator=locator,
                text_hash=part.normalized_hash,
            )
            span = FlavorSpan(
                role="flavor",
                segments=(Range(start=0, end=len(part.normalized)),),
                anchor=None,
            )
            identifier = (
                "inv:"
                + digest(
                    canonical(
                        [
                            ref.model_dump(mode="json"),
                            0,
                            "flavor",
                            [[0, len(part.normalized)]],
                        ]
                    )
                )[7:]
            )
            item = FlavorEntry(
                id=identifier,
                level="sentence",
                source_ref=ref,
                line_ordinal=0,
                role="flavor",
                normalizer_id="flavor-exact-v1",
                normalized_hash=part.normalized_hash,
                legacy_fingerprint=None,
            )
            owner = (
                None if owners is None else owners.resolve(ref, source, part.normalized)
            )
            if owner is not None:
                raw_text = part.normalized.encode()
                if (
                    owner.flavor_unit_id in units
                    and units[owner.flavor_unit_id] != raw_text
                ):
                    raise ValueError(
                        "Flavor text unit ID collision across the full batch"
                    )
                units[owner.flavor_unit_id] = raw_text
            members.append(
                Reconstructed(
                    item,
                    FlavorCandidate(span),
                    part.normalized,
                    part.normalized,
                    (),
                    (),
                    () if owner is not None else ("missing_flavor_identity_owner",),
                    owner,
                )
            )
    if len({m.entry.id for m in members}) != len(members):
        raise ValueError("Flavor inventory entry IDs must be unique")
    states = Counter(str(f["state"]) for f in fields)
    closure: dict[str, JsonValue] = {
        "complete": states["unknown"] == 0,
        "source_batch": {"store_id": batch[0], "batch_id": batch[1]},
        "expected_pages": len(frozen.inventory.current),
        "parsed_pages": len(frozen.inventory.current),
        "field_states": dict(states),
        "fields": list[JsonValue](fields),
        "history_counted_in_frequency": False,
        "history_gaps": len(frozen.inventory.history_gaps),
        "identity_basis": None if basis is None else basis.model_dump(mode="json"),
        "identity_batches": [list(pair) for pair in identity_batches],
        "identity_source_uses": []
        if owners is None
        else [use.model_dump(mode="json") for use in owners.evidence.uses],
        "owner_checked_rows": sum(m.owner is not None for m in members),
        "missing_owner_rows": sum(m.owner is None for m in members),
        "identity_files": []
        if owners is None
        else [list(row) for row in owners.evidence.authored_uses],
        "runtime_dependencies": [p.model_dump(mode="json") for p in build.dependencies],
        "runtime_configuration": parse(build.configuration.encode()),
    }
    return SourceReplay(
        canonical([p.model_dump(mode="json") for p in pins]),
        tuple(sorted(members, key=lambda m: m.entry.id)),
        canonical(closure),
        canonical(
            {
                "legacy_checkpoint_applicable": False,
                "paragraph_uses": len(members),
                "distinct_paragraphs": len({m.normalized for m in members}),
            }
        ),
    )


def _owners(
    repository: PinnedRepository,
    stores: dict[str, Path],
    build: BuildContext,
    inputs: FlavorInputs,
    main_revision: str,
) -> FlavorOwners | None:
    basis = inputs.identity_basis
    if basis is None:
        return None
    batches = tuple((b.store_id, b.batch_id) for b in inputs.identity_batches)
    batch = inputs.source_batch.store_id, inputs.source_batch.batch_id
    if (
        not batches
        or batch not in batches
        or any(store not in stores for store, _ in batches)
    ):
        raise ValueError(
            "Flavor identity replay requires its complete explicit source batches"
        )
    ancestor(repository, basis.authored_revision, main_revision)
    owners = FlavorOwners(Sources(stores, repository.root, build), basis)
    owners.evidence.complete(basis, batches)
    return owners
