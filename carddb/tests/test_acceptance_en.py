"""Sealed synthetic EN acceptance and independent failures of exact evidence."""

import shutil
from dataclasses import dataclass, replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_bundle import publish_bundle, verify_bundle
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.build_inputs import BuildContext, input_record
from sve_carddb.extract.acceptance_en import acceptance_report, review_queue
from sve_carddb.extract.official_en import extract_card, legacy_projection
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.html import MissingElementError
from sve_carddb.manifest import Kind, Manifest, Region
from sve_carddb.products import Language, load_product_identities, load_products
from sve_carddb.products.official import PARSER, parse_products
from sve_carddb.products.plan import plan_official_products
from sve_carddb.registry.build import build
from sve_carddb.registry.inputs import Mapping as CardMapping
from sve_carddb.registry.preview import FrozenEN, plan_preview
from sve_carddb.registry.records import CorrectionData, CorrectionEvidence, PrintingData
from sve_carddb.registry.review import Correction, Inputs, Receipt
from sve_carddb.registry.storage import plan_files, write_files
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.source_archive import ArchiveError, seal_batch
from sve_carddb.source_corrections import FrozenImages
from sve_carddb.source_corrections.images import evidence_url
from sve_carddb.sources.official_en import card_url
from sve_carddb.text_observations import (
    Binding,
    FrozenTexts,
    Vocabulary,
    importer,
    plan_text_observations,
    populate_text_preview,
    text_configuration,
    text_preview_uses,
)

from .en_extract_fixtures import page
from .identity_evidence_fixtures import MemoryEvidence
from .product_fixtures import envelope, family, install
from .product_identity_fixtures import (
    commit,
    identity_envelope,
    identity_record,
    install_identity,
)
from .test_effect_presence import page as presence_page
from .test_source_archive import _put, _resource, _store
from .text_observation_fixtures import MemoryTexts

if TYPE_CHECKING:
    from collections.abc import Iterable
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.build_db import Database
    from sve_carddb.build_inputs import InputRecord, SourceUse
    from sve_carddb.products.plan import OfficialProducts
    from sve_carddb.registry.preview import PreviewPlan
    from sve_carddb.registry.preview.evidence import CardEvidence
    from sve_carddb.registry.records import Region as RegistryRegion
    from sve_carddb.source_archive import ArchiveStore
    from sve_carddb.text_observations import TextPlan

NUMBER = "SYN-Ⓢ01aEN"


@dataclass
class Case:
    root: Path
    store: ArchiveStore
    batch: str
    identity: PreviewPlan
    texts: TextPlan
    official: OfficialProducts

    def report(self) -> dict[str, JsonValue]:
        return acceptance_report(
            self.texts, self.official, {self.store.store_id: self.store.root}
        )


def add_correction(store: ArchiveStore, inputs: Inputs) -> str:
    original = inputs.en[NUMBER]
    image = b"\x89PNG\r\n\x1a\nSynthetic back evidence"
    image_hash = digest(image)
    inputs.receipt.corrections = [
        Correction(
            region="en",
            card_no=NUMBER,
            face_index=1,
            field="effect",
            expected_raw_value=original.faces[1].text or "",
            corrected_value="Synthetic corrected back",
            image_sha256=image_hash,
            locator="Synthetic back effect box",
            state="active",
            reason="Synthetic typo",
        )
    ]
    proof = CorrectionEvidence(
        kind="card_image",
        region="en",
        image_src=original.faces[1].image,
        sha256=image_hash,
        locator="Synthetic back effect box",
    )
    _put(
        store,
        replace(
            _resource(evidence_url(proof), "images/en.png", image), region=Region.EN
        ),
        image,
    )
    return seal_batch(store).batch_id


