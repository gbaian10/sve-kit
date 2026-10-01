"""Small invented HTML/registry histories with independently sealed physical inputs."""

import shutil
from dataclasses import dataclass, replace
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build_inputs import BuildContext
from sve_carddb.catalog.adoption_importer import AdoptionInputs
from sve_carddb.catalog.languages import register_languages
from sve_carddb.extract.compare_jp import legacy_projection
from sve_carddb.extract.official_jp import extract_card
from sve_carddb.manifest import Kind
from sve_carddb.products.models import Language
from sve_carddb.registry.build import build
from sve_carddb.registry.inputs import Mapping
from sve_carddb.registry.preview import FrozenJP, plan_preview
from sve_carddb.registry.records import PrintingData
from sve_carddb.registry.review import Inputs, Receipt
from sve_carddb.registry.storage import Entry, plan_files, read_yaml, write_files
from sve_carddb.routes.rarity_policy import APPROVED_GENERAL_RARITIES
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.source_archive import seal_batch
from sve_carddb.sources.official_jp import card_url
from sve_carddb.text_observations import FrozenTexts, TextPlan, plan_text_observations

from .adoption_fixtures import (
    CODE,
    REPO,
    Case,
    commit,
    dependency,
    envelope,
    index,
    make_case,
    record,
    write,
)
from .build_db_fixtures import rows
from .test_effect_presence import page
from .test_source_archive import _put, _resource, _store

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import Database

RUNTIME = (
    CODE,
    "carddb/uv.lock",
    "carddb/pyproject.toml",
    "carddb/src/sve_carddb/catalog/adoption_sources.py",
    "carddb/src/sve_carddb/html.py",
    "carddb/src/sve_carddb/fetch/validate.py",
    "carddb/src/sve_carddb/sources/official_jp.py",
    "carddb/src/sve_carddb/extract/official_jp.py",
    "carddb/src/sve_carddb/extract/compare_jp.py",
    "carddb/src/sve_carddb/registry/inputs.py",
    "carddb/src/sve_carddb/registry/review.py",
)
IMAGE_URL = "https://example.invalid/synthetic.png"
IMAGE_RAW = b"\x89PNG\r\n\x1a\n" + b"Synthetic image" * 100


@dataclass(frozen=True)
class DisplayCase:
    case: Case
    archive: Path
    plan: TextPlan
    name_refs: tuple[dict[str, JsonValue], ...]

    def inputs(self) -> AdoptionInputs:
        return AdoptionInputs(
            self.case.root,
            self.case.repository,
            self.case.revision,
            ("catalog-adoptions", "display-overrides"),
        )

    def context(self) -> BuildContext:
        config = (
            self.inputs().configuration() | APPROVED_GENERAL_RARITIES.configuration()
        )
        config["text_observations"] = self.plan.configuration()
        reviewed = object_value(
            parse(
                str(object_value(self.case.review["context"])["configuration"]).encode()
            )
        )
        config["catalog_source_recipes"] = reviewed["catalog_source_recipes"]
        return BuildContext.from_inputs(
            self.case.revision,
            {path: (self.case.repository / path).read_bytes() for path in RUNTIME},
            config,
        )

    def parents(self, db: Database) -> None:
        fixture = rows()
        with db.transaction():
            register_languages(
                db,
                tuple(
                    Language(code=lang, display_name=lang, fallback_order=fallback)
                    for lang, fallback in (
                        ("ja", ("en",)),
                        ("en", ("ja",)),
                        ("zh-Hant", ("ja", "en")),
                    )
                ),
            )
            for table in ("decision", "text_unit"):
                db.insert(table, fixture[table])
            families = {
                r.data.model_dump()["home_set_id"]
                for r in self.plan.publication_identity().included("card")
            }
            for number, family in enumerate(sorted(families)):
                db.insert(
                    "product_family",
                    fixture["product_family"]
                    | {
                        "id": family,
                        "code": f"synthetic{number}",
                        "public_code": f"SYN{number}",
                    },
                )


