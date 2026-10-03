"""Template allocation and cross-area closure guards use sealed synthetic inputs."""

import copy
import re
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.template_parameter_rules.replay import _evidence, _references
from sve_carddb.template_parameters.references import adopted
from sve_carddb.template_translations.loader import (
    _allocation,
    _definitions,
    load_templates,
    payload,
)
from sve_carddb.template_translations.models import DefinitionRecord
from sve_carddb.translations.loader import load_glossary

from .name_replay_fixtures import human, name_term
from .template_intake_fixtures import (
    DEFINITIONS,
    TRANSLATIONS,
    definition,
    intake_case,
    policy_git,
    recognition_term_source,
    shard,
    write,
)
from .translation_fixtures import envelope

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue
    from pytest_mock import MockerFixture

    from .template_intake_fixtures import Case

__all__ = ("intake_case", "policy_git", "recognition_term_source")


def exact(message: str) -> str:
    return "^" + re.escape(message) + "$"


def typed(value: dict[str, JsonValue]) -> DefinitionRecord:
    return DefinitionRecord.model_validate_json(canonical(value))


def renamed(member: object, record: DefinitionRecord) -> DefinitionRecord:
    # The caller supplies a verified member, not a manufactured role/occurrence.
    from sve_carddb.template_translations.sources import Reconstructed  # ruff: ignore[import-outside-top-level] -- retain the helper's runtime boundary check

    assert isinstance(member, Reconstructed)
    schema = record.data.parameter_schema
    slot = schema.slots[0].model_copy(update={"name": "different_name"})
    data = record.data.model_copy(
        update={
            "parameter_schema": schema.model_copy(
                update={"slots": (slot, *schema.slots[1:])}
            )
        }
    )
    result = record.model_copy(update={"data": data})
    checksum = digest(payload(member, result))
    data = data.model_copy(
        update={"content_hash": checksum, "id": "T" + checksum[7:23]}
    )
    return result.model_copy(
        update={
            "data": data,
            "record_key": canonical(["sentence_template", data.id]).decode(),
        }
    )


@pytest.mark.parametrize(
    "case", ["old_and_new", "new_and_long", "unnecessary_long", "two_schemas"]
)
def test_payload_id_and_current_source_are_unique(
    intake_case: Case, tmp_path: Path, case: str
) -> None:
    member = next(
        m
        for m in intake_case.replay.entries
        if m.entry.id == typed(intake_case.definitions[0]).data.inventory_id
    )
    old = typed(definition(member))
    new = typed(definition(member, new=True))
    if case == "old_and_new":
        records = [old, new]
        message = "Template payload hash must have exactly one allocated ID"
    elif case == "new_and_long":
        long_data = new.data.model_copy(
            update={"id": "T" + new.data.content_hash[7:25]}
        )
        records = [
            new,
            new.model_copy(
                update={
                    "data": long_data,
                    "record_key": canonical(
                        ["sentence_template", long_data.id]
                    ).decode(),
                }
            ),
        ]
        message = "Template payload hash must have exactly one allocated ID"
    elif case == "unnecessary_long":
        long_data = new.data.model_copy(update={"id": "T" + new.data.content_hash[7:]})
        records = [
            new.model_copy(
                update={
                    "data": long_data,
                    "record_key": canonical(
                        ["sentence_template", long_data.id]
                    ).decode(),
                }
            )
        ]
        message = "Extended template ID requires every shorter adopted collision"
    else:
        records = [new, renamed(member, new)]
        message = "Template source member has multiple current definitions"
    root = intake_case.fork(tmp_path / case)
    files = copy.deepcopy(intake_case.files)
    files.pop(TRANSLATIONS)
    files[DEFINITIONS] = shard(
        [object_value(r.model_dump(mode="json")) for r in records]
    )
    revision = write(root, files)
    with pytest.raises(ValueError, match=exact(message)):
        load_templates(PinnedRepository(root), revision, intake_case.sources())


