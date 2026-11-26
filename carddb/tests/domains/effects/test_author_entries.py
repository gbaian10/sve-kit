"""Whole author inputs remain structural, source-qualified expansion is separate."""

import json
from copy import deepcopy
from hashlib import sha256
from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.author_leaves import AuthorIndexRecord
from sve_carddb.core.json import array, canonical, object_value
from sve_carddb.domains.effects.author_inputs import AuthorInputError, AuthorSchemas

from .author_entry_cases import meta, source_use

if TYPE_CHECKING:
    from pydantic import JsonValue

SCHEMAS = Path(__file__).resolve().parents[4] / "dsl"
GOLDEN = (
    Path(__file__).resolve().parents[2] / "fixtures" / "author_entries" / "golden.yaml"
)


@pytest.fixture(scope="module")
def schemas() -> AuthorSchemas:
    return AuthorSchemas(SCHEMAS)


def ability(head: dict[str, JsonValue]) -> dict[str, JsonValue]:
    source = source_use()
    if "keywords" in head:
        source.pop("ability_scope")
    return {
        "line": 1,
        "sources": [source],
        "ports": {"imports": {}, "exports": {}},
        **head,
    }


def document(entry: dict[str, JsonValue]) -> dict[str, JsonValue]:
    return {
        "format": 1,
        "kind": "author_effect_set",
        "scope": "shared",
        "cards": {"FIX-001": {"meta": meta(), "abilities": [entry]}},
    }


def test_prototype_requires_its_own_explicit_entry(schemas: AuthorSchemas) -> None:
    prototype: JsonValue = {
        "format": 1,
        "kind": "effect_set",
        "cards": {"FIX-001": {"status": "unsupported", "abilities": []}},
    }
    raw = canonical(prototype)
    assert schemas.prototype_tree(raw) == prototype
    with pytest.raises(AuthorInputError, match="unsupported_kind"):
        schemas.expanded(raw)
    with pytest.raises(AuthorInputError, match="unsupported_kind"):
        schemas.prototype_tree(canonical(document(ability({"keywords": ["quick"]}))))


def test_inline_rejects_missing_source_and_unknown_fields(
    schemas: AuthorSchemas,
) -> None:
    for field in ("line", "sources", "ports", "spell"):
        bad = ability({"spell": {"do": []}})
        del bad[field]
        with pytest.raises(AuthorInputError):
            schemas.expanded(canonical(document(bad)))
    bad = ability({"spell": {"do": []}}) | {"typo": True}
    with pytest.raises(AuthorInputError):
        schemas.expanded(canonical(document(bad)))


def first_ability(card: JsonValue) -> dict[str, JsonValue]:
    return object_value(array(object_value(card)["abilities"])[0])


def test_fixed_goldens_preserve_content_and_order(schemas: AuthorSchemas) -> None:
    value = schemas.expanded(GOLDEN.read_bytes())
    cards = object_value(object_value(value)["cards"])
    bp = first_ability(cards["BP08-038"])["fanfare"]
    expected_bp = b'{"fanfare":{"define":{"X":{"minus":[{"count":{"who":"self","zone":"deck"}},5]}},"do":[{"draw":{"n":"X"}}]}}'
    assert sha256(canonical({"fanfare": bp})).digest() == sha256(expected_bp).digest()
    cp = first_ability(cards["CP04-060"])["evolve_time"]
    expected_cp = '{"evolve_time":{"do":[{"search":{"n":1,"result":"searched","sel":{"not_same_name_as_this":true,"trait":"ドラゴンズネスト","who":"self","zone":"deck"},"to":"ex"}},{"apply":{"effect":{"cost_mod":{"delta":-2,"when_playing":{"in":"affected"}}},"on":{"follow":"searched"},"until":"turn"}}],"ub":true}}'.encode()
    assert (
        sha256(canonical({"evolve_time": cp})).digest() == sha256(expected_cp).digest()
    )
    assert object_value(cards["ECP01-058"])["deck_rule"] == {
        "deck": "evolve",
        "same_name_max": 10,
    }
    face = object_value(object_value(cards["BP09-SL05"])["faces"])["f:" + "2" * 32]
    expected_face: JsonValue = {
        "do": [
            {"create": {"token": ["スティールナイト", "ナイト"], "n": 1, "to": "field"}}
        ]
    }
    assert (
        sha256(canonical(first_ability(face)["evolve_time"])).digest()
        == sha256(canonical(expected_face)).digest()
    )
    assert canonical(
        schemas.expanded(json.dumps(value, ensure_ascii=False, sort_keys=True).encode())
    ) == canonical(value)
    reversed_value = deepcopy(value)
    reversed_cards = object_value(object_value(reversed_value)["cards"])
    array(
        object_value(first_ability(reversed_cards["CP04-060"])["evolve_time"])["do"]
    ).reverse()
    assert canonical(
        schemas.expanded(json.dumps(reversed_value).encode())
    ) != canonical(value)