def make_case(
    tmp_path: Path,
    *,
    correction: bool = False,
    auxiliary: bool = True,
    include_jp: bool = False,
) -> Case:
    root = tmp_path / "checkout/authored"
    raw = page(NUMBER, double=True)
    if not auxiliary:
        raw = b"<p>Next</p>".join(
            raw.rsplit(b"<p>Next<br>-----<br>Auxiliary<br>------<br>Last</p>", 1)
        )
    store = _store(tmp_path / "archive")
    _put(
        store,
        replace(
            _resource(card_url(NUMBER), "raw/en.html", raw, Kind.CARD), region=Region.EN
        ),
        raw,
    )
    batch = seal_batch(store).batch_id
    original = legacy_projection(extract_card(raw, number=NUMBER))
    inputs = Inputs(
        jp={"SYN-JP01": original.model_copy(update={"number": "SYN-JP01"})}
        if include_jp
        else {},
        en={NUMBER: original},
        mapping=CardMapping(targets={NUMBER: None}, original_art=set(), reskins={}),
        receipt=Receipt(
            policy="identity-init-2026-09-28-v1",
            reviewed_by="synthetic-reviewer",
            reviewed_on="2026-10-01",
            input_hashes={"en": "sha256:" + "1" * 64, "jp": "sha256:" + "2" * 64},
        ),
    )
    if correction:
        batch = add_correction(store, inputs)
    write_files(plan_files(root, build(inputs, {})))
    install(root, "products/family/SYN/001.yaml", envelope([family("SYN")]))
    identity = plan_preview(
        root,
        FrozenEN(store.root, store.store_id, batch, parser_version="en-identity-pin"),
        regions=("jp", "en") if include_jp else ("en",),
    )
    frozen = FrozenSources(store.root, store.store_id, batch)
    source, verified, _ = frozen.read(
        next(
            entry.source_version_id
            for entry in frozen.inventory.current
            if entry.url == card_url(NUMBER)
        ),
        parser_version=PARSER,
    )
    product = parse_products(verified, source, "en")
    install_identity(
        root,
        identity_envelope([identity_record(product)]),
        name="product-identities/en/001.yaml",
    )
    revision = commit(root)
    catalog = load_products(root, registry=identity.snapshot)
    identities = load_product_identities(
        root,
        authored_revision=revision,
        catalog=catalog,
        stores={store.store_id: store.root},
    )
    return Case(
        root,
        store,
        batch,
        identity,
        plan_text_observations(
            identity,
            FrozenTexts(
                store.root,
                store.store_id,
                batch,
                region="en",
                parser_version="en-text-pin",
            ),
        ),
        plan_official_products(identities, (product,), identity),
    )


@pytest.fixture(scope="module")
def case(tmp_path_factory: pytest.TempPathFactory) -> Case:
    return make_case(tmp_path_factory.mktemp("en-acceptance"))


@pytest.fixture
def mutable_case(case: Case, tmp_path: Path) -> Case:
    source = case.store.data_root.parent
    destination = tmp_path / "copied-archive"
    shutil.copytree(source, destination)
    store = replace(
        case.store,
        data_root=destination / case.store.data_root.relative_to(source),
        manifest_path=destination / case.store.manifest_path.relative_to(source),
        lock_path=destination / case.store.lock_path.relative_to(source),
        root=destination / case.store.root.relative_to(source),
    )
    return replace(case, store=store)


@pytest.mark.parametrize("auxiliary", [False, True])
def test_correction_plan_applied_and_unavailable_images_are_reported_separately(
    tmp_path: Path,
    auxiliary: bool,
) -> None:
    case = make_case(tmp_path, correction=True, auxiliary=auxiliary)
    report = case.report()
    rows = report["printings"]
    assert isinstance(rows, list)
    assert isinstance(rows[0], dict)
    correction = next(
        record.data
        for record in case.identity.snapshot.records.values()
        if isinstance(record.data, CorrectionData)
    )
    assert rows[0]["corrections"] == [
        {
            "correction_id": correction.id,
            "status": "deferred",
            "reason": "image_inputs_not_supplied",
        }
    ]
    case.texts = plan_text_observations(
        case.identity,
        FrozenTexts(
            case.store.root,
            case.store.store_id,
            case.batch,
            region="en",
            parser_version="en-text-pin",
        ),
        images=FrozenImages(case.store.root, case.store.store_id, case.batch),
    )
    applied = case.report()
    rows = applied["printings"]
    assert isinstance(rows, list)
    assert isinstance(rows[0], dict)
    corrections = rows[0]["corrections"]
    assert isinstance(corrections, list)
    assert isinstance(corrections[0], dict)
    assert corrections[0]["status"] == ("conflict" if auxiliary else "applied")
    assert applied["release_status"] == "blocked"
    counts = applied["expected_source_use_counts"]
    assert isinstance(counts, dict)
    assert counts["source_correction_comparison"] == 1
    assert counts["source_correction_evidence"] == 1
    assert b"Synthetic corrected back" not in canonical(applied)


