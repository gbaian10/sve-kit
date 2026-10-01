"""Compose supplemental evidence atomically without adopting card text or coverage."""

from collections import defaultdict
from typing import TYPE_CHECKING

from sve_carddb.build_inputs import input_record, insert_raw_sources
from sve_carddb.card_extras.errata import populate_errata
from sve_carddb.card_extras.models import key
from sve_carddb.card_extras.plan import plan_card_extras
from sve_carddb.products.models import LocalizedText
from sve_carddb.snapshot.values import canonical, parse
from sve_carddb.text_observations.intern import TextInterner

if TYPE_CHECKING:
    from sve_carddb.build_db import Database
    from sve_carddb.build_inputs import BuildContext, InputRecord
    from sve_carddb.card_extras.plan import ExtrasPlan, QAVersion


def populate_card_extras(
    db: Database, plan: ExtrasPlan, *, build: BuildContext
) -> InputRecord:
    """Populate in the composing transaction; recheck plans against actual identities."""
    if plan != plan_card_extras(db, plan.pages, errata=plan.errata):
        raise ValueError("Card extras plan differs from adopted identities")
    configuration = parse(build.configuration.encode())
    if (
        not isinstance(configuration, dict)
        or configuration.get("card_extras") != plan.configuration()
    ):
        raise ValueError("Card extras build configuration mismatch")
    expected = plan.source_uses()
    insert_raw_sources(db, (use.source for use in expected))
    texts = TextInterner(db)
    _questions(db, plan, texts)
    for relation in plan.relations:
        db.insert(
            "card_related",
            {
                "id": relation.id,
                "from_card_id": relation.from_card_id,
                "to_card_id": relation.to_card_id,
                "target_printing_id": relation.target_printing_id,
                "relation": "official_unspecified",
                "source_kind": "official",
                "source_id": relation.page.source.id,
            },
        )
    populate_errata(db, plan.errata, texts)
    for gap in plan.gaps:
        values = gap.report()
        db.insert(
            "build_issue",
            {
                "id": key("extras-issue", values),
                "category": "card_extras:" + gap.category,
                "severity": "error"
                if gap.category in {"errata_current_pending", "source_printing_missing"}
                else "warning",
                "entity_type": "card",
                "entity_id": gap.card_id or gap.region + ":" + gap.card_no,
                "message": canonical(values).decode(),
                "source_id": gap.source_id,
            },
        )
    record = input_record(build, expected)
    record.verify(db, build, expected, complete=False)
    return record


def _questions(db: Database, plan: ExtrasPlan, texts: TextInterner) -> None:
    groups: dict[str, list[QAVersion]] = defaultdict(list)
    for version in plan.questions:
        groups[version.qa_id].append(version)
    for owner, versions in sorted(groups.items()):
        versions.sort(key=lambda version: version.revision)
        first = versions[0]
        page = first.pages[0]
        entry = first.entry
        db.insert(
            "qa",
            {
                "id": owner,
                "region": page.region,
                "official_number": entry.official_number,
                "stable_source_key": entry.official_number or entry.stable_source_key,
                "source_url": page.source.url,
            },
        )
        previous = None
        for version in versions:
            entry, page = version.entry, version.pages[0]
            language = "ja" if page.region == "jp" else "en"
            db.insert(
                "qa_version",
                {
                    "id": version.id,
                    "qa_id": owner,
                    "revision": version.revision,
                    "published_on": entry.published_on,
                    "updated_on": entry.updated_on,
                    "date_raw": entry.date_raw,
                    "observed_at": page.source.fetched_at,
                    "question_unit_id": texts.intern(
                        LocalizedText(lang=language, text=entry.question)
                    ),
                    "answer_unit_id": texts.intern(
                        LocalizedText(lang=language, text=entry.answer)
                    ),
                    "state": entry.state,
                    "source_id": page.source.id,
                    "supersedes_id": previous,
                },
            )
            for card_id in version.card_ids:
                db.insert("qa_card", {"qa_version_id": version.id, "card_id": card_id})
            previous = version.id


def require_card_extras_ready(db: Database, scope: tuple[tuple[str, str], ...]) -> None:
    """Block the specified region/card release scope on known unresolved evidence."""
    for issue in db.rows("build_issue"):
        if issue.values["category"] not in {
            "card_extras:errata_current_pending",
            "card_extras:source_printing_missing",
        }:
            continue
        message = issue.values["message"]
        context = parse(message.encode()) if isinstance(message, str) else None
        if not isinstance(context, dict):
            raise TypeError("Invalid card extras blocking issue context")
        if (context["region"], issue.values["entity_id"]) in scope:
            raise ValueError("Unresolved supplemental evidence blocks release scope")
