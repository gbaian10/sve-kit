"""Replay pinned source recipes from sealed inputs, never from a latest cache."""

import dataclasses
import re
import shutil
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- immutable Git blobs are read with an argument vector and no shell
import tempfile
from pathlib import Path, PurePosixPath
from typing import TYPE_CHECKING
from urllib.parse import parse_qs, urlsplit

from pydantic import JsonValue, ValidationError

from sve_carddb.build_inputs import SourceUse
from sve_carddb.catalog.adoption_models import (
    AuthoredText,
    ImageEvidence,
    Normalizer,
    SourceRef,
    SourceText,
    TextEvidence,
)
from sve_carddb.extract import official_en, official_jp
from sve_carddb.extract.compare_jp import legacy_projection
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.products.models import LocalizedText
from sve_carddb.registry.inputs import JSON_VALUE
from sve_carddb.registry.records import Observation, PrintingData
from sve_carddb.registry.review import observation
from sve_carddb.registry.snapshot import load_registry
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import canonical, digest, object_value, parse
from sve_carddb.sources import official_en as en
from sve_carddb.sources import official_jp as jp

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build_inputs import BuildContext, Source
    from sve_carddb.catalog.adoption_models import Record, ReviewContext, TextValue
    from sve_carddb.catalog.current_models import (
        VocabularyRecord as CurrentVocabularyRecord,
    )
    from sve_carddb.registry.snapshot import RegistrySnapshot

SOURCE_RECIPE_PATHS = {
    "official-jp-exact-v1": "carddb/src/sve_carddb/extract/official_jp.py",
    "official-en-exact-v1": "carddb/src/sve_carddb/extract/official_en.py",
    "exact-json-v1": "carddb/src/sve_carddb/snapshot/values.py",
}


class PinnedRepository:
    def __init__(self, root: Path) -> None:
        self.root = root
        self.cache: dict[tuple[str, str], bytes] = {}
        executable = shutil.which("git")
        if executable is None:
            raise ValueError("Git is required for immutable recipe replay")
        self.executable = executable

    def read(self, revision: str, name: str) -> bytes:
        """Read immutable Git blobs without executing code or following filesystem links."""
        path = PurePosixPath(name)
        if path.is_absolute() or ".." in path.parts or path.as_posix() != name:
            raise ValueError("Unsafe recipe/dependency code path")
        key = revision, name
        if key not in self.cache:
            result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- no shell; revision and portable path are validated inputs
                [
                    self.executable,
                    "-C",
                    str(self.root),
                    "cat-file",
                    "blob",
                    f"{revision}:{name}",
                ],
                check=False,
                capture_output=True,
            )
            if result.returncode:
                raise ValueError("Pinned immutable dependency unavailable")
            self.cache[key] = result.stdout
        return self.cache[key]

    def read_many(self, revision: str, names: tuple[str, ...]) -> dict[str, bytes]:
        """Read a dependency closure in one Git process, without extracting archive paths."""
        import io  # ruff: ignore[import-outside-top-level] -- batch framing is only needed by complete dependency replay

        if re.fullmatch(r"[0-9a-f]{40}", revision) is None:
            raise ValueError("Immutable batch revision must be a full Git SHA")
        missing = tuple(name for name in names if (revision, name) not in self.cache)
        for name in missing:
            path = PurePosixPath(name)
            if (
                path.is_absolute()
                or ".." in path.parts
                or path.as_posix() != name
                or any(c in name for c in "\r\n\x00")
            ):
                raise ValueError("Unsafe batch dependency path")
        if missing:
            result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed Git batch process; immutable object names are validated
                [self.executable, "-C", str(self.root), "cat-file", "--batch"],
                input="".join(f"{revision}:{name}\n" for name in missing).encode(),
                check=False,
                capture_output=True,
            )
            if result.returncode:
                raise ValueError("Immutable dependency batch is unavailable")
            stream = io.BytesIO(result.stdout)
            content = {}
            for name in missing:
                header = stream.readline().rstrip(b"\n").split(b" ")
                if (
                    len(header) != len(("oid", "kind", "size"))
                    or header[1] != b"blob"
                    or not header[2].isdigit()
                ):
                    raise ValueError(
                        "Immutable dependency batch contains a missing/non-blob object"
                    )
                raw = stream.read(int(header[2]))
                if len(raw) != int(header[2]) or stream.read(1) != b"\n":
                    raise ValueError("Invalid immutable Git batch framing")
                content[revision, name] = raw
            if stream.read():
                raise ValueError("Unexpected immutable Git batch output")
            self.cache.update(content)
        return {name: self.cache[revision, name] for name in names}

    def context(self, context: BuildContext) -> None:
        """Verify every explicitly declared program/dependency byte pin."""
        for pin in context.dependencies:
            if digest(self.read(context.program_revision, pin.name)) != pin.sha256:
                raise ValueError("Review dependency hash mismatch")

    def implementation(
        self, pin: Normalizer, context: BuildContext, *, current_runtime: bool = True
    ) -> None:
        """Verify immutable recipe provenance and, for current builds, loaded code."""
        content = self.read(pin.program_revision, pin.code_path)
        if (
            digest(content) != pin.code_hash
            or digest(canonical(pin.config)) != pin.config_hash
        ):
            raise ValueError("Recipe program/config hash mismatch")
        expected = {p.name: p.sha256 for p in context.dependencies}
        if expected.get(pin.code_path) != pin.code_hash:
            raise ValueError("Recipe code is absent from review dependencies")
        if not current_runtime:
            return
        path = Path(__file__).resolve().parents[4] / pin.code_path
        if (
            path.is_symlink()
            or not path.is_file()
            or digest(path.read_bytes()) != pin.code_hash
        ):
            raise ValueError("Historical recipe implementation cannot be replayed")


