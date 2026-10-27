"""Negative acceptance cases at the public candidate and reader boundaries."""

import json
from dataclasses import replace
from pathlib import Path

import pytest

from sve_carddb.core.json import digest
from sve_carddb.domains.card_extras.reviewed_errata import Notices
from sve_carddb.domains.parsed_cards import batches
from sve_carddb.domains.parsed_cards.codec import encode, schema
from sve_carddb.domains.parsed_cards.generate import assemble, generate
from sve_carddb.domains.parsed_cards.models import (
    BatchFile,
    BatchSelection,
    FilePair,
    Index,
    ParsedObservation,
)
from sve_carddb.domains.parsed_cards.serialization import combine, separate
from sve_carddb.domains.parsed_cards.storage import Catalog, load, write
from sve_carddb.domains.parsed_cards.wire import CardFile, EvidenceFile
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.ingest.archive.source_archive import ArchiveStore, Missing, seal_batch
from sve_carddb.parse.pages import extract_en

from ...ingest.test_source_archive import _put, _resource, _store
from .test_inputs import REVISION, _batch


def _catalog(tmp_path: Path) -> Catalog:
    sources, number = _batch(tmp_path, "en")
    return generate(
        [("en", sources)], {("en", number): "SYN"}, producer_revision=REVISION
    )


def _rehashed(value: ParsedObservation) -> ParsedObservation:
    return ParsedObservation.model_validate_json(
        value.model_copy(
            update={"content_hash": value.calculated_hash()}
        ).model_dump_json()
    )


def test_reparse_rejects_changed_result_even_with_recomputed_hash(
    tmp_path: Path,
) -> None:
    sources, number = _batch(tmp_path, "en")
    current = generate(
        [("en", sources)], {("en", number): "SYN"}, producer_revision=REVISION
    )
    old = next(iter(current.observations().values()))
    record = old.document.record
    assert isinstance(record, extract_en.CardRecord)
    record = replace(
        record,
        faces=[
            replace(record.faces[0], name="Changed reviewed name"),
            *record.faces[1:],
        ],
    )
    changed = _rehashed(
        old.model_copy(
            update={"document": old.document.model_copy(update={"record": record})}
        )
    )
    previous = assemble([changed], current.filing(), current.batches)
    with pytest.raises(ValueError, match="Reparsing changed an existing recipe"):
        generate([("en", sources)], {}, producer_revision=REVISION, previous=previous)
    root = tmp_path / "self-consistent"
    assert write(root, previous) == previous
    with pytest.raises(
        ValueError, match="deletes or replaces a historical observation"
    ):
        load(root).validate_update(current)


@pytest.mark.parametrize(
    "field", ["sha256", "bytes", "evidence_sha256", "evidence_bytes"]
)
def test_index_file_pins_are_enforced(tmp_path: Path, field: str) -> None:
    catalog = _catalog(tmp_path)
    root = tmp_path / "inputs"
    write(root, catalog)
    pin = catalog.index.files[0]
    bad = pin.model_copy(
        update={
            field: "sha256:" + "0" * 64
            if field.endswith("sha256")
            else getattr(pin, field) + 1
        }
    )
    (root / "index.json").write_bytes(
        encode(catalog.index.model_copy(update={"files": (bad,)}))
    )
    message = (
        "Parsed evidence size/hash mismatch"
        if field.startswith("evidence")
        else "Parsed pair size/hash mismatch"
    )
    with pytest.raises(ValueError, match=message):
        load(root)


@pytest.mark.parametrize("field", ["cards", "observations"])
def test_index_counts_are_enforced(tmp_path: Path, field: str) -> None:
    catalog = _catalog(tmp_path)
    root = tmp_path / "inputs"
    write(root, catalog)
    pin = catalog.index.files[0]
    bad = pin.model_copy(update={field: getattr(pin, field) + 1})
    (root / "index.json").write_bytes(
        encode(catalog.index.model_copy(update={"files": (bad,)}))
    )
    with pytest.raises(ValueError, match="Parsed pair member count mismatch"):
        load(root)


