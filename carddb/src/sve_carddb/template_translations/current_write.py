"""Compose editable, bounded template YAML using the shared translation index."""

from typing import TYPE_CHECKING

from sve_carddb.registry.storage import MAX_BYTES, TARGET_BYTES, encode
from sve_carddb.snapshot.values import digest
from sve_carddb.template_translations.current import shard
from sve_carddb.template_translations.current_models import Shard
from sve_carddb.template_translations.files import Files, json_bytes
from sve_carddb.translations.models import Index

if TYPE_CHECKING:
    from collections.abc import Iterator

    from pydantic import BaseModel

    from sve_carddb.template_translations.current_models import Record


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
    previous = Index.model_validate_json(json_bytes(files.index))
    preserved = tuple(
        item
        for item in files.content
        if not item[0].startswith("translations/templates/")
    )
    includes = {
        path: checksum
        for path, checksum in previous.includes.items()
        if not path.startswith("translations/templates/")
    }
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
            includes[path] = digest(parsed)
            content.append((path, raw, parsed))
    index = Index(
        translation_authored_format=2, kind="translation_index", includes=includes
    )
    if len(encode(index)) >= MAX_BYTES:
        raise ValueError("Current translation index exceeds authored size limit")
    if (
        set(previous.includes)
        - set(includes)
        - {p for p in previous.includes if p.startswith("translations/templates/")}
    ):
        raise ValueError("Current template composition cannot remove shared inputs")
    return Files(files.revision, encode(index), tuple(sorted(content)))