class AdoptionSources:
    def __init__(
        self,
        stores: Mapping[str, Path],
        repository: PinnedRepository,
        *,
        historical: bool = False,
    ) -> None:
        self.stores = dict(stores)
        self.repository = repository
        self.historical = historical
        self.batches: dict[tuple[str, str], FrozenSources] = {}
        self.uses: list[SourceUse] = []
        self.cache: dict[bytes, tuple[LocalizedText, Source, JsonValue]] = {}
        self.registries: dict[bytes, RegistrySnapshot] = {}

    def recipe(self, parser: str, context: BuildContext) -> Normalizer:
        """Resolve a fully pinned recipe for historical or current frozen observations."""
        config = parse(context.configuration.encode())
        if not isinstance(config, dict) or not isinstance(
            recipes := config.get("catalog_source_recipes"), dict
        ):
            raise ValueError("Catalog source recipes must be an object")  # ruff: ignore[type-check-without-type-error] -- expose a domain refusal at the catalog entry, not an incidental boundary TypeError
        try:
            pin = Normalizer.model_validate_json(canonical(recipes.get(parser)))
        except ValidationError:
            raise ValueError("Invalid pinned source recipe fields") from None
        if pin.version != parser:
            raise ValueError("Source parser recipe ID mismatch")
        _source_recipe(pin)
        self.repository.implementation(
            pin, context, current_runtime=not self.historical
        )
        self._runtime(pin, context)
        return pin

    def registry(self, review: ReviewContext) -> RegistrySnapshot:
        """Replay the complete reviewed registry from its immutable authored revision."""
        configuration = object_value(parse(review.context.configuration.encode()))
        pin = object_value(configuration.get("catalog_registry"))
        if (
            set(pin) != {"authored_revision", "index_path", "index_hash"}
            or pin["index_path"] != "authored/ids/index.yaml"
        ):
            raise ValueError("Historical adoption registry pin is incomplete")
        key = canonical(pin)
        if key not in self.registries:
            revision = pin["authored_revision"]
            if not isinstance(revision, str) or not re.fullmatch(
                r"[0-9a-f]{40}", revision
            ):
                raise ValueError("Historical registry revision must be a full Git SHA")
            content = self.repository.read(revision, "authored/ids/index.yaml")
            with tempfile.TemporaryDirectory(prefix="catalog-review-") as name:
                root = Path(name)
                (root / "ids").mkdir()
                (root / "ids/index.yaml").write_bytes(content)
                raw = read_yaml(root / "ids/index.yaml")
                if digest(canonical(raw)) != pin["index_hash"]:
                    raise ValueError("Historical registry index hash mismatch")
                includes = object_value(object_value(raw)["includes"])
                for relative in includes:
                    path = PurePosixPath(relative)
                    if (
                        path.is_absolute()
                        or ".." in path.parts
                        or path.as_posix() != relative
                        or path.parts[0] not in {"ids", "registry"}
                    ):
                        raise ValueError("Unsafe historical registry shard")
                    target = root / relative
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes(
                        self.repository.read(revision, "authored/" + relative)
                    )
                self.registries[key] = load_registry(root)
        return self.registries[key]

    def association(
        self, ref: SourceRef, printing_id: str, face_id: str, review: ReviewContext
    ) -> None:
        """Verify source version, exact card URL, language and the reviewed source-face map."""
        registry = self.registry(review)
        found = [
            r.data
            for r in registry.records.values()
            if isinstance(r.data, PrintingData) and r.data.id == printing_id
        ]
        if len(found) != 1:
            raise ValueError("Historical name observation printing is absent")
        printing = found[0]
        source = self.text(ref, review)[1]
        provider = jp if printing.region == "jp" else en
        expected = {
            f"/faces/{mapping.source_index}/name"
            for mapping in printing.source_face_map
            if mapping.face_id == face_id
        }
        if (
            source.url != provider.card_url(printing.card_no)
            or ref.locator not in expected
        ):
            raise ValueError(
                "Historical name observation printing/face/source mismatch"
            )
        self.verify_printing(printing, review, ref.source_version_id)

    def printing_sources(
        self, printing: PrintingData, review: ReviewContext
    ) -> set[str]:
        """Reconstruct reviewed physical source closure from all pinned batch inventories."""
        provider = jp if printing.region == "jp" else en
        versions = {
            item.source_version_id
            for pin in review.source_batches
            for item in self.batch(pin.store_id, pin.batch_id).inventory.current
            if item.url == provider.card_url(printing.card_no)
        }
        if not versions:
            raise ValueError("Reviewed printing frozen source coverage is incomplete")
        return versions

    def verify_printing(
        self, printing: PrintingData, review: ReviewContext, version: str
    ) -> None:
        """Replay the reviewed complete observation, not just its name or card URL."""
        if version not in self.printing_sources(printing, review):
            raise ValueError("Historical printing source is outside reviewed closure")
        parser = (
            "official-jp-exact-v1"
            if printing.region == "jp"
            else "official-en-exact-v1"
        )
        config = object_value(parse(review.context.configuration.encode()))
        recipes = object_value(config.get("catalog_source_recipes"))
        try:
            pin = Normalizer.model_validate_json(canonical(recipes.get(parser)))
        except ValidationError:
            raise ValueError("Reviewed printing recipe is absent") from None
        if pin.version != parser:
            raise ValueError("Reviewed printing recipe ID mismatch")
        self.repository.implementation(
            pin,
            review.context,
            current_runtime=not self.historical,
        )
        self._runtime(pin, review.context)
        for batch in review.source_batches:
            frozen = self.batch(batch.store_id, batch.batch_id)
            if not any(
                item.source_version_id == version for item in frozen.inventory.current
            ):
                continue
            source, raw, _ = frozen.read(version, parser_version=parser)
            self._projection(pin, raw, source.url)
            card = (
                legacy_projection(
                    official_jp.extract_card(raw, number=printing.card_no)
                )
                if printing.region == "jp"
                else official_en.legacy_projection(
                    official_en.extract_card(raw, number=printing.card_no)
                )
            )
            actual = Observation.model_validate_json(
                canonical(observation(card, printing.region))
            )
            if actual != printing.observation:
                raise ValueError(
                    "Historical printing identity observation cannot be replayed"
                )
            self._use(source, "catalog_reviewed_identity", {"printing_id": printing.id})

    def image_association(self, evidence: ImageEvidence, review: ReviewContext) -> None:
        """Resolve historical image evidence through its reviewed printing and source face."""
        ref = evidence.image_ref
        source = self.image(evidence)
        records = [
            r.data
            for r in self.registry(review).records.values()
            if isinstance(r.data, PrintingData) and r.data.id == ref.printing_id
        ]
        if len(records) != 1:
            raise ValueError("Image adoption printing/face association mismatch")
        printing = records[0]
        maps = [m for m in printing.source_face_map if m.face_id == ref.face_id]
        if len(maps) != 1:
            raise ValueError("Image adoption printing/face association mismatch")
        for version in self.printing_sources(printing, review):
            self.verify_printing(printing, review, version)
            for batch in review.source_batches:
                frozen = self.batch(batch.store_id, batch.batch_id)
                if not any(
                    i.source_version_id == version for i in frozen.inventory.current
                ):
                    continue
                _, raw, _ = frozen.read(
                    version, parser_version="catalog-image-association-v1"
                )
                card = (
                    official_jp.extract_card(raw, number=printing.card_no)
                    if printing.region == "jp"
                    else official_en.extract_card(raw, number=printing.card_no)
                )
                images = tuple(face.image for face in card.faces)
                if images[maps[0].source_index] == source.url:
                    return
        raise ValueError("Image adoption printing/face association mismatch")

    def batch(self, store: str, batch: str) -> FrozenSources:
        """Validate the complete descriptor/receipt/raw closure once per frozen batch."""
        key = store, batch
        if key not in self.batches:
            root = self.stores.get(store)
            if root is None:
                raise ValueError("Adoption source store is not configured")
            self.batches[key] = FrozenSources(root, *key)
        return self.batches[key]

    def verify(self, record: Record, review: ReviewContext) -> None:
        """Verify every evidence item, including historical and withdrawn members."""
        self.repository.context(review.context)
        for batch in review.source_batches:
            self.batch(batch.store_id, batch.batch_id)
        for evidence in record.evidence:
            if isinstance(evidence, TextEvidence):
                self.text(evidence.source_ref, review)
            else:
                self.image(evidence)

    def text(
        self, ref: SourceRef, review: ReviewContext
    ) -> tuple[LocalizedText, Source, JsonValue]:
        """Resolve JSON Pointer and hash the exact nonempty UTF-8 string."""
        key = canonical([ref.model_dump(mode="json"), review.model_dump(mode="json")])
        if key not in self.cache:
            config = object_value(parse(review.context.configuration.encode()))
            recipes = object_value(config.get("catalog_source_recipes"))
            try:
                pin = Normalizer.model_validate_json(canonical(recipes.get(ref.parser)))
            except ValidationError:
                raise ValueError("Invalid pinned source recipe fields") from None
            if pin.version != ref.parser:
                raise ValueError("Source parser recipe ID mismatch")
            self.repository.implementation(
                pin,
                review.context,
                current_runtime=not self.historical,
            )
            self._runtime(pin, review.context)
            source, raw, descriptor = self.batch(ref.store_id, ref.batch_id).read(
                ref.source_version_id,
                parser_version=pin.version,
            )
            projection = self._projection(pin, raw, descriptor.url)
            value = pointer(projection, ref.locator)
            if (
                not isinstance(value, str)
                or not value
                or digest(value.encode()) != ref.text_hash
            ):
                raise ValueError("Source locator/exact text hash mismatch")
            if descriptor.provider not in {"jp", "en"}:
                raise ValueError("Source language cannot be determined")
            lang = "ja" if descriptor.provider == "jp" else "en"
            self.cache[key] = LocalizedText(lang=lang, text=value), source, projection
            self._use(source, "catalog_exact_text", ref.model_dump(mode="json"))
        return self.cache[key]

    @staticmethod
    def _projection(pin: Normalizer, raw: bytes, url: str) -> JsonValue:
        """Historical provenance is checked separately from fixed installed execution."""
        return _projection(pin, raw, url)

    def _runtime(self, pin: Normalizer, context: BuildContext) -> None:
        # Historical recipe closures cannot grow without a new recipe version.
        required = {
            "carddb/uv.lock",
            "carddb/pyproject.toml",
            "carddb/src/sve_carddb/catalog/adoption_sources.py",
        }
        if pin.version != "exact-json-v1":
            required.update(
                {
                    "carddb/src/sve_carddb/html.py",
                    "carddb/src/sve_carddb/fetch/validate.py",
                    "carddb/src/sve_carddb/sources/official_jp.py",
                    "carddb/src/sve_carddb/extract/official_jp.py",
                    "carddb/src/sve_carddb/extract/compare_jp.py",
                    "carddb/src/sve_carddb/registry/inputs.py",
                    "carddb/src/sve_carddb/registry/review.py",
                }
            )
        if pin.version == "official-en-exact-v1":
            required.add("carddb/src/sve_carddb/sources/official_en.py")
        required.add(pin.code_path)
        dependencies = {p.name: p.sha256 for p in context.dependencies}
        if self.historical:
            if not required <= dependencies.keys():
                raise ValueError(
                    "Source parser runtime/dependency closure cannot be replayed"
                )
            files = self.repository.read_many(
                context.program_revision, tuple(sorted(required))
            )
            if any(dependencies[name] != digest(raw) for name, raw in files.items()):
                raise ValueError("Review dependency hash mismatch")
            return
        runtime = Path(__file__).resolve().parents[4]
        for name in required:
            path = runtime / name
            if (
                path.is_symlink()
                or not path.is_file()
                or dependencies.get(name) != digest(path.read_bytes())
            ):
                raise ValueError(
                    "Source parser runtime/dependency closure cannot be replayed"
                )

    def value(
        self,
        value: TextValue,
        record: Record | CurrentVocabularyRecord,
        review: ReviewContext,
    ) -> LocalizedText:
        """Source TextValues must be part of the same human-approved evidence set."""
        if isinstance(value, AuthoredText):
            return LocalizedText(lang=value.lang, text=value.text)
        assert isinstance(value, SourceText)
        if not any(
            isinstance(e, TextEvidence) and e.source_ref == value.source_ref
            for e in record.evidence
        ):
            raise ValueError("Source TextValue is absent from adoption evidence")
        return self.text(value.source_ref, review)[0]

    def image(self, evidence: ImageEvidence) -> Source:
        """Keep image raw hashes separate from string hashes and parser recipes."""
        ref = evidence.image_ref
        source, _, _ = self.batch(ref.store_id, ref.batch_id).read(
            ref.source_version_id,
            parser_version="catalog-image-closure-v1",
        )
        if source.kind != "image" or source.sha256 != ref.raw_hash:
            raise ValueError("Adoption image descriptor/raw hash mismatch")
        self._use(source, "catalog_image_evidence", ref.model_dump(mode="json"))
        return source

    def _use(self, source: Source, usage: str, locator: JsonValue) -> None:
        self.uses.append(
            SourceUse(source=source, usage=usage, locator=canonical(locator).decode())
        )


