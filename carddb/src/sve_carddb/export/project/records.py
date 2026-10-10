"""Fixed scalar whitelists and relational projections for all public collections."""

from typing import TYPE_CHECKING

from sve_carddb.contracts.snapshot import definition
from sve_carddb.core.json import array, integer, object_value, string
from sve_carddb.export.project.source import Record, Source, json_list, pick

if TYPE_CHECKING:
    from pydantic import JsonValue


SCALARS = {
    "card": "id,layout,identity_state,home_set_id",
    "face": "id,card_id,ordinal,side",
    "printing": "id,card_id,region,card_no,card_no_state,catalog_state,listing_confidence,variant_key,rarity_code,rarity_raw,premium,serial_total,decklog_available,decklog_verification,decklog_checked_on",
    "product_family": "id,code,public_code,kind,name_unit_id",
    "product": "id,family_id,region,product_code,name_unit_id,product_type,released_on,date_precision,date_raw",
    "printing_product": "printing_id,product_id,inclusion_kind,note_unit_id",
    "identity_change": "id,kind,old_card_id,new_card_id,printing_id,data_version,reason",
    "art": "id,card_id,face_id,classification",
    "artist": "id,display_name",
    "stamp": "id,code,series_code,text_raw,kind,displayed_year",
    "text_unit": "id,lang,text",
    "face_revision": "id,face_id,region,revision,effective_from,effective_until,temporal_status,change_kind,name_unit_id,effect_unit_id,class_code,type_code,cost,attack,defense",
    "translation": "id,target_lang,origin,authority,low_confidence",
    "qa": "id,region,official_number,source_url",
    "qa_version": "id,qa_id,revision,published_on,updated_on,date_raw,question_unit_id,answer_unit_id,state",
    "cr_version": "id,region,version,published_on,effective_on,source_url",
    "cr_clause": "id,cr_version_id,number,text_unit_id",
    "errata": "id,region,official_url",
    "ruling_revision": "id,ruling_id,revision,question_unit_id,decision_unit_id,strength,decided_on,review_state",
    "rules_name": "id,region,official_name",
    "face_rules_name": "face_id,region,rules_name_id,role",
    "rules_profile": "id,region,format_code,name_unit_id",
    "restriction": "id,profile_id,announced_on,effective_from,effective_until,kind,state,max_copies,max_selected_groups",
    "card_related": "id,from_card_id,to_card_id,relation,suggested_count",
    "digital_card": "id,game,official_id",
    "digital_art": "id,style_key",
    "digital_link": "id,card_id,face_id,digital_card_id,relation,effect_similarity",
    "digital_art_link": "art_id,digital_art_id,relation",
    "digital_link_coverage": "card_id,game,state,as_of",
    "voice": "id,digital_card_id,lang,kind_code,variant,interaction_target_id,label_raw,source_url,asset_path,mime,bytes,duration_ms,availability",
    "card_voice": "card_id,voice_id,usage",
    "keyword": "id,code,kind,definition_unit_id",
    "mechanic_projection": "card_id,keyword_id,scope,relations,actions",
    "card_mechanic_coverage": "card_id,scope,complete_all,complete_mode,complete_keyword_ids,partial_mode,partial_keyword_ids",
    "vocabulary": "kind,code,label_unit_id,active",
    "search_alias": "kind,code,lang,text,normalized",
    "text_symbol": "id,code,parameter_schema,keyword_id,spellings,localizations",
    "card_route_alias": "namespace,old_key,target_namespace,target_key,reason",
    "route_override": "route_key,printing_id",
    "image_asset": "id,source_src_raw,source_url,width,height",
    "printing_image": "printing_id,face_id,image_id",
    "image_variant": "image_id,size_key,format,path,width,height,bytes",
}

PRINT_FACE = "face_id,art_id,frame_code,signed,embellishment_state,printed_name_unit_id,printed_effect_unit_id,flavor_unit_id,printed_text_state"
SECTION = "ordinal,text_unit_id,kind"


def initial(source: Source) -> dict[str, list[Record]]:
    """Select only fixed scalar columns, leaving each derived field to its owner."""
    view = {
        name: [dict(row) for row in source.rows(name, fields)]
        for name, fields in SCALARS.items()
    }
    for name in ("annotation_set", "annotation_concept", "field_annotation"):
        view[name] = []
    return view