def test_exact_full_faces_source_uses_and_history_never_authorize_release(
    case: Case, monkeypatch: pytest.MonkeyPatch
) -> None:
    before = {path: path.read_bytes() for path in case.root.rglob("*.yaml")}

    def forbidden(*_args: object, **_kwargs: object) -> None:
        raise AssertionError("No live source access")

    monkeypatch.setattr(Manifest, "open", forbidden)
    monkeypatch.setattr(Manifest, "open_live", forbidden)
    report = case.report()
    assert report["denominator"] == 1
    assert report["counts"] == {"exact": 1, "mismatch": 0, "missing_raw": 0}
    assert report["rates"] == {
        "all_registered": {"numerator": 1, "denominator": 1},
        "comparable": {"numerator": 1, "denominator": 1},
        "comparable_denominator": 1,
    }
    rows = report["printings"]
    assert isinstance(rows, list)
    assert isinstance(rows[0], dict)
    row = rows[0]
    assert row["card_no"] == NUMBER
    assert row["product_source_matches"] is True
    assert row["product_ids"] == ["permanent-example"]
    faces = row["text_faces"]
    assert isinstance(faces, list)
    assert [face["source_index"] for face in faces if isinstance(face, dict)] == [0, 1]
    assert all(isinstance(face, dict) and face["section_count"] == 2 for face in faces)
    assert report["expected_source_use_counts"] == {
        "registry_observation": 1,
        "face_text_observation": 2,
        "face_current_comparison": 2,
        "effect_presence": 2,
        "official_product_page": 1,
        "official_product": 1,
        "official_printing_product": 1,
        "product_identity_evidence_closure": 1,
        "official_product_identity": 1,
    }
    assert row["region_text_review"] == "blocked_not_supplied"
    assert report["publication_gate"] is False
    assert report["snapshot_output_authorized"] is False
    assert report["release_status"] == "blocked"
    assert (
        report["source_closure"] == "expected_inputs_only_requires_bundle_verification"
    )
    assert not report["review_queue"]
    encoded = canonical(report)
    for secret in (b"Synthetic front", b"Auxiliary", b"Synthetic universe", b"Voice"):
        assert secret not in encoded
    assert before == {path: path.read_bytes() for path in before}
    assert case.report() == report


@pytest.mark.parametrize("field", ["observation_hash", "rules_hash"])
def test_each_hash_change_queues_every_affected_decision_without_replacing_it(
    case: Case, field: str
) -> None:
    evidence = case.identity.evidence["en", NUMBER]
    changed = replace(
        evidence,
        observation=evidence.observation.model_copy(
            update={field: "sha256:" + "0" * 64}
        ),
    )
    provider = FrozenEN(
        case.store.root,
        case.store.store_id,
        case.batch,
        parser_version="en-identity-pin",
    )
    original = provider.card

    def candidate(region: RegistryRegion, card_no: str) -> CardEvidence | None:
        return (
            changed
            if (region, card_no) == ("en", NUMBER)
            else original(region, card_no)
        )

    # Only the independent evidence hash is changed, not the expected registry record.
    with pytest.MonkeyPatch.context() as patch:
        patch.setattr(provider, "card", candidate)
        identity = plan_preview(case.root, provider, regions=("en",))
    queue = review_queue(identity)
    affected = {
        projection.record_key
        for projection in identity.projections
        if any(check.status == "observation_mismatch" for check in projection.evidence)
    }
    assert {row["record_key"] for row in queue if isinstance(row, dict)} == affected
    assert queue
    decisions = {
        projection.record_key: projection.decision_id
        for projection in identity.projections
        if projection.record_key in affected
    }
    assert all(isinstance(value, str) and value for value in decisions.values())
    assert all(
        isinstance(row, dict)
        and row["decision_id"] == decisions[str(row["record_key"])]
        for row in queue
    )
    assert all(
        isinstance(row, dict)
        and row["actual_" + field] == "sha256:" + "0" * 64
        and row["expected_" + field] != row["actual_" + field]
        and row["action"] == "re_review_exact_source_without_rewriting_history"
        for row in queue
    )
    assert identity.snapshot == case.identity.snapshot
    actual_card = case.texts.observations[0].card.model_copy(
        update={"observation": changed.observation}
    )
    texts = plan_text_observations(identity, MemoryTexts({("en", NUMBER): actual_card}))
    official = plan_official_products(
        case.official.identities, case.official.pages, identity
    )
    report = acceptance_report(texts, official, {case.store.store_id: case.store.root})
    assert report["counts"] == {"exact": 0, "mismatch": 1, "missing_raw": 0}


