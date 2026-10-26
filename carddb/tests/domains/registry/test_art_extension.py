"""Explicit same-image use extension preserves old art and pins every frozen member."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.core.json import canonical
from sve_carddb.domains import physical_sources
from sve_carddb.domains.registry.preview import import_preview, plan_preview
from sve_carddb.domains.registry.records import ArtData, ArtUse, PrintingData
from sve_carddb.domains.registry.snapshot import load_registry
from sve_carddb.domains.registry.storage import Entry, Shard
from sve_carddb.domains.registry.transitions.renewal import load_renewed_registry
from sve_carddb.domains.registry.transitions.writer import (
    ArtDecisions,
    Recipe,
    plan,
    write,
)
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.ingest.archive.manifest import Kind
from sve_carddb.ingest.archive.source_archive import (
    ArchiveError,
    ArchiveStore,
    Scope,
    seal_batch,
)
from sve_carddb.parse.pages import official_jp

from ...ingest.test_source_archive import _put, _resource
from ...support.adoption_fixtures import commit
from ...support.registry_preview_fixtures import BUILD, parents
from .test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- dependency of case
from .test_registry_preview_en_sources import frozen_registry as frozen_registry  # ruff: ignore[useless-import-alias] -- dependency of case
from .test_review_renewal import case as case  # ruff: ignore[useless-import-alias] -- shared minimal Git and sealed source fixture

if TYPE_CHECKING:
    from pathlib import Path

    from .test_review_renewal import Case


@dataclass(frozen=True)
class ArtCase:
    recipe: Recipe
    decisions: ArtDecisions
    original: ArtData
    base: Case


@pytest.fixture
def art_case(case: Case, tmp_path: Path) -> ArtCase:
    snapshot = load_registry(case.authored)
    choices = [
        r.data
        for r in snapshot.records.values()
        if isinstance(r.data, PrintingData) and r.data.region == "jp"
    ]
    left, right = choices
    assert left.card_id == right.card_id
    original = ArtData(
        id="a:" + "9" * 32,
        card_id=left.card_id,
        face_id=left.source_face_map[0].face_id,
        classification="unclassified",
        uses=(ArtUse(printing_id=left.id, face_id=left.source_face_map[0].face_id),),
        observation=left.observation,
    )
    path = case.authored / "registry/art/TEST/001.yaml"
    path.parent.mkdir(parents=True)
    path.write_bytes(
        canonical(
            Shard(
                format=1,
                records=[
                    Entry(
                        kind="art", owner="TEST", data=original.model_dump(mode="json")
                    )
                ],
            ).model_dump(mode="json", round_trip=True)
        )
    )
    commit(case.recipe.repo)
    store = ArchiveStore(
        tmp_path / "data",
        tmp_path / "data/manifest/manifest.sqlite",
        tmp_path / "data/manifest/.lock",
        case.recipe.archive,
        case.recipe.store_id,
    )
    sources = FrozenSources(
        case.recipe.archive, case.recipe.store_id, case.provider.jp.batch_id
    )
    for printing in (left, right):
        found = case.provider.card("jp", printing.card_no)
        assert found is not None
        page = physical_sources.page(
            sources, found.source.id, region="jp", parser_version="jp-pin"
        ).record.get()
        url = official_jp.image_url(page.faces[0].image)
        raw = b"\x89PNG\r\n\x1a\n" + b"synthetic same art" * 20
        _put(
            store,
            _resource(url, f"images/{printing.card_no}.png", raw, Kind.IMAGE),
            raw,
        )

    images = seal_batch(store, scope=[Scope(provider="jp", kind="image")])
    recipe = case.recipe.model_copy(
        update={
            "sources": tuple(
                s.model_copy(
                    update={
                        "image_batch": images.batch_id if s.region == "jp" else None
                    }
                )
                for s in case.recipe.sources
            )
        }
    )
    extended = original.model_copy(
        update={
            "uses": (
                *original.uses,
                ArtUse(printing_id=right.id, face_id=right.source_face_map[0].face_id),
            )
        }
    )
    return ArtCase(
        recipe,
        ArtDecisions(arts=(extended,), reason="Synthetic manually reviewed same image"),
        original,
        case,
    )


def test_complete_art_extension_preserves_original_bytes_and_projects_both_uses(
    art_case: ArtCase,
) -> None:
    root = art_case.base.authored
    before = {p: p.read_bytes() for p in root.rglob("*.yaml")}
    planned = plan(art_case.recipe, art_case.decisions)
    write(root, planned)
    assert all(path.read_bytes() == raw for path, raw in before.items())
    snapshot = load_renewed_registry(root, art_case.recipe.evidence())
    updated = snapshot.records["art:" + art_case.original.id].data
    assert isinstance(updated, ArtData)
    assert len(updated.uses) == 2
    assert updated.observation == art_case.original.observation
    assert {use.usage for use in snapshot.transition_uses} == {
        "identity_transition_art",
        "identity_transition_art_image",
    }
    projected = plan_preview(
        root, art_case.base.provider, regions=("en", "jp"), snapshot=snapshot
    )
    with create_database(compile_build(("en", "related"))) as db:
        parents(db, projected)
        import_preview(
            db,
            projected,
            authored_revision=art_case.recipe.program_revision,
            build=BUILD,
        )
        assert (
            sum(
                row.values["art_id"] == art_case.original.id
                for row in db.rows("printing_face")
            )
            == 2
        )
        assert not db.rows("identity_change")
    assert plan(art_case.recipe, art_case.decisions).shard is None


@pytest.mark.parametrize("field", ["classification", "card_id", "observation", "uses"])
def test_art_extension_cannot_rewrite_old_adoption(
    art_case: ArtCase, field: str
) -> None:
    after = art_case.decisions.arts[0]
    if field == "classification":
        after = after.model_copy(update={field: "base"})
    elif field == "card_id":
        after = after.model_copy(update={field: "c:" + "0" * 32})
    elif field == "observation":
        after = after.model_copy(
            update={
                field: after.observation.model_copy(
                    update={"rules_hash": "sha256:" + "0" * 64}
                )
            }
        )
    else:
        after = after.model_copy(update={field: (after.uses[-1],)})
    decisions = art_case.decisions.model_copy(update={"arts": (after,)})
    with pytest.raises(ValueError, match=r"Art|art|observation"):
        plan(art_case.recipe, decisions)
    assert not (art_case.base.authored / "identity-transitions").exists()


def test_art_extension_requires_original_image_closure(art_case: ArtCase) -> None:
    recipe = art_case.recipe.model_copy(
        update={
            "sources": tuple(
                s.model_copy(update={"image_batch": None})
                for s in art_case.recipe.sources
            )
        }
    )
    with pytest.raises(ValueError, match="requires frozen image batch"):
        plan(recipe, art_case.decisions)


def test_art_extension_cannot_claim_a_use_already_adopted_by_another_art(
    art_case: ArtCase,
) -> None:
    target = art_case.decisions.arts[0].uses[-1]
    other = art_case.original.model_copy(
        update={"id": "a:" + "8" * 32, "uses": (target,)}
    )
    # Keep this adoption valid so the overlapping use is the only rejection.
    snapshot = load_registry(art_case.base.authored)
    printing = snapshot.records["printing:" + target.printing_id].data
    assert isinstance(printing, PrintingData)
    other = other.model_copy(update={"observation": printing.observation})
    path = art_case.base.authored / "registry/art/TEST/002.yaml"
    path.write_bytes(
        canonical(
            Shard(
                format=1,
                records=[
                    Entry(kind="art", owner="TEST", data=other.model_dump(mode="json"))
                ],
            ).model_dump(mode="json", round_trip=True)
        )
    )
    commit(art_case.recipe.repo)
    with pytest.raises(ValueError, match="Multiple adopted art groups"):
        plan(art_case.recipe, art_case.decisions)


def test_changed_original_bitmap_is_rejected(art_case: ArtCase) -> None:
    pin = next(s for s in art_case.recipe.sources if s.region == "jp")
    assert pin.image_batch is not None
    sources = FrozenSources(
        art_case.recipe.archive, art_case.recipe.store_id, pin.image_batch
    )
    entry = next(iter(sources.entries.values()))
    (art_case.recipe.archive / entry.blob.path).write_bytes(b"CHANGED SYNTHETIC BITMAP")
    with pytest.raises(ArchiveError, match=r"hash|bytes|size"):
        plan(art_case.recipe, art_case.decisions)