def test_generation_carries_old_filing_and_rejects_conflicts(tmp_path: Path) -> None:
    sources, number = _batch(tmp_path, "en")
    original = generate(
        [("en", sources)], {("en", number): "SYN"}, producer_revision=REVISION
    )
    assert (
        generate([("en", sources)], {}, producer_revision=REVISION, previous=original)
        == original
    )
    with pytest.raises(
        ValueError, match="Parsed request conflicts with existing filing"
    ):
        generate(
            [], {("en", number): "OTHER"}, producer_revision=REVISION, previous=original
        )
    with pytest.raises(ValueError, match="Missing explicit parsed card filing"):
        generate([("en", sources)], {}, producer_revision=REVISION)
    with pytest.raises(ValueError, match="Missing explicit parsed card filing"):
        assemble(original.observations().values(), {}, original.batches)


def test_duplicate_source_rejected_before_serializing(tmp_path: Path) -> None:
    original = _catalog(tmp_path)
    value = next(iter(original.observations().values()))
    with pytest.raises(ValueError, match="Duplicate parsed source version"):
        assemble([value, value], original.filing(), original.batches)
    pair = original.files[0]
    invalid = pair.model_copy(update={"cards": (pair.cards[0], pair.cards[0])})
    with pytest.raises(ValueError, match="Duplicate parsed source version"):
        separate(invalid)
    with pytest.raises(ValueError, match="Duplicate parsed source version"):
        replace(original, files=(pair, pair)).observations()


def test_duplicate_cards_and_cross_region_pairs_rejected(tmp_path: Path) -> None:
    original = _catalog(tmp_path)
    pair = original.files[0]
    with pytest.raises(ValueError, match="Parsed cards must be sorted and unique"):
        FilePair.model_validate_json(
            pair.model_copy(
                update={"cards": (pair.cards[0], pair.cards[0])}
            ).model_dump_json()
        )
    with pytest.raises(ValueError, match="Parsed pair region mismatch"):
        FilePair.model_validate_json(
            pair.model_copy(update={"region": "jp"}).model_dump_json()
        )


def test_valid_document_with_wrong_official_card_url_rejected(tmp_path: Path) -> None:
    value = next(iter(_catalog(tmp_path).observations().values()))
    document = value.document.model_copy(
        update={"record": replace(value.document.record, number="SYN-198EN")}
    )
    with pytest.raises(ValueError, match="Parsed source region/card identity mismatch"):
        _rehashed(value.model_copy(update={"document": document}))


def test_misfiled_card_is_rejected_with_valid_outer_pins(tmp_path: Path) -> None:
    catalog = _catalog(tmp_path)
    root = tmp_path / "inputs"
    write(root, catalog)
    pin = catalog.index.files[0]
    bad = pin.model_copy(update={"path": "en/SYN/000-099.json"})
    (root / pin.path).rename(root / bad.path)
    (root / pin.evidence_path).rename(root / bad.evidence_path)
    (root / "index.json").write_bytes(
        encode(catalog.index.model_copy(update={"files": (bad,)}))
    )
    with pytest.raises(ValueError, match="Duplicate or incorrectly filed parsed card"):
        load(root)


@pytest.mark.parametrize("group", ["observations", "presence", "documents"])
def test_map_order_is_accepted_when_outer_hash_is_updated(
    tmp_path: Path, group: str
) -> None:
    first, number = _batch(tmp_path / "first", "en")
    second, _ = _batch(tmp_path / "second", "en", changed=True)
    catalog = generate(
        [("en", first), ("en", second)],
        {("en", number): "SYN"},
        producer_revision=REVISION,
    )
    root = tmp_path / "inputs"
    write(root, catalog)
    pin = catalog.index.files[0]
    sidecar = group != "documents"
    path = root / (pin.evidence_path if sidecar else pin.path)
    data = json.loads(path.read_bytes())
    parent = (
        data["cards"][0]
        if group == "documents"
        else data["shared"]
        if group == "presence"
        else data
    )
    assert len(parent[group]) > 1
    parent[group] = dict(reversed(parent[group].items()))
    raw = (json.dumps(data, ensure_ascii=False, indent=2) + "\n").encode()
    path.write_bytes(raw)
    prefix = "evidence_" if sidecar else ""
    bad = pin.model_copy(
        update={prefix + "sha256": digest(raw), prefix + "bytes": len(raw)}
    )
    (root / "index.json").write_bytes(
        encode(catalog.index.model_copy(update={"files": (bad,)}))
    )
    assert load(root).files == catalog.files