def text_records(source: Source, view: dict[str, list[Record]]) -> None:
    """Preserve revision history, traits and ordered sections without audit fields."""
    revisions = {row["id"] for row in view["face_revision"]}
    for face in view["face"]:
        face["current"] = [
            pick(row, "region,revision_id,basis")
            for row in source.matching(
                "face_current", "face_id,region,revision_id,basis", face_id=face["id"]
            )
            if row["revision_id"] in revisions
        ]
    for revision in view["face_revision"]:
        for public, table, code in (
            ("traits", "face_trait", "trait_code"),
            ("titles", "face_title", "title_code"),
            ("special_kinds", "face_special_kind", "special_kind_code"),
        ):
            revision[public] = json_list(
                sorted(
                    {
                        string(row[code])
                        for row in source.matching(
                            table, "revision_id," + code, revision_id=revision["id"]
                        )
                    }
                )
            )
        revision["sections"] = [
            pick(row, SECTION)
            for row in source.matching(
                "face_text_section",
                "revision_id," + SECTION,
                revision_id=revision["id"],
            )
        ]
        revision["translations"] = []
        revision["corrections"] = []
    for table in (
        "product",
        "product_family",
        "qa_version",
        "cr_clause",
        "keyword",
        "vocabulary",
    ):
        for row in view[table]:
            row["translations"] = []


def printing_records(source: Source, view: dict[str, list[Record]]) -> None:
    """Join integer allocations, reviewed references and physical face metadata."""
    integers = {
        row["printing_id"]: row["int_id"]
        for row in source.rows("card_int_id", "printing_id,int_id")
    }
    internal = source.index("printing", "id,decklog_source_id")
    for printing in view["printing"]:
        printing["int_id"] = integers[printing["id"]]
        details = internal[string(printing["id"])]
        # Registry identities are manually confirmed; the registry has no other state.
        printing["review_level"] = "confirmed"
        printing["decklog_source_url"] = source.url(details["decklog_source_id"])
        printing["reference_urls"] = (
            json_list(
                sorted(
                    {
                        string(url)
                        for row in source.matching(
                            "printing_reference",
                            "printing_id,source_id",
                            printing_id=printing["id"],
                        )
                        if (url := source.url(row["source_id"])) is not None
                    }
                )
            )
            if printing["catalog_state"] == "unlisted"
            else []
        )
        faces: list[Record] = []
        for raw in source.matching(
            "printing_face", "printing_id," + PRINT_FACE, printing_id=printing["id"]
        ):
            face = pick(raw, PRINT_FACE)
            owner = {"printing_id": printing["id"], "face_id": face["face_id"]}
            face["sections"] = [
                pick(row, SECTION)
                for row in source.matching(
                    "printing_text_section", "printing_id,face_id," + SECTION, **owner
                )
            ]
            face["stamps"] = [
                pick(row, "stamp_id,position,color")
                for row in source.matching(
                    "printing_stamp",
                    "printing_id,face_id,stamp_id,position,color",
                    **owner,
                )
            ]
            face["translations"] = []
            face["corrections"] = []
            faces.append(face)
        ordinals = {row["id"]: integer(row["ordinal"]) for row in view["face"]}
        faces.sort(key=lambda row: ordinals[string(row["face_id"])])
        printing["faces"] = list(faces)


def ancillary_records(source: Source, view: dict[str, list[Record]]) -> None:
    """Project QA history, restrictions, CR and nested official errata evidence."""
    for qa in view["qa"]:
        versions = [row for row in view["qa_version"] if row["qa_id"] == qa["id"]]
        qa["current_version_id"] = (
            max(versions, key=lambda row: integer(row["revision"]))["id"]
            if versions
            else None
        )
    for version in view["qa_version"]:
        version["cards"] = json_list(
            sorted(
                {
                    string(row["card_id"])
                    for row in source.matching(
                        "qa_card", "qa_version_id,card_id", qa_version_id=version["id"]
                    )
                }
            )
        )
    for profile in view["rules_profile"]:
        profile["revisions"] = [
            pick(
                row,
                "id,effective_from,effective_until,cr_version_id,default_copy_limit,construction_rules_ref",
            )
            for row in source.matching(
                "rules_profile_revision",
                "profile_id,id,effective_from,effective_until,cr_version_id,default_copy_limit,construction_rules_ref",
                profile_id=profile["id"],
            )
        ]
    for restriction in view["restriction"]:
        restriction["members"] = [
            pick(row, "rules_name_id,choice_option,deck_scope")
            for row in source.matching(
                "restriction_member",
                "restriction_id,rules_name_id,choice_option,deck_scope",
                restriction_id=restriction["id"],
            )
        ]
    for errata in view["errata"]:
        versions_out: list[JsonValue] = []
        fields = "id,revision,announced_on,effective_on,date_raw,reason_unit_id,exchange_offered"
        for raw in source.matching(
            "errata_version", "errata_id," + fields, errata_id=errata["id"]
        ):
            version = pick(raw, fields)
            version["changes"] = [
                {
                    "face_id": row["face_id"],
                    "field": row["field"],
                    "before": row["before_value"],
                    "after": row["after_value"],
                }
                for row in source.matching(
                    "errata_change",
                    "errata_version_id,face_id,field,before_value,after_value",
                    errata_version_id=version["id"],
                )
            ]
            version["printings"] = [
                pick(row, "printing_id,scope")
                for row in source.matching(
                    "errata_printing",
                    "errata_version_id,printing_id,scope",
                    errata_version_id=version["id"],
                )
            ]
            versions_out.append(version)
        errata["versions"] = versions_out