def make_display_case(  # ruff: ignore[too-many-locals,too-many-statements] -- one sealed fixture owns the independent source and registry pins
    root: Path, *, variants: bool, pending: bool = False
) -> DisplayCase:
    root.mkdir()
    store = _store(root / "source")
    cards = {}
    for number in ("BP01-001", "BP01-002"):
        raw = (
            page(
                "jp",
                '<div class="detail">Synthetic alternate rule</div>'
                if pending and number == "BP01-002"
                else '<div class="detail">Synthetic relation rule</div>',
            )
            .replace(b"SYN-01", number.encode())
            .replace(b"/synthetic.png", IMAGE_URL.encode())
        )
        _put(
            store,
            _resource(card_url(number), f"raw/{number}.html", raw, Kind.CARD),
            raw,
        )
        cards[number] = legacy_projection(extract_card(raw, number=number))
    _put(
        store,
        _resource(IMAGE_URL, "raw/synthetic.png", IMAGE_RAW, Kind.IMAGE),
        IMAGE_RAW,
    )
    sealed = seal_batch(store)
    case = make_case(root / "repository")
    for path in RUNTIME:
        target = case.repository / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes((REPO / path).read_bytes())
    inputs = Inputs(
        jp=cards,
        en={},
        mapping=Mapping(targets={}, original_art=set(), reskins={}),
        receipt=Receipt(
            policy="identity-init-2026-09-28-v1",
            reviewed_by="Synthetic reviewer",
            reviewed_on="2026-10-01",
            input_hashes={"jp": digest(b"Synthetic complete cards")},
        ),
    )
    records = build(inputs, {})
    if variants:
        first = next(r for r in records if r.kind == "printing")
        alternate = first.model_copy(deep=True)
        alternate.data.update(
            id="p:ffffffffffffffffffffffffffffffff", variant_key="alternate"
        )
        alternate.record_key = "printing:p:ffffffffffffffffffffffffffffffff"
        records.append(alternate)
        allocation = next(r for r in records if r.kind == "card_int_id").model_copy(
            deep=True
        )
        allocation.record_key = "card_int_id:p:ffffffffffffffffffffffffffffffff"
        allocation.data.update(
            int_id=20003, printing_id="p:ffffffffffffffffffffffffffffffff"
        )
        records.append(allocation)
        third = alternate.model_copy(deep=True)
        third.data.update(
            id="p:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee", variant_key="second-alternate"
        )
        third.record_key = "printing:p:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
        records.append(third)
        third_id = allocation.model_copy(deep=True)
        third_id.record_key = "card_int_id:p:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
        third_id.data.update(
            int_id=20004, printing_id="p:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
        )
        records.append(third_id)
    write_files(plan_files(case.root, records, "Synthetic reviewer", "2026-10-01"))
    reviewed_revision = commit(case.repository)
    identity = plan_preview(
        case.root,
        FrozenJP(
            store.root,
            store.store_id,
            sealed.batch_id,
            parser_version="official-jp-exact-v1",
        ),
        regions=("jp",),
    )
    plan = plan_text_observations(
        identity,
        FrozenTexts(
            store.root,
            store.store_id,
            sealed.batch_id,
            region="jp",
            parser_version="official-jp-exact-v1",
        ),
    )
    code_path = "carddb/src/sve_carddb/extract/official_jp.py"
    recipe: dict[str, JsonValue] = {
        "version": "official-jp-exact-v1",
        "program_revision": reviewed_revision,
        "code_path": code_path,
        "code_hash": digest((REPO / code_path).read_bytes()),
        "config": {},
        "config_hash": digest(canonical({})),
    }
    context = BuildContext.from_inputs(
        reviewed_revision,
        {path: (REPO / path).read_bytes() for path in RUNTIME},
        {
            "catalog_source_recipes": {"official-jp-exact-v1": recipe},
            "catalog_registry": {
                "authored_revision": reviewed_revision,
                "index_path": "authored/ids/index.yaml",
                "index_hash": digest(
                    canonical(read_yaml(case.root / "ids/index.yaml"))
                ),
            },
        },
    )
    review: dict[str, JsonValue] = {
        "context": context.model_dump(mode="json"),
        "source_batches": [{"store_id": store.store_id, "batch_id": sealed.batch_id}],
    }
    for shard_path in (case.root / "catalog-adoptions").rglob("*.yaml"):
        if shard_path.name == "index.yaml":
            continue
        shard = object_value(read_yaml(shard_path))
        members = array(shard["records"])
        for member in members:
            object_value(object_value(member)["data"])["review_context_hash"] = digest(
                canonical(review)
            )
        write(
            case.root,
            shard_path.relative_to(case.root).as_posix(),
            envelope(members, review),
        )
    refs: tuple[dict[str, JsonValue], ...] = tuple(
        {
            "store_id": store.store_id,
            "batch_id": sealed.batch_id,
            "source_version_id": o.card.source.id,
            "parser": "official-jp-exact-v1",
            "locator": f"/faces/{o.source_index}/name",
            "text_hash": digest(o.content.name.encode()),
        }
        for o in plan.observations
    )
    first_observation = plan.observations[0]
    card_id, face_id = first_observation.card_id, first_observation.face_id
    rule_refs: list[dict[str, JsonValue]] = []
    for o, ref in zip(plan.observations, refs, strict=True):
        effect = o.content.effect
        assert effect is not None
        rule_refs.append(
            ref
            | {
                "locator": f"/faces/{o.source_index}/text",
                "text_hash": digest(effect.encode()),
            }
        )
    names = record(
        "rules_name_adoption",
        {"face_id": face_id, "region": "jp", "role": "treated_as"},
        {
            "names": [{"kind": "source", "source_ref": refs[0]}],
            "identity_ref": {"face_id": face_id, "card_id": card_id},
            "observations": sorted(
                [
                    {
                        "printing_id": o.printing_id,
                        "face_id": face_id,
                        "source_ref": ref,
                    }
                    for o, ref in zip(plan.observations, refs, strict=True)
                ],
                key=canonical,
            ),
            "name_basis_hash": digest(
                canonical([{"lang": "ja", "text_hash": refs[0]["text_hash"]}])
            ),
        },
        review,
        [dependency("card", id=card_id), dependency("face", id=face_id)],
    )
    names["evidence"] = sorted(
        [
            {"source_ref": ref, "role": "Synthetic reviewed basis"}
            for ref in {canonical(ref): ref for ref in (*refs, *rule_refs)}.values()
        ],
        key=canonical,
    )
    names["evidence"] = sorted(
        [
            *array(names["evidence"]),
            {
                "image_ref": {
                    "store_id": store.store_id,
                    "batch_id": sealed.batch_id,
                    "source_version_id": next(
                        e.source_version_id
                        for e in sealed.inventory.current
                        if e.url == IMAGE_URL
                    ),
                    "raw_hash": digest(IMAGE_RAW),
                    "printing_id": first_observation.printing_id,
                    "face_id": first_observation.face_id,
                },
                "role": "Synthetic image review",
            },
        ],
        key=canonical,
    )
    write(
        case.root,
        "catalog-adoptions/rules-names/shared/001.yaml",
        envelope([names], review),
    )
    candidates = sorted(o.printing_id for o in plan.observations)
    default = record(
        "default_printing_adoption",
        {"card_id": card_id, "region": "jp"},
        {
            "printing_id": candidates[-1],
            "candidates": list[JsonValue](candidates),
            "candidates_hash": digest(canonical(list[JsonValue](candidates))),
        },
        review,
        [dependency("card", id=card_id), dependency("printing", id=candidates[-1])],
    )
    write(
        case.root,
        "display-overrides/defaults/shared/001.yaml",
        envelope([default], review, entry="display-overrides"),
    )
    if variants:
        matching = sorted(
            (
                r
                for r in identity.snapshot.records.values()
                if isinstance(r.data, PrintingData) and r.data.card_no == "BP01-001"
            ),
            key=lambda r: str(r.data.model_dump()["id"]),
        )
        route_candidates: list[JsonValue] = [
            {
                "printing_id": r.data.model_dump()["id"],
                "card_id": card_id,
                "region": "jp",
                "card_no": "BP01-001",
                "card_no_state": "official",
                "variant_key": r.data.model_dump()["variant_key"],
                "identity_ref": {
                    "record_key": r.record_key,
                    "record_hash": digest(r.content),
                    "decision_id": r.decision_id,
                },
            }
            for r in matching
        ]
        selected = str(object_value(route_candidates[0])["printing_id"])
        route = record(
            "route_override_adoption",
            {"region": "jp", "route_key": "BP01-001"},
            {
                "printing_id": selected,
                "candidates": route_candidates,
                "candidates_hash": digest(canonical(route_candidates)),
            },
            review,
            [dependency("printing", id=selected)],
        )
        write(
            case.root,
            "display-overrides/routes/shared/001.yaml",
            envelope([route], review, entry="display-overrides"),
        )
    index(case.root)
    index(case.root, entry="display-overrides")
    return DisplayCase(
        replace(case, revision=commit(case.repository), review=review),
        store.root,
        plan,
        refs,
    )