@pytest.mark.parametrize("mutation", ["name", "back_rule", "speech", "image"])
def test_real_changed_bytes_are_mismatch_even_when_rules_remain_equal(
    mutable_case: Case, mutation: str
) -> None:
    case = mutable_case
    raw = page(NUMBER, double=True)
    if mutation == "name":
        raw = raw.replace(b"Synthetic front", b"Changed front")
    elif mutation == "back_rule":
        raw = b"Changed auxiliary".join(raw.rsplit(b"Auxiliary", 1))
    elif mutation == "speech":
        raw = raw.replace(b"Voice", b"Changed speech")
    else:
        raw = raw.replace(b"../exact/", b"../changed/")
    _put(
        case.store,
        replace(
            _resource(card_url(NUMBER), "raw/en.html", raw, Kind.CARD), region=Region.EN
        ),
        raw,
    )
    batch = seal_batch(case.store).batch_id
    identity = plan_preview(
        case.root,
        FrozenEN(
            case.store.root,
            case.store.store_id,
            batch,
            parser_version="en-identity-pin",
        ),
        regions=("en",),
    )
    texts = plan_text_observations(
        identity,
        FrozenTexts(
            case.store.root,
            case.store.store_id,
            batch,
            region="en",
            parser_version="en-text-pin",
        ),
    )
    frozen = FrozenSources(case.store.root, case.store.store_id, batch)
    source, verified, _ = frozen.read(
        frozen.inventory.current[0].source_version_id, parser_version=PARSER
    )
    official = plan_official_products(
        case.official.identities, (parse_products(verified, source, "en"),), identity
    )
    report = acceptance_report(texts, official, {case.store.store_id: case.store.root})
    assert report["counts"] == {"exact": 0, "mismatch": 1, "missing_raw": 0}
    assert report["rates"] == {
        "all_registered": {"numerator": 0, "denominator": 1},
        "comparable": {"numerator": 0, "denominator": 1},
        "comparable_denominator": 1,
    }
    assert report["review_queue"]
    assert identity.snapshot == case.identity.snapshot
    old = case.identity.evidence["en", NUMBER].observation
    new = identity.evidence["en", NUMBER].observation
    assert old.observation_hash != new.observation_hash
    if mutation == "image":
        assert old.rules_hash == new.rules_hash
    else:
        assert old.rules_hash != new.rules_hash


@pytest.fixture(scope="module")
def mixed_case(tmp_path_factory: pytest.TempPathFactory) -> Case:
    return make_case(tmp_path_factory.mktemp("en-acceptance-mixed"), include_jp=True)


def test_mixed_regions_report_counts_only_en_printings(mixed_case: Case) -> None:
    registered = [
        record.data
        for record in mixed_case.identity.snapshot.records.values()
        if isinstance(record.data, PrintingData)
    ]
    assert {printing.region for printing in registered} == {"jp", "en"}
    assert set(mixed_case.identity.regions) == {"jp", "en"}
    report = mixed_case.report()
    assert report["denominator"] == 1
    assert report["counts"] == {"exact": 1, "mismatch": 0, "missing_raw": 0}
    assert report["rates"] == {
        "all_registered": {"numerator": 1, "denominator": 1},
        "comparable": {"numerator": 1, "denominator": 1},
        "comparable_denominator": 1,
    }
    rows = report["printings"]
    assert isinstance(rows, list)
    assert [row["card_no"] for row in rows if isinstance(row, dict)] == [NUMBER]


