"""Synthetic list, human events and vendor reports for the public adoption entry."""

import hashlib
import json
import shutil
from dataclasses import dataclass, field
from pathlib import Path

from pydantic import JsonValue

from sve_carddb.core.json import array, canonical, digest, object_value, parse
from sve_carddb.domains.construction.adoption import Settings
from sve_carddb.domains.construction.adoption_reports import (
    REQUIRED_DEPENDENCIES,
    StorePin,
)
from sve_carddb.domains.construction.adoption_sources import Lists
from sve_carddb.ingest.archive.source_archive import ArchiveStore, seal_batch
from sve_carddb.ingest.archive.source_import.importer import prepare, register

from .source_import_fixtures import REPO, create_seed, git

DAY = "2020-01-02"
HTML = b"""<!doctype html><html><head><title>Synthetic restrictions</title></head><body><main class="sw-Lower_Container">
<h2>REPLACE</h2><h3>BAN</h3><h4>2020/1/1(day)PLACE</h4>
<a href="https://shadowverse-evolve.com/cardlist/?cardno=TEST-001%E2%93%88a">FIRST</a>
<h3>ONE</h3><h4>2020/1/1(day)PLACE</h4>
<a href="https://shadowverse-evolve.com/cardlist/?cardno=TEST-002">SECOND</a>
<h3>PAST</h3></main></body></html>"""


def synthetic_html() -> bytes:
    return (
        HTML.decode()
        .replace("REPLACE", "カードの使用制限に関するルール")
        .replace("BAN", "禁止カード")
        .replace("ONE", "殿堂入りカード")
        .replace("PAST", "過去の更新内容")
        .replace("(day)PLACE", "(水)より適用")
        .replace("FIRST", "『Synthetic name』")
        .replace("SECOND", "『Synthetic two』")
        .encode()
    )


