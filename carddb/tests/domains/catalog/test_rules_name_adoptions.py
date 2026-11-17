"""Special construction names: checked sources, rebuilt name bases and collab wording."""

from types import SimpleNamespace
from typing import TYPE_CHECKING, Any, cast

import pytest
from pydantic import JsonValue

from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.catalog.adoption_models import (
    AuthoredText,
    SourceRef,
    SourceText,
)
from sve_carddb.domains.catalog.records import NameRecord
from sve_carddb.domains.catalog.rules_name_adoptions import (
    NameAdoption,
    check,
    collab_hashes,
    evaluate,
    name_basis,
    name_refs,
)
from sve_carddb.domains.products.models import LocalizedText
from sve_carddb.domains.registry.records import (
    FaceData,
    FaceMap,
    Observation,
    PrintingData,
)
from sve_carddb.domains.text_observations.equivalence import (
    comparable,
    equivalent,
    is_collab,
)
from sve_carddb.domains.text_observations.models import FaceContent

if TYPE_CHECKING:
    from sve_carddb.domains.catalog.adoption_loader import AdoptionSnapshot
    from sve_carddb.domains.catalog.adoption_models import ReviewContext
    from sve_carddb.domains.catalog.adoption_sources import AdoptionSources
    from sve_carddb.domains.registry.snapshot import RegistrySnapshot
    from sve_carddb.domains.text_observations.plan import TextPlan

CARD, FACE = "c:" + "a" * 32, "f:" + "a" * 32
OWN, COLLAB = "p:" + "1" * 32, "p:" + "2" * 32
BATCH = "sha256:" + "b" * 64
RULES, COLLAB_NAME = "吸血姫・ヴァンピィ", "ラプラスの魔"
LOCATORS = {"/own/name": RULES, "/collab/name": COLLAB_NAME}


def ref(locator: str) -> dict[str, JsonValue]:
    return {
        "batch_id": BATCH,
        "source_version_id": "src:v1:" + ("1" if "own" in locator else "2") * 64,
        "parser": "official-jp-exact-v1",
        "locator": locator,
        "text_hash": digest(LOCATORS[locator].encode()),
    }


def record(
    *,
    role: str = "collab",
    names: list[JsonValue] | None = None,
    evidence: list[JsonValue] | None = None,
    basis: str | None = None,
    card: str = CARD,
) -> NameRecord:
    observations: list[JsonValue] = [
        {"printing_id": OWN, "face_id": FACE, "source_ref": ref("/own/name")},
        {"printing_id": COLLAB, "face_id": FACE, "source_ref": ref("/collab/name")},
    ]
    return NameRecord.model_validate_json(
        canonical(
            {
                "kind": "rules_name_adoption",
                "data": {
                    "subject": {"face_id": FACE, "region": "jp", "role": role},
                    "value": {
                        "names": names
                        if names is not None
                        else [{"kind": "source", "source_ref": ref("/collab/name")}],
                        "identity_ref": {"face_id": FACE, "card_id": card},
                        "observations": observations,
                        "name_basis_hash": basis
                        or name_basis(
                            ("ja", digest(text.encode()))
                            for text in (RULES, COLLAB_NAME)
                        ),
                    },
                    "evidence": evidence or [],
                },
            }
        )
    )


class Sources:
    def text(
        self, source: SourceRef, _review: object
    ) -> tuple[LocalizedText, None, None]:
        return LocalizedText(lang="ja", text=LOCATORS[source.locator]), None, None

    def value(self, value: AuthoredText | SourceText, review: object) -> LocalizedText:
        if isinstance(value, AuthoredText):
            return LocalizedText(lang=value.lang, text=value.text)
        return self.text(value.source_ref, review)[0]


def registry(face_card: str = CARD) -> RegistrySnapshot:
    def printing(identifier: str) -> SimpleNamespace:
        return SimpleNamespace(
            data=PrintingData.model_construct(
                id=identifier,
                card_id=CARD,
                region="jp",
                card_no=identifier,
                variant_key="standard",
                home_set_id="BP02",
                source_face_map=(FaceMap(source_index=0, face_id=FACE),),
                observation=Observation(
                    region="jp",
                    card_no=identifier,
                    recipe="registry-observation-v2",
                    observation_hash=BATCH,
                    rules_hash=BATCH,
                ),
            )
        )

    records = {
        "face:" + FACE: SimpleNamespace(
            data=FaceData(id=FACE, card_id=face_card, ordinal=0, side="front")
        ),
        "printing:" + OWN: printing(OWN),
        "printing:" + COLLAB: printing(COLLAB),
    }
    return cast("RegistrySnapshot", SimpleNamespace(records=records))


def checked(item: NameRecord, face_card: str = CARD) -> NameAdoption | None:
    return check(
        item,
        cast("ReviewContext", None),
        cast("AdoptionSources", Sources()),
        registry(face_card),
    )


def content(name: str, effect: str = "Draw 1.") -> FaceContent:
    return FaceContent(
        name=name, effect=effect, class_raw="c", type_raw="t", stats=("1", "1", "1")
    )