def test_unchanged_batch_does_not_rewrite_card_or_evidence_files(
    tmp_path: Path,
) -> None:
    first, number = _batch(tmp_path / "first", "en")
    data = first.root.parent / "data"
    store = ArchiveStore(
        data,
        data / "manifest/manifest.sqlite",
        data / "manifest/.lock",
        first.root,
        first.store_id,
    )
    sealed = seal_batch(store)
    second = FrozenSources(store.root, store.store_id, sealed.batch_id)
    assert first.batch_id != second.batch_id
    before = generate(
        [("en", first)], {("en", number): "SYN"}, producer_revision=REVISION
    )
    after = generate([("en", second)], {}, producer_revision=REVISION, previous=before)
    assert before.index.files == after.index.files
    assert before.files == after.files
    assert before.batches[0] in after.batches
    assert len(after.batches) == 2
    public = b"".join(encode(b) for b in after.batches) + encode(after.index)
    for private_field in (
        b'"created_at"',
        b'"manifest"',
        b'"store_id"',
        b'"receipt_id"',
    ):
        assert private_field not in public


def test_observation_must_be_selected_by_a_batch(tmp_path: Path) -> None:
    first, number = _batch(tmp_path / "first", "en")
    second, _ = _batch(tmp_path / "second", "en", changed=True)
    catalog = generate(
        [("en", first), ("en", second)],
        {("en", number): "SYN"},
        producer_revision=REVISION,
    )
    root = tmp_path / "inputs"
    write(root, catalog)
    removed, remaining = catalog.index.batches
    for part in range(len(removed.parts)):
        (root / removed.path(part)).unlink()
    (root / "index.json").write_bytes(
        encode(catalog.index.model_copy(update={"batches": (remaining,)}))
    )
    with pytest.raises(
        ValueError, match="Parsed observation is absent from every batch"
    ):
        load(root)


def test_template_only_change_reuses_card_document(tmp_path: Path) -> None:
    first, number = _batch(tmp_path / "first", "en")
    second, _ = _batch(tmp_path / "second", "en", layout_only=True)
    before = generate(
        [("en", first)], {("en", number): "SYN"}, producer_revision=REVISION
    )
    after = generate([("en", second)], {}, producer_revision=REVISION, previous=before)
    cards, _ = separate(after.files[0])
    assert len(cards.cards[0].documents) == 1
    assert len(cards.cards[0].observations) == 2
    assert len({o.document_ref for o in cards.cards[0].observations}) == 1
    assert cards.cards[0].documents == separate(before.files[0])[0].cards[0].documents
    assert write(tmp_path / "inputs", after) == after


@pytest.mark.parametrize("change", ["selection", "drop"])
def test_historical_batch_selection_is_immutable(tmp_path: Path, change: str) -> None:
    original = _catalog(tmp_path)
    batch = original.batches[0]
    changed = batch.model_copy(
        update={"versions": dict.fromkeys(batch.versions, False)}
    )
    updated = replace(original, batches=() if change == "drop" else (changed,))
    with pytest.raises(ValueError, match="Parsed update changes a historical batch"):
        updated.validate_update(original)


def test_generation_rejects_changed_existing_batch_id(tmp_path: Path) -> None:
    sources, number = _batch(tmp_path, "en")
    previous = generate(
        [("en", sources)], {("en", number): "SYN"}, producer_revision=REVISION
    )
    batch = previous.batches[0]
    previous = assemble(
        previous.observations().values(),
        previous.filing(),
        [batch.model_copy(update={"versions": dict.fromkeys(batch.versions, False)})],
    )
    with pytest.raises(ValueError, match="Source batch changed under an existing pin"):
        generate([("en", sources)], {}, producer_revision=REVISION, previous=previous)


