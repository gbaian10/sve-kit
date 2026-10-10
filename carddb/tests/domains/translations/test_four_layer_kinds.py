"""Name and token-trait kind evidence is independent of the written classifier."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.domains.catalog.models import Term as VocabularyTerm
from sve_carddb.domains.products.models import LocalizedText
from sve_carddb.domains.text_observations.vocabulary import Binding, Vocabulary
from sve_carddb.domains.translations.four_layer_classification import Term
from sve_carddb.domains.translations.four_layer_kinds import KindFacts, adopted_kinds
from sve_carddb.domains.translations.four_layer_normalizer import normalize_source
from sve_carddb.domains.translations.inputs import load_glossary

from ...support.digital_link_import_fixtures import make_fixture
from ...support.translation_fixtures import envelope, template, term, write
from .test_four_layer_classification import classifier, source

if TYPE_CHECKING:
    from pathlib import Path


@pytest.mark.parametrize("kind", [None, "follower", "amulet", "unregistered"])
def test_named_field_count_requires_independent_kind_evidence(kind: str | None) -> None:
    name = Term("term:name.synthetic_kind", "card_name", "仮名")
    raw = "仮。自分の場の『仮名』２体を選ぶ。"
    engine = classifier((name,))
    if kind is not None:
        engine.references.kind_facts = KindFacts(named=((name.id, kind),))
    field = normalize_source(raw, source(raw))
    part = field.parts[0]
    found = engine.recognize(raw, field.source, part)
    assert bool(found.issues) is (kind != "follower")
    if kind == "follower":
        frame, binding = found.bind(field.source, part)
        engine.verify(raw, field, part, frame, binding)
        assert frame.leaf_schema.slots[-1].role == "selection_count"
        assert all(s.type != "CardKind" for s in frame.leaf_schema.slots)
        engine.references.kind_facts = KindFacts(named=((name.id, "amulet"),))
        with pytest.raises(ValueError, match="Unresolved source leaves"):
            engine.verify(raw, field, part, frame, binding)
    if kind == "amulet":
        assert "source_unit_mismatch" in found.issues


@pytest.mark.parametrize(
    "kinds", [(), ("follower",), ("amulet",), ("follower", "amulet")]
)
def test_token_trait_keeps_empty_or_heterogeneous_definitions_unresolved(
    kinds: tuple[str, ...],
) -> None:
    engine = classifier()
    engine.references.vocabulary = Vocabulary(
        bindings=(Binding(region="jp", kind="trait", code="synthetic", raw="仮族"),),
        terms=(
            VocabularyTerm(
                kind="trait",
                code="synthetic",
                active=True,
                label=LocalizedText(lang="ja", text="仮族"),
            ),
        ),
    )
    engine.references.kind_facts = KindFacts(token_traits=(("synthetic", kinds),))
    raw = "仮。自分の場の仮族・トークン２体を選ぶ。"
    field = normalize_source(raw, source(raw))
    part = field.parts[0]
    found = engine.recognize(raw, field.source, part)
    assert bool(found.issues) is (kinds != ("follower",))
    if kinds == ("follower",):
        frame, binding = found.bind(field.source, part)
        engine.verify(raw, field, part, frame, binding)
        context = engine.references.kind_facts.context(
            raw, part, part.canonical_source.index("N"), engine.references
        )
        assert context is not None
        assert context.token is True


def test_adopted_named_kind_uses_its_own_frozen_definition(tmp_path: Path) -> None:
    fixture = make_fixture(tmp_path, dual=True)
    record = term("name.synthetic_kind", category="card_name")
    data = record["data"]
    assert isinstance(data, dict)
    data["source_ref"] = fixture.jp.model_dump(mode="json")
    root = tmp_path / "authored"
    write(root, {"translations/glossary/defs/000.yaml": envelope([record])})
    snapshot = load_glossary(root)
    with template().copy() as db:
        fixture.publish(db)
        facts = adopted_kinds(db, snapshot, fixture.sources())
        assert facts.named == (("term:name.synthetic_kind", "follower"),)
        revision = next(
            r.values
            for r in db.rows("face_revision")
            if r.values["id"] == "link-revision"
        )
        with db.transaction():
            for kind, code in (("trait", "synthetic"), ("special_kind", "token")):
                db.insert(
                    "vocabulary",
                    {
                        "kind": kind,
                        "code": code,
                        "label_unit_id": revision["name_unit_id"],
                        "active": True,
                    },
                )
            db.insert(
                "face_special_kind",
                {"revision_id": revision["id"], "special_kind_code": "token"},
            )
            db.insert(
                "face_trait", {"revision_id": revision["id"], "trait_code": "synthetic"}
            )
        token_facts = adopted_kinds(db, snapshot, fixture.sources())
        assert token_facts.token_traits == (("synthetic", ("follower",)),)
        data["source_ref"] = fixture.jp.model_dump(mode="json") | {
            "source_version_id": "src:v1:" + "c" * 64,
        }
        write(root, {"translations/glossary/defs/000.yaml": envelope([record])})
        other_snapshot = load_glossary(root)
        assert adopted_kinds(db, other_snapshot, fixture.sources()).named == ()