@dataclass
class AdoptionFixture:
    repository: Path
    directory: Path
    archive: Path
    store_id: str
    batch_id: str
    tool_revision: str
    context: dict[str, JsonValue]
    records: list[dict[str, JsonValue]]
    settings: Settings | None = None
    revision: str = ""
    report_hashes: set[str] = field(default_factory=set)

    def write(self) -> None:  # ruff: ignore[too-many-locals, too-many-statements] -- Keep the complete admission boundary visible.
        context_hash = digest(canonical(self.context))
        dependencies = object_value(self.context["context"])["configuration"]
        configuration = object_value(json.loads(str(dependencies)))
        root = self.directory / "reviews"
        root.mkdir(exist_ok=True)
        pins = configuration["dependencies"]
        assert isinstance(pins, list)
        for raw in pins:
            pin = object_value(raw)
            target = root / "dependencies/sha256" / str(pin["sha256"])[7:]
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes((self.repository / str(pin["name"])).read_bytes())
        groups: dict[str, list[dict[str, JsonValue]]] = {}
        basis_path = root / "inputs/basis-inputs.json"
        basis_inputs: dict[str, JsonValue] = (
            {}
            if not basis_path.exists()
            else object_value(parse(basis_path.read_bytes()))
        )
        for record in self.records:
            data = object_value(record["data"])
            data["review_context_hash"] = context_hash
            data["dependencies"] = sorted(array(data["dependencies"]), key=canonical)
            basis_hash = digest(
                canonical(
                    {
                        key: value
                        for key, value in record.items()
                        if key not in {"review", "adoption_review"}
                    }
                )
            )
            basis_inputs[basis_hash] = {
                key: value
                for key, value in record.items()
                if key not in {"review", "adoption_review"}
            }
            evidence = record["evidence"]
            assert isinstance(evidence, list)
            uses = sorted(
                (object_value(item)["use"] for item in evidence), key=canonical
            )
            reviews: list[JsonValue] = []
            for vendor, model, version in (
                ("anthropic", "Claude Sonnet", "5.5"),
                ("openai", "Codex", "gpt-6.1-sol"),
            ):
                review: dict[str, JsonValue] = {
                    "vendor": vendor,
                    "model": model,
                    "version": version,
                    "reviewed_at": "2026-10-24T00:00:00Z",
                    "basis_hash": basis_hash,
                    "outcome": "agreed",
                }
                report: dict[str, JsonValue] = review | {
                    "construction_review_report_format": 1,
                    "source_uses": uses,
                    "program_revision": self.tool_revision,
                    "dependencies": pins,
                    "note": "Synthetic fixture report; no real model or human event is asserted.",
                }
                encoded = canonical(report)
                checksum = digest(encoded)
                target = (
                    root / "reports/sha256" / checksum[7:9] / (checksum[7:] + ".json")
                )
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(encoded)
                self.report_hashes.add(checksum)
                reviews.append(review | {"report_hash": checksum})
            record["review"] = {
                "basis_hash": basis_hash,
                "model_reviews": reviews,
                "maintainer_resolution": {
                    "reviewed_by": "Synthetic maintainer",
                    "reviewed_at": "2026-10-24T00:00:00Z",
                    "reviewed_precision": "day",
                    "basis_hash": basis_hash,
                    "outcome": "adopted",
                    "note": "Synthetic event for isolated tests only.",
                },
                "maintainer_samples": [],
            }
            record["adoption_review"] = {
                "mode": "human",
                "policy": None,
                "initial_sample_decisions": [],
            }
            groups.setdefault(str(record["kind"]), []).append(record)
        authored = self.repository / "authored/construction-adoptions"
        if authored.exists():
            shutil.rmtree(authored)
        authored.mkdir(parents=True)
        includes: dict[str, JsonValue] = {}
        for kind, records in groups.items():
            members = sorted(
                [
                    [str(record["record_key"]), digest(canonical(record))]
                    for record in records
                ]
            )
            membership = digest(canonical([list(pair) for pair in members]))
            identifier = "d:" + membership[7:]
            decision: dict[str, JsonValue] = {
                "id": identifier,
                "state": "confirmed",
                "scope": "batch",
                "category": kind,
                "members": [list(pair) for pair in members],
                "membership_hash": membership,
                "policy_id": "construction-human-v1",
                "sample_ids": [pair[0] for pair in members],
                "confidence": None,
                "note": "Synthetic complete human batch.",
            }
            area = {
                "construction_profile": "profiles",
                "construction_restriction": "restrictions",
                "construction_coverage": "coverage",
            }[kind]
            shard: dict[str, JsonValue] = {
                "construction_adoption_format": 1,
                "kind": "construction_adoption_shard",
                "review_context": self.context,
                "default_decision_id": identifier,
                "records": list(records),
                "decisions": [decision],
            }
            content = canonical(shard)
            name = "construction-adoptions/" + area + "/jp/standard/001.yaml"
            target = self.repository / "authored" / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(content)
            includes[name] = digest(content)
        (authored / "index.yaml").write_bytes(
            canonical(
                {
                    "construction_adoption_format": 1,
                    "kind": "construction_adoption_index",
                    "includes": includes,
                }
            )
        )
        basis_file = root / "inputs/basis-inputs.json"
        basis_file.parent.mkdir(exist_ok=True)
        basis_file.write_bytes(canonical(basis_inputs))
        reports: list[JsonValue] = []
        reports.extend(sorted(self.report_hashes))
        inventory = canonical(
            {
                "construction_review_inventory_format": 1,
                "store_id": "synthetic-reviews",
                "reports": reports,
                "dependencies": pins,
                "review_inputs": [
                    {
                        "name": "inputs/basis-inputs.json",
                        "sha256": digest(basis_file.read_bytes()),
                    }
                ],
            }
        )
        (root / "inventory.json").write_bytes(inventory)
        backup, restored = self.directory / "backup", self.directory / "restored"
        shutil.copytree(root, backup, dirs_exist_ok=True)
        shutil.copytree(backup, restored, dirs_exist_ok=True)
        self.settings = Settings(
            source_stores={self.store_id: self.archive},
            review_stores=(
                StorePin(
                    store_id="synthetic-reviews",
                    root=root,
                    backup=backup,
                    restored=restored,
                    inventory_sha256=digest(inventory),
                ),
            ),
        )
        git(self.repository, "add", "authored")
        git(self.repository, "commit", "--allow-empty", "-qm", "Synthetic adoption")
        self.revision = git(self.repository, "rev-parse", "HEAD")


