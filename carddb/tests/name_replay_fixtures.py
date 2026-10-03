"""Small adopted name overrides over one shared synthetic frozen registry."""

from dataclasses import dataclass, replace
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build_inputs import BuildContext
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.text_observations.archive import FrozenTexts
from sve_carddb.text_observations.models import FaceObservation, candidate_revision_id
from sve_carddb.translations.importer import Inputs
from sve_carddb.translations.loader import load_glossary
from sve_carddb.translations.name_replay import NameReplay, replay_names
from sve_carddb.translations.sources import Sources

from .adoption_fixtures import commit
from .digital_link_import_fixtures import Fixture, make_fixture
from .test_glossary_adoption import authored
from .translation_fixtures import envelope, write

if TYPE_CHECKING:
    from pathlib import Path


def name_term(
    key: str = "name.synthetic", text: str = "Synthetic card"
) -> dict[str, JsonValue]:
    record = authored(key, category="card_name")
    object_value(record["data"])["authored_source_ja"] = text
    return record


def human(records: list[dict[str, JsonValue]]) -> dict[str, JsonValue]:
    shard = envelope(records)
    decision = object_value(array(shard["decisions"])[0])
    decision["reviewed_by"] = "gbaian10"
    if records[0]["kind"] == "card_name_concept":
        decision["policy_id"] = "card-name-concept-v1"
    return shard


@dataclass(frozen=True)
class Case:
    frozen: Fixture
    revision_id: str
    basis: dict[str, JsonValue]

    def concept(
        self,
        *,
        key: str = "name.synthetic",
        number: int = 1,
        previous: JsonValue = None,
    ) -> dict[str, JsonValue]:
        subject: dict[str, JsonValue] = {
            "card_id": self.frozen.card.id,
            "face_id": self.frozen.face.id,
            "source_lang": "ja",
            "source_hash": self.frozen.jp.text_hash,
        }
        return {
            "kind": "card_name_concept",
            "filing_key": "concepts",
            "record_key": canonical(["card_name_concept", subject, number]).decode(),
            "data": {
                "subject": subject,
                "term_id": "term:" + key,
                "source_ref": self.frozen.jp.model_dump(mode="json"),
                "identity_basis": parse(canonical(self.basis)),
                "reason": "Synthetic disambiguated concept.",
                "adoption_no": number,
                "predecessor": previous,
            },
            "evidence": [],
        }

    def assignment(
        self,
        *,
        variant: str = "synthetic",
        key: str = "name.synthetic",
        number: int = 1,
        previous: JsonValue = None,
    ) -> dict[str, JsonValue]:
        owner: dict[str, JsonValue] = {
            "kind": "face_revision",
            "revision_id": self.revision_id,
        }
        return {
            "kind": "context_assignment",
            "filing_key": "assignments",
            "record_key": canonical(
                ["context_assignment", owner, "name", None, number]
            ).decode(),
            "data": {
                "owner": owner,
                "field": "name",
                "ordinal": None,
                "source_hash": self.frozen.jp.text_hash,
                "variant": variant,
                "concept_key": key,
                "reason": "Synthetic true homonym.",
                "adoption_no": number,
                "predecessor": previous,
            },
            "evidence": [
                {"source_ref": self.frozen.jp.model_dump(mode="json"), "role": "name"}
            ],
        }

    def stage(
        self, shards: dict[str, dict[str, JsonValue]]
    ) -> tuple[Inputs, BuildContext]:
        write(self.frozen.root / "authored", shards)
        revision = commit(self.frozen.root)
        inputs = Inputs(self.frozen.root / "authored", self.frozen.root, revision)
        config = (
            object_value(parse(self.frozen.build.configuration.encode()))
            | inputs.configuration()
        )
        return inputs, BuildContext.from_inputs(
            self.frozen.program,
            {
                p.name: (self.frozen.root / p.name).read_bytes()
                for p in self.frozen.build.dependencies
            },
            config,
        )

    def replay(
        self, shards: dict[str, dict[str, JsonValue]]
    ) -> tuple[NameReplay, Inputs, BuildContext]:
        inputs, build = self.stage(shards)
        snapshot = load_glossary(inputs.root)
        originals = {
            r.data.id: str(r.data.authored_source_ja)
            for r, _ in snapshot.records()
            if r.kind == "glossary_term"
        }
        sources = Sources({"test-store": self.frozen.store}, inputs.repository, build)
        replay, _ = replay_names(snapshot, originals, inputs, sources)
        return replay, inputs, build


def make_case(root: Path) -> Case:
    frozen = make_fixture(root)
    ref = frozen.jp
    texts = FrozenTexts(
        frozen.store, ref.store_id, ref.batch_id, region="jp", parser_version=ref.parser
    )
    card = texts.version("jp", frozen.printing.card_no, ref.source_version_id)
    item = FaceObservation(
        card_id=frozen.card.id,
        printing_id=frozen.printing.id,
        face_id=frozen.face.id,
        region="jp",
        card_no=frozen.printing.card_no,
        source_index=0,
        card=card,
        content=card.projected(0),
    )
    return Case(
        frozen,
        candidate_revision_id(item),
        {
            "authored_revision": frozen.authored,
            "registry_index_hash": digest(
                canonical(read_yaml(root / "authored/ids/index.yaml"))
            ),
            "transition_index_hash": None,
        },
    )


def copied(case: Case, root: Path) -> Case:
    import shutil  # ruff: ignore[import-outside-top-level] -- clones stay local to this synthetic fixture

    shutil.copytree(case.frozen.root, root)
    return replace(
        case,
        frozen=replace(
            case.frozen,
            root=root,
            store=root / case.frozen.store.relative_to(case.frozen.root),
        ),
    )