def test_hash_collision_stops_even_when_ids_differ(
    intake_case: Case, mocker: MockerFixture
) -> None:
    member = next(
        m
        for m in intake_case.replay.entries
        if m.entry.id == typed(intake_case.definitions[0]).data.inventory_id
    )
    first = typed(definition(member))
    second = renamed(member, first)
    checksum = first.data.content_hash
    second = second.model_copy(
        update={
            "data": second.data.model_copy(
                update={"content_hash": checksum, "id": "T" + checksum[7:23]}
            )
        }
    )
    real_digest = digest
    contents = {payload(member, first), payload(member, second)}
    mocker.patch(
        "sve_carddb.template_translations.loader.digest",
        side_effect=lambda raw: checksum if raw in contents else real_digest(raw),
    )
    with pytest.raises(ValueError, match=exact("Template full payload hash collision")):
        _definitions(
            ((first, "decision"), (second, "decision")), {member.entry.id: member}
        )


def test_legacy_id_collision_is_checked_before_definitions(intake_case: Case) -> None:
    member = next(
        m for m in intake_case.replay.entries if m.candidate.legacy_id is not None
    )
    altered = replace(
        member, normalized=member.normalized + " different synthetic payload"
    )
    with pytest.raises(
        ValueError,
        match=exact("Legacy template fingerprint collision across the full inventory"),
    ):
        _definitions((), {"first": member, "second": altered})


def test_extended_id_requires_all_shorter_collisions(intake_case: Case) -> None:
    base = typed(intake_case.definitions[0])
    checksum = "sha256:" + "a" * 64
    long = base.model_copy(
        update={
            "data": base.data.model_copy(
                update={"id": "T" + "a" * 20, "content_hash": checksum}
            )
        }
    )
    prefix16 = base.model_copy(
        update={
            "data": base.data.model_copy(
                update={
                    "id": "T" + "a" * 16,
                    "content_hash": "sha256:" + "a" * 16 + "b" * 48,
                }
            )
        }
    )
    prefix18 = base.model_copy(
        update={
            "data": base.data.model_copy(
                update={
                    "id": "T" + "a" * 18,
                    "content_hash": "sha256:" + "a" * 18 + "c" * 46,
                }
            )
        }
    )
    definitions = {r.data.id: r for r in (long, prefix16, prefix18)}
    _allocation(long, definitions)
    definitions.pop(prefix16.data.id)
    with pytest.raises(
        ValueError,
        match=exact("Extended template ID requires every shorter adopted collision"),
    ):
        _allocation(long, definitions)


@pytest.mark.parametrize(
    "parent", ["legacy", "other_legacy", "adopted", "other_adopted"]
)
def test_supersedes_tracks_only_its_verified_source_family(
    intake_case: Case, tmp_path: Path, parent: str
) -> None:
    first = typed(intake_case.definitions[0])
    member = next(
        m for m in intake_case.replay.entries if m.entry.id == first.data.inventory_id
    )
    child = renamed(member, first)
    second = typed(intake_case.definitions[1])
    ancestor = first if parent in {"legacy", "adopted"} else second
    child = child.model_copy(
        update={
            "data": child.data.model_copy(update={"supersedes_id": ancestor.data.id})
        }
    )
    records = [child] if "adopted" not in parent else [child, ancestor]
    root = intake_case.fork(tmp_path / parent)
    files = copy.deepcopy(intake_case.files)
    files.pop(TRANSLATIONS)
    files[DEFINITIONS] = shard(
        [object_value(r.model_dump(mode="json")) for r in records]
    )
    revision = write(root, files)
    if parent.startswith("other"):
        message = (
            "Template supersedes adopted parent belongs to another source family"
            if parent == "other_adopted"
            else "Template supersedes requires an adopted parent or its verified legacy family"
        )
        with pytest.raises(ValueError, match=exact(message)):
            load_templates(PinnedRepository(root), revision, intake_case.sources())
    else:
        result = load_templates(PinnedRepository(root), revision, intake_case.sources())
        assert result.frequencies == ((child.data.id, 1),)
        assert result.unadopted_parents == (
            ((child.data.id, first.data.id),) if parent == "legacy" else ()
        )
        assert result.database_parent(child.data.id) == (
            None if parent == "legacy" else first.data.id
        )
        assert (
            next(
                r
                for r, _ in result.records()
                if isinstance(r, DefinitionRecord) and r.data.id == child.data.id
            ).data.supersedes_id
            == first.data.id
        )


