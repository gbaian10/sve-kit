"""Synthetic current-template packages for reader and validation tests."""

from sve_carddb.core.json import canonical
from sve_carddb.domains.translations.templates.files import Files
from sve_carddb.domains.translations.templates.records import Record, Shard


def template_files(files: Files, records: tuple[Record, ...]) -> Files:
    preserved = tuple(
        item
        for item in files.content
        if not item[0].startswith("translations/templates/")
    )
    content = list(preserved)
    for kind in sorted({record.kind for record in records}):
        shard = Shard(
            translation_authored_format=2,
            kind="translation_shard",
            records=tuple(record for record in records if record.kind == kind),
        )
        raw = canonical(shard.model_dump(mode="json", round_trip=True))
        content.append((f"translations/templates/{kind}/001.yaml", raw, raw))
    return Files(files.revision, tuple(sorted(content)))