def append_successor(fixture: AdoptionFixture, *, broken: bool = False) -> None:
    """Retain the complete original shard and add one independently signed successor."""
    entry = fixture.repository / "authored/construction-adoptions"
    previous = {
        path.relative_to(entry): path.read_bytes() for path in entry.rglob("*.yaml")
    }
    original = next(
        record
        for record in fixture.records
        if record["kind"] == "construction_restriction"
    )
    old_shard = object_value(parse(previous[Path("restrictions/jp/standard/001.yaml")]))
    successor = object_value(parse(canonical(original)))
    data = object_value(successor["data"])
    data["adoption_no"] = 2
    data["predecessor"] = {
        "record_key": original["record_key"],
        "record_hash": "sha256:" + "f" * 64 if broken else digest(canonical(original)),
        "decision_id": old_shard["default_decision_id"],
    }
    data["reason"] = (
        "Synthetic second confirmation without changing the observed value."
    )
    successor["record_key"] = canonical(
        [successor["kind"], data["subject"], 2]
    ).decode()
    use = object_value(object_value(array(successor["evidence"])[0])["use"])
    locator = object_value(parse(str(use["locator"]).encode()))
    locator["record_key"] = successor["record_key"]
    use["locator"] = canonical(locator).decode()
    fixture.records = [successor]
    fixture.write()
    newest = (entry / "restrictions/jp/standard/001.yaml").read_bytes()
    for name, raw in previous.items():
        target = entry / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(raw)
    (entry / "restrictions/jp/standard/002.yaml").write_bytes(newest)
    index = object_value(parse(previous[Path("index.yaml")]))
    object_value(index["includes"])[
        "construction-adoptions/restrictions/jp/standard/002.yaml"
    ] = digest(newest)
    (entry / "index.yaml").write_bytes(canonical(index))
    git(fixture.repository, "add", "authored")
    git(fixture.repository, "commit", "-qm", "Synthetic append-only successor")
    fixture.revision = git(fixture.repository, "rev-parse", "HEAD")