@pytest.mark.parametrize(
    "date", ["2026-02-29", "0000-01-01", "2026-10-21\n", "2026-1-01"]
)
def test_body_writer_date_is_real_exact_calendar_date(
    schemas: AuthorSchemas, date: str
) -> None:
    value: JsonValue = {
        "format": 2,
        "kind": "effect_body_shard",
        "records": [
            {
                "id": "synthetic",
                "body": {"spell": {"do": []}},
                "written_by": [{"actor": {"who": "test", "model": None}, "date": date}],
            }
        ],
    }
    with pytest.raises(AuthorInputError, match="pattern"):
        schemas.storage_tree(canonical(value), "effect_body_shard")


def test_keyword_review_requires_draft_metadata(schemas: AuthorSchemas) -> None:
    value = document(ability({"keywords": ["ward"]}))
    card = object_value(object_value(value["cards"])["FIX-001"])
    object_value(card["meta"])["status"] = "reviewed"
    with pytest.raises(AuthorInputError, match="unsupported_keyword_review_evidence"):
        schemas.expanded(canonical(value))


@pytest.mark.parametrize(
    "raw",
    [b"[", b"{x: 1, x: 2}", b"{1: value}", b"{format: 2}"],
    ids=["syntax", "duplicate", "key", "version"],
)
def test_selected_index_context_survives_boundary_errors(
    schemas: AuthorSchemas, raw: bytes
) -> None:
    context = AuthorIndexRecord(
        card_id="c:" + "1" * 32,
        scope="shared",
        file="effects/fixture.yaml",
        kind="author_effect_set",
        record_key="FIX-001",
    )
    with pytest.raises(AuthorInputError) as caught:
        schemas.expanded(raw, context=context)
    diagnostic = caught.value.diagnostic
    assert (diagnostic.card_id, diagnostic.card_no, diagnostic.scope) == (
        context.card_id,
        "FIX-001",
        "shared",
    )


@pytest.mark.parametrize(
    ("card_no", "file", "face", "line", "digest"),
    [
        (
            "BP08-038",
            "BP08/000-099.json",
            0,
            1,
            "7a884eac36c1801277243b0bd7489062e396130b52be10bf3ab09ea4869904cb",
        ),
        (
            "CP04-060",
            "CP04/000-099.json",
            0,
            0,
            "86fa086090511c9105cfc1d9210f2102f69ef7351e6930dcd9dd89a8f53f3350",
        ),
        (
            "BP09-SL05",
            "BP09/SL/000-099.json",
            1,
            1,
            "298430d53c7ea027f74e3175d1d374c5fcd40f7a9fa1bb8e9db53b85cce26b80",
        ),
        (
            "ECP01-058",
            "ECP01/000-099.json",
            0,
            0,
            "c5b5200342452cde425e90b5db756548f926d3122c1d6e1f118a89fb9bcfc3f8",
        ),
    ],
)
def test_golden_content_retains_frozen_line_evidence(
    card_no: str, file: str, face: int, line: int, digest: str
) -> None:
    value = json.loads((SCHEMAS.parent / "card-data" / "jp" / file).read_bytes())
    card = next(card for card in value["cards"] if card["card_no"] == card_no)
    hashes = {
        sha256(
            document["record"]["faces"][face]["text"].split("\n")[line].encode()
        ).hexdigest()
        for document in card["documents"].values()
    }
    assert hashes == {digest}