def test_glossary_pin_covers_templates_and_inventory(
    intake_case: Case, tmp_path: Path
) -> None:
    root = intake_case.fork(tmp_path / "mixed", published=True)
    sources = intake_case.sources()
    before = load_glossary(root / "authored")
    assert len(before.records()) == 1
    assert len(array(before.pins()["shards"])) == 4
    refs = adopted(
        root / "authored",
        _evidence(
            PinnedRepository(root),
            intake_case.source.recipe,
            {"test-store": intake_case.source.store},
        ),
    )
    pin = object_value(refs.pins["glossary"])
    refs.pins["glossary"] = {**pin, "authored_revision": intake_case.revision}
    recipe = intake_case.source.recipe
    config = copy.deepcopy(recipe.config)
    config["references"] = refs.pins
    recipe = recipe.model_copy(
        update={"config": config, "config_hash": digest(canonical(config))}
    )
    rebuilt = _references(
        PinnedRepository(root), recipe, {"test-store": intake_case.source.store}
    )
    assert rebuilt.terms == refs.terms
    assert sources is intake_case.replay_sources


def test_template_loader_accepts_override_area_and_glossary_validates_it(
    intake_case: Case, tmp_path: Path
) -> None:
    root = intake_case.fork(tmp_path / "overrides")
    ref = intake_case.replay.entries[0].entry.source_ref.model_dump(mode="json")
    subject: dict[str, JsonValue] = {
        "card_id": "card:synthetic",
        "face_id": "face:synthetic",
        "source_lang": "ja",
        "source_hash": ref["text_hash"],
    }
    record: dict[str, JsonValue] = {
        "kind": "card_name_concept",
        "filing_key": "concepts",
        "record_key": canonical(["card_name_concept", subject, 1]).decode(),
        "data": {
            "subject": subject,
            "term_id": "term:name.synthetic",
            "source_ref": ref,
            "identity_basis": {
                "authored_revision": intake_case.prior,
                "registry_index_hash": "sha256:" + "d" * 64,
                "transition_index_hash": None,
            },
            "reason": "Synthetic unresolved concept",
            "adoption_no": 1,
            "predecessor": None,
        },
        "evidence": [],
    }
    files = copy.deepcopy(intake_case.files)
    files["translations/glossary/concepts/002.yaml"] = envelope([name_term()])
    files["translations/overrides/concepts/001.yaml"] = human([record])
    revision = write(root, files)
    result = load_templates(PinnedRepository(root), revision, intake_case.sources())
    assert len(result.glossary.records()) == 3
    assert len(load_glossary(root / "authored").records()) == 3
    # Foreign areas never bypass the original glossary validator.
    file = root / "authored/translations/glossary/concepts/001.yaml"
    value = object_value(parse(file.read_bytes()))
    object_value(array(value["decisions"])[0])["membership_hash"] = "sha256:" + "f" * 64
    write(root, {"translations/glossary/concepts/001.yaml": value})
    with pytest.raises(
        ValueError, match=exact("Translation decision exact membership mismatch")
    ):
        load_glossary(root / "authored")