def test_proven_absence_keeps_raw_and_projected_effect_separate(
    mutable_case: Case,
) -> None:
    case = mutable_case
    raw = presence_page("en", double=True).replace(b"SYN-01", NUMBER.encode())
    _put(
        case.store,
        replace(
            _resource(card_url(NUMBER), "raw/en.html", raw, Kind.CARD), region=Region.EN
        ),
        raw,
    )
    batch = seal_batch(case.store).batch_id
    identity = plan_preview(
        case.root,
        FrozenEN(
            case.store.root,
            case.store.store_id,
            batch,
            parser_version="en-identity-pin",
        ),
        regions=("en",),
    )
    texts = plan_text_observations(
        identity,
        FrozenTexts(
            case.store.root,
            case.store.store_id,
            batch,
            region="en",
            parser_version="en-text-pin",
        ),
    )
    card = texts.observations[0].card
    assert card.faces[0].effect is None
    assert card.effect_presence[0].result.state == "absent"
    assert card.projected(0).effect is not None
    assert not card.projected(0).effect
    official = plan_official_products(
        case.official.identities, case.official.pages, identity
    )
    report = acceptance_report(texts, official, {case.store.store_id: case.store.root})
    rows = report["printings"]
    assert isinstance(rows, list)
    assert isinstance(rows[0], dict)
    faces = rows[0]["text_faces"]
    assert isinstance(faces, list)
    assert isinstance(faces[0], dict)
    face = faces[0]
    assert face["raw_effect_present"] is False
    assert face["effect_present"] is True
    assert face["raw_content_hash"] == card.faces[0].fingerprint()
    assert face["content_hash"] == card.projected(0).fingerprint()
    assert face["raw_content_hash"] != face["content_hash"]
    assert face["effect_presence"] == card.effect_presence[0].value()
    assert report["publication_gate"] is False


@pytest.mark.parametrize("stage", ["identity", "text"])
@pytest.mark.parametrize("failure", ["parse", "archive"])
def test_invalid_frozen_source_aborts_before_success_report(
    mutable_case: Case, stage: str, failure: str
) -> None:
    case = mutable_case
    batch = case.batch
    if failure == "parse":
        raw = b"<html><!--" + b"x" * 1100 + b"--></html>"
        _put(
            case.store,
            replace(
                _resource(card_url(NUMBER), "raw/en.html", raw, Kind.CARD),
                region=Region.EN,
            ),
            raw,
        )
        batch = seal_batch(case.store).batch_id
    else:
        frozen = FrozenSources(case.store.root, case.store.store_id, batch)
        entry = frozen.inventory.current[0]
        version = frozen.entries[entry.source_version_id]
        (case.store.root / version.blob.path).write_bytes(b"Synthetic damaged blob")
    reports: list[dict[str, JsonValue]] = []
    before = {path: path.read_bytes() for path in case.root.rglob("*.yaml")}

    def compose_report() -> None:
        identity = case.identity
        if stage == "identity":
            identity = plan_preview(
                case.root,
                FrozenEN(
                    case.store.root,
                    case.store.store_id,
                    batch,
                    parser_version="en-identity-pin",
                ),
                regions=("en",),
            )
        texts = plan_text_observations(
            identity,
            FrozenTexts(
                case.store.root,
                case.store.store_id,
                batch,
                region="en",
                parser_version="en-text-pin",
            ),
        )
        official = plan_official_products(
            case.official.identities, case.official.pages, identity
        )
        reports.append(
            acceptance_report(texts, official, {case.store.store_id: case.store.root})
        )

    expected_error = MissingElementError if failure == "parse" else ArchiveError
    with pytest.raises(expected_error):
        compose_report()
    assert reports == []
    assert before == {path: path.read_bytes() for path in before}


def test_missing_source_keeps_denominator_and_ids(case: Case) -> None:
    identity = plan_preview(case.root, MemoryEvidence({}), regions=("en",))
    texts = plan_text_observations(
        identity,
        FrozenTexts(
            case.store.root,
            case.store.store_id,
            case.batch,
            region="jp",
            parser_version="jp-text-pin",
        ),
    )
    official = plan_official_products(
        case.official.identities, case.official.pages, identity
    )
    report = acceptance_report(texts, official, {case.store.store_id: case.store.root})
    assert report["denominator"] == 1
    assert report["counts"] == {"exact": 0, "mismatch": 0, "missing_raw": 1}
    assert report["rates"] == {
        "all_registered": {"numerator": 0, "denominator": 1},
        "comparable": None,
        "comparable_denominator": 0,
    }
    rows = report["printings"]
    assert isinstance(rows, list)
    assert isinstance(row := rows[0], dict)
    assert row["text_faces"] == []
    assert row["product_source_matches"] is False
    assert isinstance(face_map := row["source_face_map"], list)
    assert len(face_map) == 2
    assert identity.snapshot == case.identity.snapshot
    queue = report["review_queue"]
    assert isinstance(queue, list)
    assert queue
    assert all(
        isinstance(row, dict)
        and row["source"] is None
        and row["actual_observation_hash"] is None
        and row["actual_rules_hash"] is None
        and row["action"] == "supply_pinned_source_without_live_fallback"
        for row in queue
    )


