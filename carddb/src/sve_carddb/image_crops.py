"""Load the complete, source-bound crop adoption closure from a pinned revision."""

import re
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- enumerate immutable Git objects without executing repository code
from dataclasses import dataclass
from types import MappingProxyType
from typing import TYPE_CHECKING, Annotated, Literal

from pydantic import Field, JsonValue, ValidationError, field_validator, model_validator

from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.image_variants import CropBox, CropOverride
from sve_carddb.registry.inputs import JSON_VALUE
from sve_carddb.registry.records import Hash, RecordData, Region
from sve_carddb.registry.storage import MAX_BYTES
from sve_carddb.registry.yaml_reader import parse_yaml
from sve_carddb.snapshot.values import canonical, digest, parse

if TYPE_CHECKING:
    from collections.abc import Mapping
    from pathlib import Path

    from sve_carddb.build_inputs import BuildContext
    from sve_carddb.source_archive import Descriptor

PREFIX = "authored/image-crops/"
_SHARD = re.compile(r"authored/image-crops/[A-Za-z0-9_-]+/[0-9]{3,}\.yaml\Z")
HexHash = Annotated[str, Field(pattern=r"^[0-9a-f]{64}\Z")]
Nonnegative = Annotated[int, Field(ge=0)]
Positive = Annotated[int, Field(gt=0)]


def conversion_image_id(source_version_id: str) -> str:
    """Keep conversion IDs distinct from the later HTML binding IDs."""
    return "img:v1:" + digest(canonical({"source_id": source_version_id}))[7:]


class CropRecord(RecordData):
    source_key: Hash
    source_sha256: HexHash
    left: Nonnegative
    top: Nonnegative
    width: Positive
    height: Positive
    reason: str
    region: Region
    card_no: str

    @model_validator(mode="after")
    def _ratio(self) -> CropRecord:
        if self.width * 3 != self.height * 4:
            raise ValueError("Crop must have an exact 4:3 ratio")
        return self

    @property
    def key(self) -> tuple[str, str]:
        """One adopted box per resource/version, independent of identity repairs."""
        return self.source_key, self.source_sha256

    @property
    def image_id(self) -> str:
        """Derive the existing API ID instead of asking authors to copy it."""
        version = (
            "src:v1:"
            + digest(
                canonical(
                    {
                        "source_key": self.source_key,
                        "raw_sha256": "sha256:" + self.source_sha256,
                    }
                )
            )[7:]
        )
        return conversion_image_id(version)

    def box(self) -> CropBox:
        """Return oriented integer coordinates without rotating the artwork."""
        return CropBox(self.left, self.top, self.width, self.height)

    @field_validator("reason", "card_no")
    @classmethod
    def _nonblank(cls, value: str) -> str:
        if not value.strip():
            raise ValueError("Crop annotation must not be blank")
        return value

    def override(self) -> CropOverride:
        """Supply the unchanged low-level encoder boundary."""
        return CropOverride(
            self.image_id,
            self.source_sha256,
            self.left,
            self.top,
            self.width,
            self.height,
            self.reason,
        )


class CropShard(RecordData):
    image_crop_format: Annotated[int, Field(ge=2, le=2)]
    kind: Literal["crop_override_shard"]
    records: Annotated[tuple[CropRecord, ...], Field(min_length=1)]


@dataclass(frozen=True)
class ImageCrops:
    revision: str
    files: Mapping[str, bytes]
    records: Mapping[tuple[str, str], CropRecord]

    def dependencies(self) -> dict[str, bytes]:
        """Pin unused regional rows along with the selected crops."""
        return dict(self.files)

    def configuration(self) -> dict[str, JsonValue]:
        """Declare the complete no-index closure, including an explicitly empty one."""
        return {
            "image_crop_format": 2,
            "authored_revision": self.revision,
            "files": [
                {"name": name, "sha256": digest(raw)}
                for name, raw in sorted(self.files.items())
            ],
        }

    def verify_context(self, build: BuildContext) -> None:
        """Caller-provided pins may check but cannot replace authored crop inputs."""
        config = parse(build.configuration.encode())
        if not isinstance(config, dict) or canonical(
            config.get("image_crop_overrides")
        ) != canonical(self.configuration()):
            raise ValueError("Image crop configuration pin mismatch")
        actual = {
            pin.name: pin.sha256
            for pin in build.dependencies
            if pin.name.startswith(PREFIX)
        }
        if actual != {name: digest(raw) for name, raw in self.files.items()}:
            raise ValueError("Image crop dependency closure mismatch")

    def override(self, descriptor: Descriptor) -> CropOverride | None:
        """A known resource with new bytes must never fall back to the standard crop."""
        if descriptor.kind != "image" or descriptor.provider not in {"jp", "en"}:
            raise ValueError("Crop source must be a JP or EN image")
        key = descriptor.source_key, descriptor.raw_sha256.removeprefix("sha256:")
        known = sorted(
            checksum for source_key, checksum in self.records if source_key == key[0]
        )
        record = self.records.get(key)
        if record is None and known:
            raise ValueError(
                f"Unadopted crop source version: source_key={key[0]} new_hash={key[1]} adopted_hashes={','.join(known)}"
            )
        if record is None:
            return None
        if record.image_id != conversion_image_id(descriptor.id):
            raise ValueError("Crop source version ID differs from adopted key")
        return record.override()