@pytest.mark.parametrize(
    "change", ["closure", "duplicate", "order", "day", "family", "same_lang"]
)
def test_remaining_review_counterexamples(
    intake_case: Case, tmp_path: Path, change: str
) -> None:
    root = intake_case.fork(tmp_path / change)
    files = copy.deepcopy(intake_case.files)
    sources = intake_case.sources()
    area = TRANSLATIONS if change in {"closure", "day", "same_lang"} else DEFINITIONS
    records = [object_value(r) for r in array(object_value(files[area])["records"])]
    ref = intake_case.replay.entries[0].entry.source_ref.model_dump(mode="json")
    evidence: dict[str, JsonValue] = {"role": "synthetic sample", "source_ref": ref}
    if change == "closure":
        object_value(ref)["source_version_id"] = "src:v1:" + "f" * 64
        records[0]["evidence"] = [evidence]
        message = "Template evidence is absent from its verified frozen field closure"
    elif change in {"duplicate", "order"}:
        other = {**evidence, "role": "z synthetic sample"}
        records[0]["evidence"] = (
            [evidence, evidence] if change == "duplicate" else [other, evidence]
        )
        message = "Adoption array must be sorted and unique"
    elif change == "family":
        other_record = typed(intake_case.definitions[1])
        member = next(
            m
            for m in intake_case.replay.entries
            if m.entry.id == other_record.data.inventory_id
        )
        records[0]["evidence"] = [
            {
                "role": "synthetic different family",
                "source_ref": member.entry.source_ref.model_dump(mode="json"),
            }
        ]
        # Two different candidates can share a complete field. Use a separate locator
        # in a verified replay cache so field closure succeeds but family binding fails.
        member = replace(
            member,
            entry=member.entry.model_copy(
                update={
                    "source_ref": member.entry.source_ref.model_copy(
                        update={"locator": "/faces/0/sections/9"}
                    )
                }
            ),
        )
        records[0]["evidence"] = [
            {
                "role": "synthetic different family",
                "source_ref": member.entry.source_ref.model_dump(mode="json"),
            }
        ]
        from sve_carddb.template_translations.sources import TemplateSources  # ruff: ignore[import-outside-top-level] -- a detached cache supplies one separately verified evidence field

        sources = TemplateSources(
            PinnedRepository(root),
            {"test-store": intake_case.source.store},
            main_revision=intake_case.source.main,
            legacy_bytes=intake_case.source.legacy,
        )
        replay = replace(
            intake_case.replay, entries=(*intake_case.replay.entries, member)
        )
        sources._cache[b"detached-evidence"] = replay
        # Use the complete original source replay for inventory verification.
        sources._cache[
            canonical([p.model_dump(mode="json") for p in intake_case.pins])
        ] = intake_case.replay
        message = "Template definition evidence must belong to its matched family"
    elif change == "same_lang":
        data = object_value(records[0]["data"])
        data["lang"] = "ja"
        records[0]["record_key"] = canonical(
            ["template_translation", data["template_id"], "ja", data["revision"]]
        ).decode()
        message = "Template translation language must differ from its source"
    else:
        message = "Invalid adopted template shard"
    files[area] = shard(records)
    if change == "day":
        decision = object_value(array(object_value(files[area])["decisions"])[0])
        decision["reviewed_at"] = "2026-10-02T01:00:00Z"
    revision = write(root, files)
    with pytest.raises(ValueError, match=exact(message)):
        load_templates(
            PinnedRepository(root),
            revision,
            sources,
        )


def test_new_short_id_collision_only_extends_new_allocation(
    intake_case: Case, mocker: MockerFixture
) -> None:
    members = {m.entry.id: m for m in intake_case.replay.entries}
    first, second = (typed(r) for r in intake_case.definitions)
    first_hash = "sha256:" + "a" * 16 + "b" * 48
    second_hash = "sha256:" + "a" * 16 + "c" * 48
    first = first.model_copy(
        update={
            "data": first.data.model_copy(
                update={"id": "T" + first_hash[7:23], "content_hash": first_hash}
            )
        }
    )
    second = second.model_copy(
        update={
            "data": second.data.model_copy(
                update={"id": "T" + second_hash[7:25], "content_hash": second_hash}
            )
        }
    )
    content_hashes = {
        payload(members[first.data.inventory_id], first): first_hash,
        payload(members[second.data.inventory_id], second): second_hash,
    }
    mocker.patch(
        "sve_carddb.template_translations.loader.digest",
        side_effect=lambda raw: content_hashes.get(raw, digest(raw)),
    )
    definitions, frequencies, parents = _definitions(
        ((first, "d:first"), (second, "d:second")), members
    )
    assert set(definitions) == {first.data.id, second.data.id}
    assert frequencies == tuple(sorted(((first.data.id, 1), (second.data.id, 1))))
    assert parents == ()