def test_generation_rejects_incomplete_batch(tmp_path: Path) -> None:
    sources, number = _batch(tmp_path, "en")
    sources.inventory = sources.inventory.model_copy(
        update={
            "missing": [
                Missing(
                    url="https://example.invalid/missing",
                    expected_raw_sha256="0" * 64,
                    reason="missing_raw",
                )
            ]
        }
    )
    with pytest.raises(ValueError, match="Cannot admit an incomplete source batch"):
        generate([("en", sources)], {("en", number): "SYN"}, producer_revision=REVISION)


def test_generation_rejects_foreign_region(tmp_path: Path) -> None:
    sources, number = _batch(tmp_path, "en")
    with pytest.raises(
        ValueError, match="Parsed batch must contain only same-region SVE card pages"
    ):
        generate([("jp", sources)], {("jp", number): "SYN"}, producer_revision=REVISION)


def test_oversized_card_file_requires_contract_review(tmp_path: Path) -> None:
    original = _catalog(tmp_path)
    value = next(iter(original.observations().values()))
    record = replace(value.document.record, notes=["x" * 1_048_576])
    value = _rehashed(
        value.model_copy(
            update={"document": value.document.model_copy(update={"record": record})}
        )
    )
    with pytest.raises(ValueError, match="Parsed file exceeds 1 MiB"):
        assemble([value], original.filing(), original.batches)


@pytest.mark.parametrize(
    ("name", "model"),
    [
        ("index", Index),
        ("cards", CardFile),
        ("evidence", EvidenceFile),
        ("batch", BatchFile),
        ("errata", Notices),
    ],
)
def test_committed_schema_has_no_drift(name: str, model: type) -> None:
    root = Path(__file__).parents[4]
    expected = schema(model)
    path = (
        (root / "docs/schema/ingest/parsed-errata.schema.json")
        if name == "errata"
        else (root / "docs/schema/ingest/parsed-cards" / f"{name}.schema.json")
    )
    assert json.loads(path.read_bytes()) == expected


def test_non_sve_source_is_rejected(tmp_path: Path) -> None:
    store = _store(tmp_path)
    raw = b"synthetic bitmap"
    _put(store, _resource("https://example.invalid/card.png", "raw/card.png", raw), raw)
    sealed = seal_batch(store)
    sources = FrozenSources(store.root, store.store_id, sealed.batch_id)
    with pytest.raises(
        ValueError, match="Parsed batch must contain only same-region SVE card pages"
    ):
        generate([("jp", sources)], {}, producer_revision=REVISION)


@pytest.mark.parametrize(
    "change", ["hash", "bytes", "identity", "missing", "foreign", "order"]
)
def test_batch_selection_reader_checks(tmp_path: Path, change: str) -> None:
    catalog = _catalog(tmp_path)
    root = tmp_path / "inputs"
    write(root, catalog)
    pin = catalog.index.batches[0]
    batch = batches.parts(catalog.batches[0])[0]
    part_pin = pin.parts[0]
    if change in {"hash", "bytes"}:
        bad_part = part_pin.model_copy(
            update={"sha256": "sha256:" + "0" * 64}
            if change == "hash"
            else {"bytes": part_pin.bytes + 1}
        )
        message = "Parsed batch size/hash mismatch"
    else:
        data = batch.model_dump(mode="json")
        if change == "identity":
            data["id"] = "sha256:" + "0" * 64
            message = "Parsed batch identity differs from pin"
        elif change == "foreign":
            data["region"] = "jp"
            message = "Parsed batch has an absent or foreign source version"
        elif change == "missing":
            data["versions"] = {"src:v1:" + "0" * 64: True}
            message = "Parsed batch has an absent or foreign source version"
        else:
            data["versions"] = {"src:v1:" + "f" * 64: True, "src:v1:" + "0" * 64: True}
            message = "Parsed batch versions must be nonempty and sorted"
        raw = (json.dumps(data, ensure_ascii=False, indent=2) + "\n").encode()
        (root / pin.path(0)).write_bytes(raw)
        bad_part = part_pin.model_copy(
            update={"sha256": digest(raw), "bytes": len(raw)}
        )
    (root / "index.json").write_bytes(
        encode(
            catalog.index.model_copy(
                update={"batches": (pin.model_copy(update={"parts": (bad_part,)}),)}
            )
        )
    )
    with pytest.raises(ValueError, match=message):
        load(root)