def pointer(value: JsonValue, locator: str) -> JsonValue:
    """Use RFC 6901 indexing only; no executable expressions or implicit normalization."""
    if not locator:
        return value
    if not locator.startswith("/"):
        raise ValueError("Invalid adoption JSON Pointer")
    for segment in locator.split("/")[1:]:
        if re.search(r"~(?![01])", segment):
            raise ValueError("Invalid adoption JSON Pointer escape")
        key = segment.replace("~1", "/").replace("~0", "~")
        if isinstance(value, dict) and key in value:
            value = value[key]
        elif (
            isinstance(value, list)
            and key.isascii()
            and key.isdecimal()
            and str(int(key)) == key
            and int(key) < len(value)
        ):
            value = value[int(key)]
        else:
            raise ValueError("Adoption source locator is absent")
    return value


def _source_recipe(pin: Normalizer) -> None:
    if pin.config:
        raise ValueError("Unsupported source recipe configuration")
    if SOURCE_RECIPE_PATHS.get(pin.version) != pin.code_path:
        raise ValueError("Unsupported source parser recipe")


def _projection(pin: Normalizer, raw: bytes, url: str) -> JsonValue:
    _source_recipe(pin)
    if pin.version == "exact-json-v1":
        return parse(raw)
    number = parse_qs(urlsplit(url).query).get("cardno", [])
    if len(number) != 1:
        raise ValueError("Card source lacks exact official number")
    result = (
        official_jp.extract_card(raw, number=number[0])
        if pin.version == "official-jp-exact-v1"
        else official_en.extract_card(raw, number=number[0])
    )
    return JSON_VALUE.validate_python(dataclasses.asdict(result), strict=True)
