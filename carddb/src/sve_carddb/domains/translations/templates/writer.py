"""Compose editable, bounded template YAML in fixed data areas."""

from typing import TYPE_CHECKING

from sve_carddb.core.yaml import MAX_BYTES
from sve_carddb.domains.registry.storage import TARGET_BYTES, encode
from sve_carddb.domains.translations.templates.files import Files, json_bytes
from sve_carddb.domains.translations.templates.loader import shard
from sve_carddb.domains.translations.templates.records import Shard

if TYPE_CHECKING:
    from collections.abc import Iterator

    from pydantic import BaseModel

    from sve_carddb.domains.translations.templates.records import Record


CHUNK_VALUES = 256


def _chunks(model: BaseModel, field: str) -> Iterator[bytes]:
    values: tuple[object, ...] = getattr(model, field)
    if len(values) > CHUNK_VALUES:
        for start in range(0, len(values), CHUNK_VALUES):
            yield from _chunks(
                model.model_copy(update={field: values[start : start + CHUNK_VALUES]}),
                field,
            )
        return
    raw = encode(model)
    if len(raw) > TARGET_BYTES and len(values) > 1:
        midpoint = len(values) // 2
        yield from _chunks(model.model_copy(update={field: values[:midpoint]}), field)
        yield from _chunks(model.model_copy(update={field: values[midpoint:]}), field)
    elif len(raw) >= MAX_BYTES:
        raise ValueError("Current template record exceeds authored size limit")
    else:
        yield raw


def compose(files: Files, records: tuple[Record, ...]) -> Files:
    """Replace only the template area; retain all indexed shared glossary and override bytes."""
    preserved = tuple(
        item
        for item in files.content
        if not item[0].startswith("translations/templates/")
    )
    content = list(preserved)
    for kind in sorted({r.kind for r in records}):
        selected = tuple(
            sorted((r for r in records if r.kind == kind), key=lambda r: r.record_key)
        )
        model = Shard(
            translation_authored_format=2, kind="translation_shard", records=selected
        )
        for number, raw in enumerate(_chunks(model, "records"), start=1):
            path = f"translations/templates/{kind}/{number:03d}.yaml"
            parsed = json_bytes(raw)
            shard(parsed)
            content.append((path, raw, parsed))
    return Files(files.revision, tuple(sorted(content)))