def create(root: Path) -> AdoptionFixture:  # ruff: ignore[too-many-locals] -- Keep the complete admission boundary visible.
    inputs = create_seed(root)
    for name in REQUIRED_DEPENDENCIES:
        target = inputs.program / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes((REPO / name).read_bytes())
    git(inputs.program, "add", ".")
    git(inputs.program, "commit", "--allow-empty", "-qm", "Synthetic construction tool")
    tool_revision = git(inputs.program, "rev-parse", "HEAD")
    selection = json.loads(inputs.selection.read_bytes())
    selection["program_revision"] = tool_revision
    selection["sources"] = selection["sources"][:1]
    selection["source_import_format"] = 2
    selection["date_precision_approval"] = {
        "authority": "maintainer-approved-one-time",
        "basis": "Synthetic date observation authorization.",
    }
    inputs.selection.write_text(json.dumps(selection))
    observation = json.loads(
        (inputs.input_dir / "index.jsonl").read_text().splitlines()[0]
    )
    raw = synthetic_html()
    checksum = hashlib.sha256(raw).hexdigest()
    for unused in (inputs.input_dir / "raw").iterdir():
        unused.unlink()
    (inputs.input_dir / "raw" / checksum).write_bytes(raw)
    observation.update(
        sha256=checksum,
        bytes=len(raw),
        fetched_at=None,
        observed_on=DAY,
        observed_precision="date",
        observed_timezone="Asia/Taipei",
        observed_basis="Synthetic known date without an acquisition instant.",
    )
    (inputs.input_dir / "index.jsonl").write_text(json.dumps(observation) + "\n")
    acquired = root / "registered"
    register(prepare(inputs.input_dir, inputs.selection), acquired, inputs.program)
    store = ArchiveStore(
        acquired,
        acquired / "manifest/manifest.sqlite",
        acquired / "manifest/.lock",
        root / "archive",
        "synthetic-construction",
    )
    batch_id = seal_batch(store).batch_id
    lists = Lists({store.store_id: store.root}, (batch_id,))
    frozen = lists.batches[0]
    version = frozen.current_versions[0].source_version_id
    source = lists.source(batch_id, version)
    projection = lists.projection(batch_id, version, "jp")
    dependencies: list[JsonValue] = [
        {"name": name, "sha256": digest((inputs.program / name).read_bytes())}
        for name in REQUIRED_DEPENDENCIES
    ]
    context: dict[str, JsonValue] = {
        "context": {
            "program_revision": tool_revision,
            "configuration": canonical(
                {
                    "recipe": "construction-two-models-v1",
                    "scope": "current-list-day-v1",
                    "dependencies": dependencies,
                    "construction_review_stores": ["synthetic-reviews"],
                }
            ).decode(),
        },
        "source_batches": [{"batch_id": batch_id}],
    }
    profile_dependency: dict[str, JsonValue] = {
        "kind": "profile",
        "id": "standard",
        "region": "jp",
        "basis_hash": digest(
            canonical({"id": "standard", "region": "jp", "format_code": "standard"})
        ),
    }
    records: list[dict[str, JsonValue]] = []

    def add(
        kind: str,
        subject: dict[str, JsonValue],
        value: dict[str, JsonValue],
        pointer: str,
        result: JsonValue,
        dependencies: list[JsonValue],
    ) -> None:
        key = canonical([kind, subject, 1]).decode()
        use: dict[str, JsonValue] = {
            "source": source.model_dump(mode="json"),
            "usage": kind,
            "locator": canonical(
                {
                    "record_key": key,
                    "field": "value",
                    "context_key": "standard",
                    "number": None,
                    "location": {
                        "kind": "projection_pointer",
                        "pointer": pointer,
                        "page_numbers": [],
                    },
                    "result_hash": digest(canonical(result)),
                }
            ).decode(),
        }
        records.append(
            {
                "record_key": key,
                "kind": kind,
                "filing_key": "jp-standard",
                "data": {
                    "subject": subject,
                    "adoption_no": 1,
                    "predecessor": None,
                    "value": value,
                    "review_context_hash": "sha256:" + "0" * 64,
                    "dependencies": sorted(dependencies, key=canonical),
                    "reason": "Synthetic adoption.",
                },
                "evidence": [
                    {
                        "role": kind.removeprefix("construction_"),
                        "primary": True,
                        "use": use,
                    }
                ],
            }
        )

    add(
        "construction_profile",
        {"profile_id": "standard"},
        {
            "region": "jp",
            "format_code": "standard",
            "name": {"lang": "ja", "text": "Synthetic Standard"},
        },
        "",
        projection,
        [],
    )
    entries = projection["entries"]
    assert isinstance(entries, list)
    for index, (identifier, card, face, name) in enumerate(
        (
            ("rules_name", "card", "face", "Synthetic name"),
            ("rules_name2", "card1", "face1", "Synthetic two"),
        )
    ):
        entry = object_value(entries[index])
        deps: list[JsonValue] = [
            profile_dependency,
            {
                "kind": "rules_name",
                "id": identifier,
                "region": "jp",
                "basis_hash": digest(
                    canonical(
                        {
                            "id": identifier,
                            "region": "jp",
                            "official_name": name,
                            "faces": [{"face_id": face, "role": "primary"}],
                        }
                    )
                ),
            },
            {
                "kind": "card",
                "id": card,
                "region": "jp",
                "basis_hash": digest(
                    canonical(
                        {
                            "id": card,
                            "region": "jp",
                            "layout": "single",
                            "faces": [
                                {
                                    "id": face,
                                    "ordinal": 0,
                                    "side": "front",
                                    "type_code": "follower",
                                }
                            ],
                        }
                    )
                ),
            },
        ]
        add(
            "construction_restriction",
            {"restriction_id": "restriction" + str(index)},
            {
                "profile_id": "standard",
                "announced_on": None,
                "effective_from": "2020-01-01",
                "effective_until": None,
                "kind": "copy_limit",
                "state": "confirmed",
                "max_copies": index,
                "max_selected_groups": None,
                "members": [
                    {
                        "rules_name_id": identifier,
                        "choice_option": 0,
                        "deck_scope": "main",
                    }
                ],
            },
            "/entries/" + str(index),
            entry,
            deps,
        )
    add(
        "construction_coverage",
        {"profile_id": "standard", "from_date": DAY},
        {
            "until_date": "2020-01-03",
            "as_of": DAY,
            "state": "complete",
            "source_set": {
                "baseline": [version],
                "notices": [],
                "discovery": [],
                "excluded": [],
            },
            "reconciliation": {
                "mode": "current-list-day-v1",
                "observed_on": DAY,
                "observed_timezone": "Asia/Taipei",
                "projection_hash": digest(canonical(projection)),
                "excluded_entry_pointers": [],
            },
        },
        "",
        projection,
        [profile_dependency],
    )
    fixture = AdoptionFixture(
        inputs.program,
        root,
        store.root,
        store.store_id,
        batch_id,
        tool_revision,
        context,
        records,
    )
    fixture.write()
    return fixture