def test_batch_cannot_select_two_current_versions_of_one_url(tmp_path: Path) -> None:
    first, number = _batch(tmp_path / "first", "en")
    second, _ = _batch(tmp_path / "second", "en", changed=True)
    catalog = generate(
        [("en", first), ("en", second)],
        {("en", number): "SYN"},
        producer_revision=REVISION,
    )
    bad = catalog.batches[0].model_copy(
        update={"versions": dict.fromkeys(sorted(catalog.observations()), True)}
    )
    with pytest.raises(ValueError, match="Multiple current versions of a parsed page"):
        assemble(catalog.observations().values(), catalog.filing(), [bad])


def test_documents_reference_cannot_escape_its_card(tmp_path: Path) -> None:
    catalog = _catalog(tmp_path)
    cards, evidence = separate(catalog.files[0])
    card = cards.cards[0]
    invalid = cards.model_copy(
        update={"cards": (card.model_copy(update={"documents": {}}),)}
    )

    with pytest.raises(
        ValueError, match="Parsed evidence has a dangling shared reference"
    ):
        combine(invalid, evidence)


def test_region_mismatch_reaches_semantic_check_with_valid_wire_schema(
    tmp_path: Path,
) -> None:
    catalog = _catalog(tmp_path)
    cards, evidence = separate(catalog.files[0])
    pair = catalog.files[0]
    pin = catalog.index.files[0]
    cards_raw = encode(cards.model_copy(update={"region": "jp"}))
    evidence_raw = encode(evidence.model_copy(update={"region": "jp"}))
    root = tmp_path / "inputs"
    write(root, catalog)
    (root / pin.path).write_bytes(cards_raw)
    (root / pin.evidence_path).write_bytes(evidence_raw)
    bad = pin.model_copy(
        update={
            "sha256": digest(cards_raw),
            "bytes": len(cards_raw),
            "evidence_sha256": digest(evidence_raw),
            "evidence_bytes": len(evidence_raw),
        }
    )
    (root / "index.json").write_bytes(
        encode(catalog.index.model_copy(update={"files": (bad,)}))
    )
    with pytest.raises(ValueError, match="Parsed pair region mismatch"):
        load(root)
    jp, number = _batch(tmp_path / "jp", "jp")
    foreign = generate(
        [("jp", jp)], {("jp", number): "SYN"}, producer_revision=REVISION
    )
    mixed = pair.model_copy(
        update={
            "cards": tuple(
                sorted((*pair.cards, *foreign.files[0].cards), key=lambda c: c.card_no)
            )
        }
    )
    with pytest.raises(ValueError, match="Parsed pair region mismatch"):
        FilePair.model_validate_json(mixed.model_dump_json())


def test_large_batch_has_fixed_bounded_parts_and_lossless_selection() -> None:
    selection = BatchSelection(
        id="sha256:" + "a" * 64,
        region="en",
        versions={f"src:v1:{i:064x}": True for i in range(15001)},
    )
    members = batches.parts(selection)
    assert len(members) == 16
    assert all(len(p.versions) == 1000 for p in members[:-1])
    assert len(members[-1].versions) == 1
    assert all(len(encode(p)) < 100_000 for p in members)
    assert batches.join(members) == selection
    with pytest.raises(ValueError, match="Parsed batch parts are not canonical"):
        batches.join(tuple(reversed(members)))
    with pytest.raises(ValueError, match="Duplicate parsed batch source version"):
        batches.join((members[0], members[0]))
    with pytest.raises(ValueError, match="Parsed batch parts are not canonical"):
        batches.join(members[1:])