def _model[T: RecordData](model: type[T], data: bytes) -> T:
    try:
        if len(data) >= MAX_BYTES:
            raise ValueError("Oversized image crop YAML")
        raw = JSON_VALUE.validate_python(parse_yaml(data), strict=True)
        return model.model_validate_json(canonical(raw))
    except ValidationError as error:
        details = "; ".join(
            ".".join(map(str, issue["loc"])) + ":" + issue["type"]
            for issue in error.errors(include_input=False, include_context=False)
        )
        raise ValueError("Invalid image crop fields: " + details) from None


def _inventory(repository: PinnedRepository, revision: str) -> tuple[str, ...]:
    result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed Git tree read; full revision validated by loader
        [
            repository.executable,
            "-C",
            str(repository.root),
            "ls-tree",
            "-r",
            "-z",
            revision,
            "--",
            PREFIX.removesuffix("/"),
        ],
        capture_output=True,
        check=False,
    )
    if result.returncode:
        raise ValueError("Pinned image crop revision is unavailable")
    names = []
    for entry in result.stdout.split(b"\0"):
        if not entry:
            continue
        metadata, raw_name = entry.split(b"\t", 1)
        mode, kind, _oid = metadata.split(b" ")
        if mode not in {b"100644", b"100755"} or kind != b"blob":
            raise ValueError("Image crop Git inputs must be regular files")
        name = raw_name.decode("utf-8")
        if name.lower().endswith((".yaml", ".yml")):
            names.append(name)
    return tuple(sorted(names))


def _disk(root: Path) -> dict[str, bytes]:
    directory = root / "image-crops"
    if root.is_symlink() or directory.is_symlink():
        raise ValueError("Symlinks are forbidden in image crop inputs")
    if not directory.exists():
        return {}
    if not directory.is_dir():
        raise ValueError("Image crop input must be a directory")
    files = {}
    for path in sorted(directory.rglob("*")):
        if path.is_symlink():
            raise ValueError("Symlinks are forbidden in image crop inputs")
        if path.is_file() and path.suffix.lower() in {".yaml", ".yml"}:
            files["authored/" + path.relative_to(root).as_posix()] = path.read_bytes()
    return files


def load_image_crops(root: Path, *, authored_revision: str) -> ImageCrops:
    """Validate all shards before selecting any region or source."""
    if re.fullmatch(r"[0-9a-f]{40}", authored_revision) is None:
        raise ValueError("Image crop revision must be a full Git SHA")
    repository = PinnedRepository(root.parent)
    names = _inventory(repository, authored_revision)
    files = _disk(root)
    if tuple(sorted(files)) != names:
        raise ValueError("Image crop file closure differs from pinned revision")
    pinned = repository.read_many(authored_revision, names)
    if files != pinned:
        raise ValueError("Image crop bytes differ from pinned authored revision")
    records = _records(files)
    return ImageCrops(
        authored_revision, MappingProxyType(files), MappingProxyType(records)
    )


def _records(files: dict[str, bytes]) -> dict[tuple[str, str], CropRecord]:
    records: dict[tuple[str, str], CropRecord] = {}
    for name, raw in sorted(files.items()):
        if _SHARD.fullmatch(name) is None:
            raise ValueError("Unsafe image crop input path")
        shard = _model(CropShard, raw)
        keys = tuple((r.region, r.card_no, *r.key) for r in shard.records)
        if keys != tuple(sorted(keys)):
            raise ValueError("Image crop shard records must be sorted")
        for record in shard.records:
            if record.key in records:
                raise ValueError("Duplicate global image crop key")
            records[record.key] = record
    return records