def plan(names: dict[str, str], reasons: tuple[str, ...] = ()) -> TextPlan:
    return cast(
        "TextPlan",
        SimpleNamespace(
            groups=(SimpleNamespace(face_id=FACE, region="jp", reasons=reasons),),
            observations=tuple(
                SimpleNamespace(face_id=FACE, region="jp", content=content(name))
                for name in names.values()
            ),
        ),
    )


def test_checked_collab_adoption_applies_while_the_name_basis_is_unchanged() -> None:
    adoption = checked(record())
    assert adoption is not None
    assert adoption.identity_current
    assert adoption.names == (COLLAB_NAME,)
    bindings, report = evaluate(
        (adoption,), plan({OWN: RULES, COLLAB: COLLAB_NAME, "reprint": RULES})
    )
    assert [(b.official_name, b.role, b.decision_id) for b in bindings] == [
        (COLLAB_NAME, "collab", None)
    ]
    assert report.payload() == {
        "applied": [adoption.record_key],
        "stale": [],
        "unverifiable": [],
    }


@pytest.mark.parametrize(
    ("names", "reasons", "face_card", "outcome"),
    [
        ({OWN: RULES, COLLAB: COLLAB_NAME, "new": "Another name"}, (), CARD, "stale"),
        ({OWN: RULES}, (), CARD, "stale"),
        ({OWN: RULES, COLLAB: COLLAB_NAME}, (), "c:" + "f" * 32, "stale"),
        ({OWN: RULES, COLLAB: COLLAB_NAME}, ("missing_source",), CARD, "unverifiable"),
    ],
)
def test_changed_or_unverifiable_name_sets_are_not_applied(
    names: dict[str, str], reasons: tuple[str, ...], face_card: str, outcome: str
) -> None:
    adoption = checked(record(), face_card)
    assert adoption is not None
    bindings, report = evaluate((adoption,), plan(names, reasons))
    assert bindings == ()
    assert report.payload()[outcome] == [adoption.record_key]


def test_withdrawn_adoption_is_not_checked() -> None:
    item = record().model_copy(
        update={"data": record().data.model_copy(update={"value": None})}
    )
    assert item.data.value is None
    assert checked(item) is None


@pytest.mark.parametrize(
    ("kwargs", "message"),
    [
        ({"basis": "sha256:" + "0" * 64}, "basis hash differs"),
        (
            {"names": [{"kind": "authored", "lang": "ja", "text": COLLAB_NAME}]},
            "must come from sources",
        ),
        (
            {
                "role": "primary",
                "names": [{"kind": "source", "source_ref": ref("/own/name")}],
            },
            "one authored text",
        ),
        (
            {
                "role": "primary",
                "names": [{"kind": "authored", "lang": "ja", "text": "X"}],
            },
            "requires evidence",
        ),
        (
            {
                "names": [{"kind": "authored", "lang": "en", "text": "X"}],
                "role": "primary",
                "evidence": [{"source_ref": ref("/own/name"), "role": "rules_text"}],
            },
            "language differs",
        ),
    ],
)
def test_invalid_adoptions_are_rejected(kwargs: dict[str, Any], message: str) -> None:
    with pytest.raises(ValueError, match=message):
        checked(record(**kwargs))


def test_explicit_primary_keeps_its_authored_name_and_cites_every_source() -> None:
    item = record(
        role="primary",
        names=[{"kind": "authored", "lang": "ja", "text": "魔法のアイテム"}],
        evidence=[{"source_ref": ref("/own/name"), "role": "rules_text"}],
    )
    adoption = checked(item)
    assert adoption is not None
    assert adoption.names == ("魔法のアイテム",)
    assert len(name_refs(item)) == 3


def test_collab_hashes_read_only_current_collab_names() -> None:
    primary = record(
        role="primary",
        names=[{"kind": "authored", "lang": "ja", "text": "X"}],
        evidence=[{"source_ref": ref("/own/name"), "role": "rules_text"}],
    )
    snapshot = cast(
        "AdoptionSnapshot",
        SimpleNamespace(current_records=lambda: (record(), primary)),
    )
    assert collab_hashes((snapshot,)) == {
        (FACE, "jp"): frozenset({digest(COLLAB_NAME.encode())})
    }


def test_collab_printings_compare_every_field_but_their_name() -> None:
    hashes = frozenset({digest(COLLAB_NAME.encode())})
    own, collab = content(RULES), content(COLLAB_NAME)
    assert is_collab(collab, hashes)
    assert not is_collab(own, hashes)
    assert not equivalent((own, collab))
    assert equivalent(comparable((own, collab), hashes))
    assert not equivalent(comparable((own, content(COLLAB_NAME, "Draw 2.")), hashes))
    # Two non-collab names still disagree; only declared names are set aside.
    assert not equivalent(comparable((own, content("Other"), collab), hashes))
    assert equivalent(comparable((collab, collab), hashes))
    assert comparable((own, collab), frozenset()) == (own, collab)