def test_changed_product_plan_is_rejected(case: Case) -> None:
    with pytest.raises(ValueError, match="product/identity"):
        acceptance_report(
            case.texts,
            replace(case.official, inclusions=()),
            {case.store.store_id: case.store.root},
        )


def test_jp_only_plan_cannot_pass_en_acceptance(case: Case) -> None:
    texts = replace(case.texts, identity=replace(case.identity, regions=("jp",)))
    with pytest.raises(ValueError, match="explicit EN"):
        acceptance_report(texts, case.official, {case.store.store_id: case.store.root})


def test_omitted_back_face_is_rejected(case: Case) -> None:
    with pytest.raises(ValueError, match="face inventory"):
        acceptance_report(
            replace(case.texts, observations=case.texts.observations[:1]),
            case.official,
            {case.store.store_id: case.store.root},
        )


@pytest.mark.parametrize(
    "change", [None, "omit_back", "parser", "usage", "locator", "archive"]
)
def test_en_bundle_checks_each_source_use_dimension_and_never_publishes_on_failure(
    case: Case, tmp_path: Path, monkeypatch: pytest.MonkeyPatch, change: str | None
) -> None:
    vocabulary = Vocabulary(
        bindings=(
            Binding(region="en", kind="class", raw="Synthetic class", code="synthetic"),
            Binding(region="en", kind="type", raw="Synthetic type", code="synthetic"),
            Binding(region="en", kind="trait", raw="Alpha", code="alpha"),
            Binding(region="en", kind="trait", raw="Beta", code="beta"),
            Binding(
                region="en", kind="title", raw="Synthetic universe", code="synthetic"
            ),
        )
    )
    context = BuildContext.from_inputs(
        case.official.identities.revision,
        case.official.identities.dependencies()
        | {"synthetic.lock": b"Test dependencies"},
        {
            "product_identity": case.official.identities.configuration(),
            **text_configuration(case.texts, vocabulary, ()),
        },
    )
    stores = {case.store.store_id: case.store.root}
    expected = text_preview_uses(
        case.official.identities.catalog, case.texts, stores, official=case.official
    )

    def corrupt(build: BuildContext, uses: Iterable[SourceUse]) -> InputRecord:
        values = list(uses)
        use = next(
            use
            for use in values
            if use.usage == "face_text_observation" and '"face_index":1' in use.locator
        )
        values.remove(use)
        if change == "archive":
            use = use.model_copy(
                update={
                    "source": use.source.model_copy(
                        update={
                            "archive": use.source.archive.model_copy(
                                update={"batch_id": "sha256:" + "0" * 64}
                            )
                        }
                    )
                }
            )
        elif change == "parser":
            use = use.model_copy(
                update={
                    "source": use.source.model_copy(
                        update={"parser_version": "changed"}
                    )
                }
            )
        elif change in {"usage", "locator"}:
            use = use.model_copy(update={change: "changed"})
        if change != "omit_back":
            values.append(use)
        return input_record(build, values)

    if change is not None:
        monkeypatch.setattr(importer, "input_record", corrupt)

    def populate(db: Database) -> InputRecord:
        return populate_text_preview(
            db,
            case.official.identities.catalog,
            case.texts,
            authored_revision=case.official.identities.revision,
            build=context,
            vocabulary=vocabulary,
            published=(),
            languages=(
                Language(code="en", fallback_order=(), display_name="English"),
                Language(code="ja", fallback_order=(), display_name="Japanese"),
            ),
            stores=stores,
            official=case.official,
        )

    destination = tmp_path / "bundle"
    schema = compile_build(("en",))
    if change is None:
        record = publish_bundle(
            schema,
            destination,
            context,
            expected,
            populate,
            case.report(),
            stores=stores,
        )
        assert (
            verify_bundle(schema, destination, context, expected, stores=stores)
            == record
        )
        assert b"Synthetic front" not in (destination / "report.json").read_bytes()
    else:
        with pytest.raises(ValueError, match="use closure"):
            publish_bundle(
                schema,
                destination,
                context,
                expected,
                populate,
                case.report(),
                stores=stores,
            )
        assert not destination.exists()