def art_records(source: Source, view: dict[str, list[Record]]) -> None:
    """Derive art regions solely from current physical uses in selected regions."""
    prints = {row["id"]: row for row in view["printing"]}
    uses = source.rows("printing_face", "printing_id,art_id")
    view["art"] = [
        row
        for row in view["art"]
        if any(
            use["art_id"] == row["id"] and use["printing_id"] in prints for use in uses
        )
    ]
    for art in view["art"]:
        art["review_level"] = "confirmed"
        art["regions"] = json_list(
            sorted(
                {
                    string(prints[use["printing_id"]]["region"])
                    for use in uses
                    if use["art_id"] == art["id"] and use["printing_id"] in prints
                }
            )
        )
        art["artists"] = [
            pick(row, "artist_id,role")
            for row in source.matching(
                "art_artist", "art_id,artist_id,role", art_id=art["id"]
            )
        ]
    artists = {
        object_value(row)["artist_id"]
        for art in view["art"]
        for row in array(art["artists"])
    }
    view["artist"] = [row for row in view["artist"] if row["id"] in artists]
    for table in ("stamp", "digital_link", "digital_art_link"):
        fields = SCALARS[table] + ",decision_id"
        keys = [string(key) for key in array(definition(table)["x-primary-key"])]
        decisions_by_key = {
            tuple(raw[key] for key in keys): raw for raw in source.rows(table, fields)
        }
        # Publication filtering can remove rows; position is not an identity join.
        for row in view[table]:
            raw = decisions_by_key[tuple(row[key] for key in keys)]
            row["review_level"] = source.review(raw["decision_id"])


def digital_records(source: Source, view: dict[str, list[Record]]) -> None:
    """Join digital phase instead of exposing the build-only digital face ID."""
    faces = source.index("digital_face", "id,digital_card_id,phase")
    for table in ("digital_art", "digital_link", "voice"):
        lookup = source.index(table, "id,digital_face_id")
        for row in view[table]:
            face_id = lookup[string(row["id"])]["digital_face_id"]
            face = None if face_id is None else faces[string(face_id)]
            row["phase" if table == "digital_art" else "digital_phase"] = (
                None if face is None else face["phase"]
            )
            if table == "digital_art":
                if face is None:
                    raise ValueError("Digital art requires its source face")
                row["digital_card_id"] = face["digital_card_id"]


def ruling_records(source: Source, view: dict[str, list[Record]]) -> None:
    """Expose public evidence and replacement scopes while keeping reviewers private."""
    for ruling in view["ruling_revision"]:
        ruling["cards"] = json_list(
            sorted(
                {
                    string(row["card_id"])
                    for row in source.matching(
                        "ruling_card",
                        "ruling_revision_id,card_id",
                        ruling_revision_id=ruling["id"],
                    )
                }
            )
        )
        scopes = source.matching(
            "ruling_supersession",
            "old_revision_id,new_revision_id,scope_unit_id",
            old_revision_id=ruling["id"],
        )
        ruling["replaced_scopes"] = [
            {
                "scope_unit_id": row["scope_unit_id"],
                "replacement_revision_id": row["new_revision_id"],
            }
            for row in scopes
        ]
        ruling["active_scopes"] = []
        evidence = source.matching(
            "ruling_evidence",
            "ruling_revision_id,qa_version_id,cr_clause_id,source_id,role,quote,locator",
            ruling_revision_id=ruling["id"],
        )
        ruling["evidence"] = [
            pick(row, "qa_version_id,cr_clause_id,role,quote,locator")
            | {"source_url": source.url(row["source_id"])}
            for row in evidence
        ]
        ruling["hints"] = [
            pick(row, "lang,text_unit_id,parameter_schema")
            for row in source.matching(
                "ruling_hint",
                "ruling_revision_id,lang,text_unit_id,parameter_schema",
                ruling_revision_id=ruling["id"],
            )
        ]