def current_case(
    base: DisplayCase,
    root: Path,
    *,
    name: str = "Synthetic name",
    rule: str = "Synthetic relation rule",
    extra: bool = False,
) -> DisplayCase:
    """Rebuild new frozen bytes and complete registry observations, retaining historic evidence."""
    store = _store(root)
    shutil.copytree(base.archive, store.root, dirs_exist_ok=True)
    cards = {}
    numbers = (
        ("BP01-001", "BP01-002", "BP01-003") if extra else ("BP01-001", "BP01-002")
    )
    for number in numbers:
        raw = (
            page("jp", f'<div class="detail">{rule}</div>')
            .replace(b"SYN-01", number.encode())
            .replace(b"Synthetic name", name.encode())
            .replace(b"/synthetic.png", IMAGE_URL.encode())
            + b"<!-- Synthetic new source version -->"
        )
        _put(
            store,
            _resource(card_url(number), f"raw/{number}.html", raw, Kind.CARD),
            raw,
        )
        cards[number] = legacy_projection(extract_card(raw, number=number))
    sealed = seal_batch(store)
    inputs = Inputs(
        jp=cards,
        en={},
        mapping=Mapping(targets={}, original_art=set(), reskins={}),
        receipt=Receipt(
            policy="identity-init-2026-09-28-v1",
            reviewed_by="Synthetic reviewer",
            reviewed_on="2026-10-01",
            input_hashes={"jp": digest(b"Synthetic revised cards")},
        ),
    )
    existing = {
        r.record_key: Entry.model_validate_json(r.content)
        for r in base.plan.identity.snapshot.records.values()
    }
    # The fixture rewrites a synthetic reviewed registry; actual identity repairs have their own contract.
    for path in (base.case.root / "registry").rglob("*.yaml"):
        path.unlink()
    for path in (base.case.root / "ids").rglob("*.yaml"):
        path.unlink()
    write_files(
        plan_files(
            base.case.root, build(inputs, existing), "Synthetic reviewer", "2026-10-01"
        )
    )
    identity = plan_preview(
        base.case.root,
        FrozenJP(
            store.root,
            store.store_id,
            sealed.batch_id,
            parser_version="official-jp-exact-v1",
        ),
        regions=("jp",),
    )
    plan = plan_text_observations(
        identity,
        FrozenTexts(
            store.root,
            store.store_id,
            sealed.batch_id,
            region="jp",
            parser_version="official-jp-exact-v1",
        ),
    )
    return replace(
        base,
        plan=plan,
        archive=store.root,
        case=replace(base.case, revision=commit(base.case.repository)),
    )
