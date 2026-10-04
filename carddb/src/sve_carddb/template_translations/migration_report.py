"""One prose-free processing list for new templates, unresolved drafts and quality flags."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.snapshot.values import array, object_value, parse

if TYPE_CHECKING:
    from sve_carddb.template_translations.current_models import (
        DefinitionRecord,
        TranslationRecord,
    )
    from sve_carddb.template_translations.migration_definitions import Definitions
    from sve_carddb.template_translations.migration_targets import Targets
    from sve_carddb.template_translations.sources import Reconstructed


def processing_list(  # ruff: ignore[complex-structure] -- each independent migration disposition remains visible in the one processing list
    definitions: Definitions,
    members: tuple[Reconstructed, ...],
    targets: tuple[Targets, ...],
    existing: tuple[DefinitionRecord, ...],
    matches: tuple[tuple[str, str], ...],
    *,
    source_report: bytes = b"[]",
) -> tuple[dict[str, JsonValue], ...]:
    """Affected cards count distinct source pages, never paragraph frequency as owner count."""
    pages = {m.entry.id: m.entry.source_ref.source_version_id for m in members}
    matched: dict[str, set[str]] = {}
    for entry_id, identifier in matches:
        matched.setdefault(identifier, set()).add(entry_id)
    rows: list[dict[str, JsonValue]] = []

    def add(
        kind: str,
        identifier: str,
        reason: str,
        entries: tuple[str, ...] | set[str],
        recommendation: str,
    ) -> None:
        rows.append(
            {
                "type": kind,
                "data_id": identifier,
                "reason": reason,
                "affected_cards": len({pages[e] for e in entries if e in pages}),
                "recommendation": recommendation,
            }
        )

    for entry_id, reasons in definitions.issues:
        for reason in reasons:
            add(
                "source",
                entry_id,
                reason,
                (entry_id,),
                "核對來源分類或精確概念引用；維持原文。",
            )
    rows.extend(_coverage_rows(source_report))
    originals = {r.data.id for r in existing}
    translations: dict[str, TranslationRecord] = {}
    for target in targets:
        for record in target.records:
            translations[record.data.template_id] = record
        for pending in target.pending:
            for reason in pending.reasons:
                add(
                    pending.kind,
                    pending.identifier,
                    reason,
                    pending.entries,
                    "保留候選原稿，依來源位置補具名參數或概念引用。",
                )
            if pending.low_confidence:
                add(
                    pending.kind,
                    pending.identifier,
                    "low_confidence",
                    pending.entries,
                    "候選仍待結構修正；保留既有低信心標記。",
                )
    for definition in definitions.records:
        identifier = definition.data.id
        entries = matched.get(identifier, set())
        if identifier not in originals:
            add(
                "template",
                identifier,
                "new_template",
                entries,
                "已自動驗回來源；可於普通資料 PR 修訂譯文。",
            )
        translated = translations.get(identifier)
        if translated is None:
            add(
                "template",
                identifier,
                "missing_translation",
                entries,
                "補上具名參數譯文；目前回原文。",
            )
        elif translated.low_confidence:
            add(
                "translation",
                identifier,
                "low_confidence",
                entries,
                "有效譯文照常顯示並標待校對，可切回原文。",
            )
    return tuple(
        sorted(
            rows, key=lambda r: (str(r["type"]), str(r["data_id"]), str(r["reason"]))
        )
    )


def _coverage_rows(source_report: bytes) -> list[dict[str, JsonValue]]:
    rows: list[dict[str, JsonValue]] = []
    reports = parse(source_report)
    for report in array(reports) if isinstance(reports, list) else [reports]:
        value = object_value(report)
        coverage = object_value(value.get("effect_coverage", {}))
        for failure in array(coverage.get("failures", [])):
            item = object_value(failure)
            rows.append(
                {
                    "type": "source_coverage",
                    "data_id": str(item["source_version_id"])
                    + "#"
                    + str(item["locator"]),
                    "reason": str(item["reason"]),
                    "affected_cards": 1,
                    "recommendation": "核對來源版型或缺漏；來源覆蓋仍未完成。",
                }
            )
        for field in array(value.get("flavor_fields", [])):
            item = object_value(field)
            if item["state"] == "unknown":
                rows.append(
                    {
                        "type": "source_coverage",
                        "data_id": str(item["source_version_id"])
                        + "#"
                        + str(item["locator"]),
                        "reason": "unknown_flavor_source",
                        "affected_cards": 1,
                        "recommendation": "風味來源未知；不以空字串或零參數模板代替。",
                    }
                )
    return rows
