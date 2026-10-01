"""Import announcement fragments without manufacturing full revisions or applicability."""

from collections import defaultdict
from typing import TYPE_CHECKING

from sve_carddb.build_db import Json
from sve_carddb.card_extras.models import key
from sve_carddb.card_extras.plan import printing_index
from sve_carddb.products.models import LocalizedText

if TYPE_CHECKING:
    from collections.abc import Iterator

    from sve_carddb.build_db import Database
    from sve_carddb.card_extras.models import ErrataPage
    from sve_carddb.text_observations.intern import TextInterner


def populate_errata(
    db: Database, notices: tuple[ErrataPage, ...], texts: TextInterner
) -> None:
    """Accept explicit same-region identities only; missing evidence fails the build."""
    printings = printing_index(db)
    faces = {row.values["id"]: row.values["card_id"] for row in db.rows("face")}
    printing_faces = {
        (row.values["printing_id"], row.values["face_id"])
        for row in db.rows("printing_face")
    }
    groups: dict[tuple[str, str], list[ErrataPage]] = defaultdict(list)
    for notice in notices:
        groups[notice.region, notice.official_url].append(notice)
    for (region, url), versions in sorted(groups.items()):
        owner = key("errata", [region, url])
        db.insert("errata", {"id": owner, "region": region, "official_url": url})
        previous = None
        for revision, fingerprint, notice in _versions(owner, versions):
            db.insert(
                "errata_version",
                {
                    "id": fingerprint,
                    "errata_id": owner,
                    "revision": revision,
                    "announced_on": notice.announced_on,
                    "effective_on": notice.effective_on,
                    "date_raw": notice.date_raw,
                    "reason_unit_id": None
                    if notice.reason is None
                    else texts.intern(
                        LocalizedText(
                            lang="ja" if region == "jp" else "en", text=notice.reason
                        )
                    ),
                    "exchange_offered": notice.exchange_offered,
                    "source_id": notice.source.id,
                    "supersedes_id": previous,
                },
            )
            for change in notice.changes:
                printing = printings.get((region, change.card_no))
                if (
                    printing is None
                    or faces.get(change.face_id) != printing["card_id"]
                    or (printing["id"], change.face_id) not in printing_faces
                ):
                    raise ValueError(
                        "Errata change needs an explicit adopted same-region face/printing"
                    )
                db.insert(
                    "errata_change",
                    {
                        "id": key(
                            "errata-change",
                            [fingerprint, change.model_dump(mode="json")],
                        ),
                        "errata_version_id": fingerprint,
                        "face_id": change.face_id,
                        "before_revision_id": None,
                        "after_revision_id": None,
                        "before_value": Json(change.before_value),
                        "after_value": Json(change.after_value),
                        "field": change.field,
                    },
                )
            for inclusion in notice.printings:
                printing = printings.get((region, inclusion.card_no))
                if printing is None:
                    raise ValueError(
                        "Errata listing needs an adopted same-region printing"
                    )
                if inclusion.scope == "confirmed_applies":
                    _confirmed(db, inclusion.decision_id, notice.source.id)
                db.insert(
                    "errata_printing",
                    {
                        "errata_version_id": fingerprint,
                        "printing_id": printing["id"],
                        "scope": inclusion.scope,
                        "decision_id": inclusion.decision_id,
                    },
                )
            previous = fingerprint


def _confirmed(db: Database, decision_id: str | None, source_id: str) -> None:
    decisions = {row.values["id"]: row.values["state"] for row in db.rows("decision")}
    evidence = {
        (row.values["decision_id"], row.values["source_id"])
        for row in db.rows("decision_source")
    }
    if (
        decisions.get(decision_id) != "confirmed"
        or (decision_id, source_id) not in evidence
    ):
        raise ValueError(
            "Confirmed errata applicability needs confirmed pinned evidence"
        )


def _versions(
    owner: str, notices: list[ErrataPage]
) -> Iterator[tuple[int, str, ErrataPage]]:
    seen: set[str] = set()
    previous_content = None
    revision = 0
    for notice in notices:
        content = key(
            "erratav", [owner, notice.model_dump(mode="json", exclude={"source"})]
        )
        if content == previous_content:
            continue
        identifier = (
            key("erratav", [content, notice.source.id]) if content in seen else content
        )
        seen.add(content)
        previous_content = content
        yield revision, identifier, notice
        revision += 1
