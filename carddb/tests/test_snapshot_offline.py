"""Small regional composition counterexamples, sharing sealed immutable baselines."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from typer.testing import CliRunner

from sve_carddb.build_db.t1 import compile_minimum
from sve_carddb.build_inputs import InputRecord, SourceUse, input_record
from sve_carddb.card_extras import (
    CardPage,
    ErrataChange,
    ErrataPage,
    ErrataPrinting,
    QAEntry,
    RelatedLink,
    populate_card_extras,
)
from sve_carddb.card_extras.archive import EN_PARSER, PARSER
from sve_carddb.card_extras.importer import CardExtrasRestriction
from sve_carddb.catalog import adoption_importer
from sve_carddb.catalog.models import Catalog
from sve_carddb.catalog.projection import CatalogProjection
from sve_carddb.cli import app
from sve_carddb.products import OfficialProducts, ProductIdentities
from sve_carddb.registry.records import PrintingData
from sve_carddb.snapshot import offline
from sve_carddb.snapshot.export import export_snapshot
from sve_carddb.snapshot.offline import (
    Inputs,
    RegionalInput,
    build,
    require_offline_coverage,
)
from sve_carddb.snapshot.preview import Roots, require_unknown_coverage, write_preview
from sve_carddb.snapshot.project import project
from sve_carddb.snapshot.publication import require_preview
from sve_carddb.snapshot.reader import read_snapshot
from sve_carddb.snapshot.values import array, canonical, digest, object_value

from .adoption_fixtures import REPO
from .catalog_vocabulary_fixtures import make_vocabulary_case
from .test_snapshot_export import exported as exported  # ruff: ignore[useless-import-alias] -- register shared export fixture
from .test_snapshot_preview import EmptySources, prepare_build
from .test_snapshot_preview import logical as logical  # ruff: ignore[useless-import-alias] -- register the export fixture parent
from .text_observation_fixtures import LANGUAGES

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import Database
    from sve_carddb.build_inputs import BuildContext
    from sve_carddb.card_extras import ExtrasPlan
    from sve_carddb.snapshot.export import Snapshot
    from sve_carddb.snapshot.project import Decisions, Projection, Settings

    from .catalog_vocabulary_fixtures import VocabularyCase
    from .shared_case_fixtures import TextCaseTemplate
    from .text_observation_fixtures import Case


@pytest.fixture
def prepared(
    default_text_case: TextCaseTemplate, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> tuple[Case, Inputs, tuple[CardPage, ...]]:
    case = default_text_case.copy(tmp_path / "synthetic")
    original = prepare_build(case, tmp_path, monkeypatch)
    for name in (
        "carddb/pyproject.toml",
        "carddb/src/sve_carddb/extract/official_jp.py",
        "carddb/src/sve_carddb/extract/official_en.py",
        "carddb/src/sve_carddb/snapshot/values.py",
    ):
        target = original.repo / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes((REPO / name).read_bytes())
    pins = tuple(
        RegionalInput(
            region=region,
            card_batch=next(
                card.source.archive.batch_id
                for (code, _), card in case.provider.cards.items()
                if code == region
            ),
            image_batch=original.image_batch,
            parser_version="synthetic-v1",
        )
        for region in ("en", "jp")
    )
    recipe = Inputs(
        **original.model_dump(
            exclude={
                "card_batch",
                "image_batch",
                "parser_version",
                "vocabulary",
                "languages",
            }
        ),
        sources=pins,
    )
    original.vocabulary.unlink()

    class AdoptedInputs:
        def configuration(self) -> dict[str, str]:
            return {"synthetic_adoptions": "immutable-receipts"}

    monkeypatch.setattr(
        adoption_importer, "AdoptionInputs", lambda *_args, **_kwargs: AdoptedInputs()
    )
    monkeypatch.setattr(
        adoption_importer,
        "derive_catalog",
        lambda *_args, **_kwargs: CatalogProjection(
            Catalog(
                languages=LANGUAGES,
                terms=(),
                aliases=(),
                symbols=(),
                normalizer_version="nfkc-casefold-v1",
            ),
            case.vocabulary,
        ),
    )
    adoption_uses = (
        SourceUse(
            source=next(iter(case.provider.cards.values())).source,
            usage="catalog_exact_text",
            locator="synthetic-adoption-field",
        ),
    )
    monkeypatch.setattr(offline, "_adoption_uses", lambda *_args: adoption_uses)
    monkeypatch.setattr(
        adoption_importer,
        "populate_adoptions",
        lambda _db, _inputs, *, build, **_kwargs: input_record(build, adoption_uses),
    )
    identities = ProductIdentities(
        recipe.revision, digest(b"{}"), b"{}", (), {}, {}, (), case.catalog
    )
    monkeypatch.setattr(
        offline, "plan_preview", lambda *_args, **_kwargs: case.identity
    )
    monkeypatch.setattr(
        offline, "plan_text_observations", lambda *_args, **_kwargs: case.plan
    )
    monkeypatch.setattr(
        offline, "load_products", lambda *_args, **_kwargs: case.catalog
    )
    monkeypatch.setattr(
        offline, "load_product_identities", lambda *_args, **_kwargs: identities
    )
    for name in (
        "FrozenJP",
        "FrozenEN",
        "FrozenRegions",
        "FrozenTexts",
        "FrozenProducts",
        "RegionalImages",
    ):
        monkeypatch.setattr(offline, name, lambda *_args, **_kwargs: EmptySources())
    monkeypatch.setattr(
        offline,
        "plan_official_products",
        lambda *_args, **_kwargs: OfficialProducts(
            identities, (), (), (), (), case.identity
        ),
    )
    pages = tuple(
        CardPage(
            source=card.source.model_copy(
                update={"parser_version": PARSER if region == "jp" else EN_PARSER}
            ),
            region=region,
            card_no=number,
            qa=(
                QAEntry(
                    stable_source_key="Q900000",
                    official_number="Q900000",
                    locator="qa-block:0",
                    question="Synthetic regional question?",
                    answer="Synthetic answer.",
                    published_on=None,
                ),
            ),
            related=(
                RelatedLink(
                    locator="related-link:0",
                    href_raw="?cardno="
                    + ("GF01-001EN" if number != "GF01-001EN" else "BP02-070EN"),
                ),
            ),
        )
        for (region, number), card in sorted(case.provider.cards.items())
    )

    class Pages:
        def __init__(self, *_args: object, region: str) -> None:
            self.region = region

        def pages(self) -> tuple[CardPage, ...]:
            return tuple(page for page in pages if page.region == self.region)

    monkeypatch.setattr(offline, "FrozenCardExtras", Pages)
    return case, recipe, pages


def test_regional_build_projects_qa_related_and_reskin_with_complete_sources(
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]], tmp_path: Path
) -> None:
    case, recipe, pages = prepared
    built = build(recipe, bundle_dir=tmp_path / "bundle")
    tables = built.projection.tables
    assert {row["region"] for row in tables["printing"]} == {"en", "jp"}
    assert len(tables["qa"]) == 2
    assert {(row["region"], row["official_number"]) for row in tables["qa"]} == {
        ("jp", "Q900000"),
        ("en", "Q900000"),
    }
    assert all(row["published_on"] is None for row in tables["qa_version"])
    assert len(tables["qa_version"]) == 2
    assert all(array(row["cards"]) for row in tables["qa_version"])
    assert any(
        row["relation"] == "official_unspecified" for row in tables["card_related"]
    )
    reskin = [
        row for row in tables["card_related"] if row["relation"] == "same_rules_reskin"
    ]
    assert len(reskin) == 1
    assert reskin[0]["applicable_regions"] == ["en"]
    record = InputRecord.model_validate_json(built.input_content)
    assert {
        use.source.id for use in record.uses if use.usage == "card_extras_page"
    } == {page.source.id for page in pages}
    assert sum(use.usage == "official_qa" for use in record.uses) == len(pages)
    assert {use.source.id for use in case.plan.source_uses()} <= {
        use.source.id for use in record.uses
    }
    assert {path.name for path in (tmp_path / "bundle").iterdir()} == {
        "build.sqlite",
        "inputs.json",
        "report.json",
        "seal.json",
    }
    assert (tmp_path / "bundle/inputs.json").read_bytes() == built.input_content
    assert "Synthetic answer." not in canonical(built.report).decode()
    assert tables["errata"] == []
    assert built.projection.metadata["errata_card_ids"] == []
    assert built.projection.metadata["source_windows"] == []
    assert built.projection.metadata["restriction_coverage"] == []
    snapshot = export_snapshot(built.projection, built.ownership, recipe.batch())
    joined = read_snapshot(
        snapshot.manifest, {key: blob.raw for key, blob in snapshot.payloads.items()}
    )
    assert joined == tables
    report = write_preview(
        snapshot,
        Roots(tmp_path / "preview", tmp_path / "formal"),
        built.report,
        regions=("en", "jp"),
    )
    assert report["pointer"]
    assert not (tmp_path / "formal").exists()
    with pytest.raises(ValueError, match=r"^Preview requires exactly the JP region$"):
        require_preview(snapshot.manifest)
    with pytest.raises(
        ValueError, match=r"^Unrequested ancillary sources cannot become public facts$"
    ):
        require_unknown_coverage(built.projection)


def test_errata_pending_keeps_cards_routes_and_existing_current(
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]], monkeypatch: pytest.MonkeyPatch
) -> None:
    case, recipe, pages = prepared
    en_page = next(page for page in pages if page.card_no == "BP02-070EN")
    reference = "https://en.shadowverse-evolve.com/errata/synthetic/"
    changed = en_page.model_copy(update={"errata_urls": (reference,)})
    monkeypatch.setattr(
        offline,
        "FrozenCardExtras",
        lambda *_args, **kwargs: PageSource(
            tuple(
                changed if page == en_page else page
                for page in pages
                if page.region == kwargs["region"]
            )
        ),
    )
    built = build(recipe)
    tables = built.projection.tables
    card_id = next(
        row.data.card_id
        for row in case.identity.included("printing")
        if isinstance(row.data, PrintingData) and row.data.card_no == en_page.card_no
    )
    assert any(row["id"] == card_id for row in tables["card"])
    assert any(
        object_value(current)["region"] == "en"
        for face in tables["face"]
        if face["card_id"] == card_id
        for current in array(face["current"])
    )
    assert any(row["card_id"] == card_id for row in tables["printing"])
    card = next(row for row in tables["card"] if row["id"] == card_id)
    assert any(
        object_value(region)["region"] == "en" for region in array(card["regions"])
    )
    assert any(row["card_no"] == en_page.card_no for row in tables["printing"])
    support = next(
        row for row in tables["card_engine_support"] if row["card_id"] == card_id
    )
    blocks = {
        object_value(raw)["region"]: array(object_value(raw)["reasons"])
        for raw in array(support["region_blocks"])
    }
    assert "errata_current_pending" in blocks["en"]
    assert "errata_source_missing" in blocks["en"]
    assert "errata_current_pending" not in blocks.get("jp", [])
    assert "region_text_unreviewed" in blocks["en"]
    assert any(
        object_value(item)["target"] == reference
        for item in array(object_value(built.report["card_extras"])["staging"])
    )
    assert tables["errata"] == []


class PageSource:
    def __init__(self, pages: tuple[CardPage, ...]) -> None:
        self.items = pages

    def pages(self) -> tuple[CardPage, ...]:
        return self.items


def test_errata_fragments_dates_and_multiple_notices_remain_independent(
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]],
) -> None:
    case, recipe, pages = prepared
    page = next(page for page in pages if page.region == "en")
    face = next(
        item.face_id for item in case.plan.observations if item.card_no == page.card_no
    )
    notices = tuple(
        ErrataPage(
            source=page.source.model_copy(
                update={
                    "id": "src:v1:" + digest(str(index).encode())[7:],
                    "url": "https://en.shadowverse-evolve.com/errata/synthetic-"
                    + str(index)
                    + "/",
                }
            ),
            official_url="https://en.shadowverse-evolve.com/errata/synthetic-"
            + str(index)
            + "/",
            region="en",
            announced_on="2026-10-02" if index == 0 else None,
            changes=(
                ErrataChange(
                    card_no=page.card_no,
                    face_id=face,
                    field="effect",
                    before_value="Synthetic wrong fragment",
                    after_value="Synthetic corrected fragment",
                    locator="change:0",
                ),
            ),
            printings=(ErrataPrinting(card_no=page.card_no),),
        )
        for index in range(2)
    )
    built = build(recipe, errata=notices)
    errata = built.projection.tables["errata"]
    assert len(errata) == 2
    versions = [
        object_value(version)
        for notice in errata
        for version in array(notice["versions"])
    ]
    assert {version["announced_on"] for version in versions} == {"2026-10-02", None}
    assert all(
        object_value(array(version["changes"])[0])["before"]
        == "Synthetic wrong fragment"
        for version in versions
    )
    assert all(
        object_value(array(version["changes"])[0])["after"]
        == "Synthetic corrected fragment"
        for version in versions
    )
    assert len(array(built.projection.metadata["errata_card_ids"])) == 1
    assert built.report["supplemental_restrictions"]
    export_snapshot(built.projection, built.ownership, recipe.batch()).verify(
        built.projection
    )


@pytest.mark.parametrize(
    "usage", ["official_qa", "card_extras_page", "official_related"]
)
def test_missing_returned_use_is_rejected(
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]],
    monkeypatch: pytest.MonkeyPatch,
    usage: str,
) -> None:
    def incomplete(
        db: Database, plan: ExtrasPlan, *, build: BuildContext
    ) -> InputRecord:
        record = populate_card_extras(db, plan, build=build)
        return record.model_copy(
            update={"uses": tuple(use for use in record.uses if use.usage != usage)}
        )

    monkeypatch.setattr(offline, "populate_card_extras", incomplete)
    with pytest.raises(
        ValueError, match=r"^Build input use closure or context mismatch$"
    ):
        build(prepared[1])


@pytest.mark.parametrize(
    "field",
    ["source_windows", "restriction_coverage", "cr_version", "restriction", "errata"],
)
def test_offline_coverage_remains_unknown(
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]], field: str
) -> None:
    projection = build(prepared[1]).projection
    if field in {"source_windows", "restriction_coverage"}:
        poisoned = replace(
            projection, metadata=projection.metadata | {field: [{"state": "complete"}]}
        )
        message = "Uncovered sources must remain empty windows / unknown"
    else:
        poisoned = replace(
            projection, tables=projection.tables | {field: [{"id": "invented"}]}
        )
        message = "Unrequested ancillary sources cannot become public facts"
    with pytest.raises(ValueError, match=r"^" + message + "$"):
        require_offline_coverage(poisoned, errata=False)


def test_offline_cli_writes_private_bundle_and_dual_preview(
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]],
    tmp_path: Path,
) -> None:
    recipe = prepared[1]
    path = tmp_path / "inputs.json"
    path.write_bytes(canonical(recipe.model_dump(mode="json")))
    result = CliRunner().invoke(
        app,
        [
            "snapshot",
            "export-offline",
            "--inputs",
            str(path),
            "--preview-dir",
            str(tmp_path / "preview"),
            "--cdn-dir",
            str(tmp_path / "formal"),
            "--bundle-dir",
            str(tmp_path / "bundle"),
        ],
    )
    assert result.exit_code == 0, result.exception
    assert (tmp_path / "bundle/build.sqlite").is_file()
    assert (tmp_path / "preview/snapshots/preview/current.json").is_file()
    assert not (tmp_path / "formal").exists()


@pytest.mark.parametrize("pins", [(), ("jp",), ("jp", "en"), ("en", "en")])
def test_offline_requires_explicit_dual_region_pins(
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]], pins: tuple[str, ...]
) -> None:
    recipe = prepared[1]
    values = recipe.model_dump(mode="json")
    values["sources"] = [
        recipe.sources[0].model_dump(mode="json") | {"region": region}
        for region in pins
    ]
    with pytest.raises(
        ValueError, match="Offline launch inputs require sorted EN and JP pins"
    ):
        Inputs.model_validate_json(canonical(values))


@pytest.mark.parametrize("field", ["data_version"])
def test_offline_rejects_formal_version_or_missing_language_before_io(
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]], field: str
) -> None:
    values = prepared[1].model_dump(mode="json")
    values[field] = "20261002T010203Z-0001" if field == "data_version" else []
    message = (
        "Offline composition requires a preview- data version"
        if field == "data_version"
        else "Offline launch requires adopted EN and JA languages"
    )
    with pytest.raises(ValueError, match=message):
        Inputs.model_validate_json(canonical(values))


@pytest.mark.parametrize("protected", ["repo", "archive"])
def test_api_bundle_cannot_write_inside_immutable_input(
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]], protected: str
) -> None:
    recipe = prepared[1]
    root = getattr(recipe, protected)
    with pytest.raises(
        ValueError, match=r"^Offline bundle must be disjoint from immutable inputs$"
    ):
        build(recipe, bundle_dir=root)


@pytest.mark.parametrize("protected", ["repo", "archive", "formal"])
def test_cli_cannot_write_bundle_into_protected_roots(
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]],
    tmp_path: Path,
    protected: str,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    from sve_carddb.snapshot.preview import commands  # ruff: ignore[import-outside-top-level] -- spy only on this CLI boundary

    recipe = prepared[1]
    path = tmp_path / "inputs.json"
    path.write_bytes(canonical(recipe.model_dump(mode="json")))
    formal = tmp_path / "formal"
    target = formal if protected == "formal" else getattr(recipe, protected)

    def forbidden(*_args: object, **_kwargs: object) -> None:
        raise AssertionError("No build before output validation")

    monkeypatch.setattr(commands, "build_offline", forbidden)
    result = CliRunner().invoke(
        app,
        [
            "snapshot",
            "export-offline",
            "--inputs",
            str(path),
            "--preview-dir",
            str(tmp_path / "preview"),
            "--cdn-dir",
            str(formal),
            "--bundle-dir",
            str(target),
        ],
    )
    assert isinstance(result.exception, ValueError)
    assert (
        str(result.exception)
        == "Offline bundle must be disjoint from protected inputs and formal output"
    )
    assert not (tmp_path / "preview").exists()


@pytest.mark.parametrize(
    "regions", [(), ("en",), ("jp", "en"), ("jp", "jp"), ("en", "jp", "en")]
)
def test_unsupported_preview_region_combination_is_rejected(
    exported: Snapshot, regions: tuple[str, ...]
) -> None:
    with pytest.raises(ValueError, match=r"^Unsupported preview recipe regions$"):
        require_preview(exported.manifest, regions=regions)


def test_unknown_supplemental_reason_fails_in_projection(
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]], monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(
        offline,
        "require_card_extras_ready",
        lambda *_args, **_kwargs: (
            CardExtrasRestriction("en", "card", "card", (), "invented", "issue"),
        ),
    )
    with pytest.raises(ValueError, match=r"^Unknown supplemental restriction reason$"):
        build(prepared[1])


@pytest.mark.parametrize(
    "field",
    ["source_windows", "restriction_coverage", "cr_version", "restriction", "errata"],
)
def test_composer_checks_coverage_before_returning_or_publishing(
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]],
    field: str,
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
) -> None:
    def poisoned(
        db: Database,
        *,
        regions: tuple[str, ...],
        as_of: str,
        settings: Settings,
        decisions: Decisions,
        publication_printings: frozenset[str] | None = None,
    ) -> Projection:
        projection = project(
            db,
            regions=regions,
            as_of=as_of,
            settings=settings,
            decisions=decisions,
            publication_printings=publication_printings,
        )
        if field in {"source_windows", "restriction_coverage"}:
            return replace(
                projection,
                metadata=projection.metadata | {field: [{"state": "complete"}]},
            )
        return replace(
            projection, tables=projection.tables | {field: [{"id": "invented"}]}
        )

    monkeypatch.setattr(offline, "project", poisoned)
    message = (
        "Uncovered sources must remain empty windows / unknown"
        if field in {"source_windows", "restriction_coverage"}
        else "Unrequested ancillary sources cannot become public facts"
    )
    output = tmp_path / "bundle"
    with pytest.raises(ValueError, match=r"^" + message + "$"):
        build(prepared[1], bundle_dir=output)
    assert not output.exists()


def test_cli_preview_cannot_contain_its_recipe(
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]],
    tmp_path: Path,
) -> None:
    path = tmp_path / "preview" / "inputs.json"
    path.parent.mkdir()
    content = canonical(prepared[1].model_dump(mode="json"))
    path.write_bytes(content)
    result = CliRunner().invoke(
        app,
        [
            "snapshot",
            "export-offline",
            "--inputs",
            str(path),
            "--preview-dir",
            str(path.parent),
            "--cdn-dir",
            str(tmp_path / "formal"),
            "--bundle-dir",
            str(tmp_path / "bundle"),
        ],
    )
    assert isinstance(result.exception, ValueError)
    assert str(result.exception) == "Offline preview must be disjoint from recipe"
    assert path.read_bytes() == content
    assert not (tmp_path / "bundle").exists()


def test_recipe_cannot_supply_private_vocabulary_or_languages(
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]],
) -> None:
    for name in ("vocabulary", "languages"):
        values = prepared[1].model_dump(mode="json") | {name: "invented"}
        with pytest.raises(ValueError, match="Extra inputs are not permitted"):
            Inputs.model_validate_json(canonical(values))


def test_missing_adopted_language_fails_before_population(
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(
        adoption_importer,
        "derive_catalog",
        lambda *_args, **_kwargs: CatalogProjection(
            Catalog(
                languages=(LANGUAGES[0],),
                terms=(),
                aliases=(),
                symbols=(),
                normalizer_version="nfkc-casefold-v1",
            ),
            prepared[0].vocabulary,
        ),
    )
    with pytest.raises(
        ValueError, match=r"^Offline launch requires adopted EN and JA languages$"
    ):
        build(prepared[1])


def test_lost_adoption_evidence_is_rejected_by_complete_closure(
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(
        adoption_importer,
        "populate_adoptions",
        lambda _db, _inputs, *, build, **_kwargs: input_record(build, ()),
    )
    with pytest.raises(
        ValueError, match=r"^Build input use closure or context mismatch$"
    ):
        build(prepared[1])


@pytest.fixture(scope="module")
def immutable_adoptions(tmp_path_factory: pytest.TempPathFactory) -> VocabularyCase:
    return make_vocabulary_case(tmp_path_factory.mktemp("offline-adoptions"))


def test_offline_adapter_uses_real_checked_bilingual_adoptions(
    immutable_adoptions: VocabularyCase,
) -> None:
    case = immutable_adoptions
    derived = offline._derive_adoptions(
        case.case.inputs(),
        compile_minimum(include_en=True),
        case.build(),
        {"test-store": case.archive},
    )
    assert {item.code for item in derived.catalog.languages} == {"en", "ja", "zh-Hant"}
    for region in ("en", "jp"):
        binding = derived.vocabulary.lookup(region, "type", "Synthetic type")
        assert binding.code == "follower"
        assert binding.special_kinds == ("evolve",)
    uses = offline._adoption_uses(case.case.inputs(), {"test-store": case.archive})
    assert uses
    assert {use.usage for use in uses} == {"catalog_exact_text"}
    assert {use.source.url for use in uses} == {
        "https://example.invalid/en",
        "https://example.invalid/jp",
    }
    assert derived.catalog.terms[0].label.text != "Synthetic type"


def test_adoption_adapter_rejects_an_unpinned_configuration(
    immutable_adoptions: VocabularyCase,
) -> None:
    case = immutable_adoptions
    build = case.build().model_copy(update={"configuration": "{}"})
    with pytest.raises(
        ValueError, match=r"^Build configuration does not pin adoption inputs$"
    ):
        offline._derive_adoptions(
            case.case.inputs(),
            compile_minimum(include_en=True),
            build,
            {"test-store": case.archive},
        )
