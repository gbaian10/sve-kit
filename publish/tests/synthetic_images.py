"""Synthetic pixels and rows; no authored or private official test data."""

import hashlib
from copy import deepcopy
from dataclasses import dataclass, replace
from io import BytesIO
from typing import TYPE_CHECKING

import pytest
from PIL import Image, ImageCms, ImageDraw
from sve_carddb.export.media import MediaPlan, prepare_media
from sve_carddb.export.project import Projection
from sve_carddb.export.read_api import array, object_value, parse, string
from sve_carddb.export.transport import Batch, Ownership, export_snapshot
from sve_carddb.images.variants import ImageSource, build_variants

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.export.project.source import Record
    from sve_carddb.export.transport import Snapshot

RAW_SRC = "../cards/card image.png?edition=jp"
BATCH = Batch("preview-20261004T010203Z-0001", "2026-10-04T01:02:03Z", ("jp",))

PROJECTION = b'{"tables":{"card":[{"id":"card","layout":"single","identity_state":"confirmed","home_set_id":"family","faces":["face"],"regions":[{"region":"en","release_state":"announced","mapping_state":"unmapped","as_of":"2026-09-29","mapping_as_of":null,"mapping_scope":null,"default_printing_id":null,"default_method":null,"deck_role":null,"debut_product_ids":[],"debut_state":"unknown"},{"region":"jp","release_state":"released","mapping_state":"confirmed_none","as_of":"2026-10-01","mapping_as_of":"2026-09-29","mapping_scope":"Synthetic scope","default_printing_id":"printing","default_method":"override","deck_role":"extra","debut_product_ids":[],"debut_state":"unknown"}]},{"id":"old_card","layout":"single","identity_state":"retired","home_set_id":"family","faces":[],"regions":[{"region":"en","release_state":"unknown","mapping_state":"unmapped","as_of":"2026-10-01","mapping_as_of":null,"mapping_scope":null,"default_printing_id":null,"default_method":null,"deck_role":null,"debut_product_ids":[],"debut_state":"unknown"},{"region":"jp","release_state":"unknown","mapping_state":"unmapped","as_of":"2026-10-01","mapping_as_of":null,"mapping_scope":null,"default_printing_id":null,"default_method":null,"deck_role":null,"debut_product_ids":[],"debut_state":"unknown"}]}],"face":[{"id":"face","card_id":"card","ordinal":0,"side":"front","current":[{"region":"jp","revision_id":"revision","basis":"latest_observed_no_errata"}],"wording":[]}],"printing":[{"id":"printing","card_id":"card","region":"jp","card_no":"TEST-001\xe2\x93\x88a","card_no_state":"official","catalog_state":"official","listing_confidence":null,"variant_key":"standard","rarity_code":null,"rarity_raw":"","premium":null,"serial_total":null,"decklog_available":true,"decklog_verification":"unverified","decklog_checked_on":null,"int_id":20001,"review_level":"confirmed","decklog_source_url":null,"reference_urls":[],"faces":[{"face_id":"face","art_id":"art","frame_code":null,"signed":null,"embellishment_state":"unreviewed","printed_name_unit_id":null,"printed_effect_unit_id":null,"flavor_unit_id":null,"printed_text_state":"unknown","sections":[{"ordinal":0,"text_unit_id":"t:ja:6a34655cc94a4524","kind":"unknown"}],"stamps":[{"stamp_id":"stamp","position":null,"color":null}],"translations":[],"corrections":[{"field":"effect","corrected_from":"Before","is_corrected":true,"reason":"Synthetic correction","source_url":"https://example.invalid/card"}],"observations":[{"revision_id":"revision","state":"available","source_url":"https://example.invalid/card"}],"name_concept_id":null}]}],"product_family":[{"id":"family","code":"test","public_code":"TEST","kind":"other","name_unit_id":"t:ja:6a34655cc94a4524","translations":[]}],"product":[{"id":"product","family_id":null,"region":"jp","product_code":null,"name_unit_id":"t:ja:6a34655cc94a4524","product_type":"other","released_on":null,"date_precision":"unknown","date_raw":null,"translations":[]}],"printing_product":[{"printing_id":"printing","product_id":"product","inclusion_kind":"pack","note_unit_id":null,"available_on":null,"date_precision":null,"date_raw":null,"first_inclusion_state":"unknown"}],"identity_change":[{"id":"change","kind":"merge","old_card_id":"old_card","new_card_id":"card","printing_id":null,"data_version":"preview-20260929T000000Z-0001","reason":"Synthetic merge"}],"art":[{"id":"art","card_id":"card","face_id":"face","classification":"unclassified","review_level":"confirmed","regions":["jp"],"artists":[{"artist_id":"artist","role":"illustration"}]}],"artist":[{"id":"artist","display_name":"Synthetic artist"}],"stamp":[{"id":"stamp","code":"synthetic","series_code":null,"text_raw":"Synthetic stamp","kind":"event","displayed_year":null,"review_level":"confirmed"}],"text_unit":[{"id":"t:ja:6a34655cc94a4524","lang":"ja","text":"Synthetic text"},{"id":"t:ja:b203904f7511cdf8","lang":"ja","text":"Synthetic keyword"},{"id":"t:zh-Hant:dfec68408c771a79","lang":"zh-Hant","text":"Synthetic translation"}],"face_revision":[{"id":"revision","face_id":"face","region":"jp","revision":0,"effective_from":null,"effective_until":null,"temporal_status":"unknown","change_kind":"initial","name_unit_id":"t:ja:6a34655cc94a4524","effect_unit_id":"t:ja:6a34655cc94a4524","class_code":null,"type_code":"follower","cost":null,"attack":null,"defense":null,"traits":["synthetic"],"titles":["synthetic"],"special_kinds":["synthetic"],"sections":[{"ordinal":0,"text_unit_id":"t:ja:6a34655cc94a4524","kind":"rule"}],"translations":[{"basis":"own_source","counterpart":null,"field":"name","ordinal":null,"source":{"field":"name","ordinal":null,"owner":{"id":"revision","kind":"face_revision"}},"target_lang":"zh-Hant","translation_id":"translation"}],"corrections":[{"field":"effect","corrected_from":"Before","is_corrected":true,"reason":"Synthetic correction","source_url":"https://example.invalid/card"}],"name_concept_id":null}],"translation":[{"annotation_set_id":"ann:0cef6ab6f6432acc17b2f5f594544615f3c722f8d1e7ec7ec5320f6efd607b8d","authority":"unofficial","id":"translation","low_confidence":false,"origin":"project","source_unit_id":"t:ja:6a34655cc94a4524","target_lang":"zh-Hant","text_unit_id":"t:zh-Hant:dfec68408c771a79","annotation_kind":"explicit"}],"qa":[{"id":"qa","region":"jp","official_number":"Q1","source_url":"https://example.invalid/qa","current_version_id":"qa_v"}],"qa_version":[{"id":"qa_v","qa_id":"qa","revision":0,"published_on":"2026-09-29","updated_on":null,"date_raw":null,"question_unit_id":"t:ja:6a34655cc94a4524","answer_unit_id":"t:ja:6a34655cc94a4524","state":"active","translations":[],"cards":["card"]}],"cr_version":[{"id":"cr","region":"jp","version":"synthetic-1","published_on":"2026-09-29","effective_on":null,"source_url":"https://example.invalid/rules.pdf"}],"cr_clause":[{"cr_version_id":"cr","id":"clause","number":"1.10.2","text_unit_id":"t:ja:6a34655cc94a4524","translations":[]}],"errata":[{"id":"errata","region":"jp","official_url":"https://example.invalid/errata","versions":[{"id":"errata_v","revision":0,"announced_on":null,"effective_on":null,"date_raw":null,"reason_unit_id":null,"exchange_offered":null,"changes":[{"face_id":"face","field":"effect","before":"Before","after":"Synthetic text"}],"printings":[{"printing_id":"printing","scope":"confirmed_applies"}]}]}],"ruling_revision":[{"id":"ruling-rev","ruling_id":"ruling","revision":1,"question_unit_id":"t:ja:6a34655cc94a4524","decision_unit_id":"t:ja:6a34655cc94a4524","strength":"inferred","decided_on":"2026-09-29","review_state":"current","cards":["card"],"replaced_scopes":[{"scope_unit_id":"t:ja:6a34655cc94a4524","replacement_revision_id":"ruling-rev2"}],"active_scopes":["t:ja:6a34655cc94a4524"],"evidence":[{"qa_version_id":"qa_v","cr_clause_id":null,"role":"supporting","quote":"Synthetic text","locator":"answer","source_url":null}],"hints":[{"lang":"ja","text_unit_id":"t:ja:6a34655cc94a4524","parameter_schema":{"parameters":[]}}]},{"id":"ruling-rev2","ruling_id":"ruling","revision":2,"question_unit_id":"t:ja:6a34655cc94a4524","decision_unit_id":"t:ja:6a34655cc94a4524","strength":"inferred","decided_on":"2026-09-29","review_state":"needs_review","cards":[],"replaced_scopes":[],"active_scopes":["t:ja:6a34655cc94a4524"],"evidence":[],"hints":[]}],"rules_name":[{"id":"rules_name","region":"jp","official_name":"Synthetic name"}],"face_rules_name":[{"face_id":"face","region":"jp","rules_name_id":"rules_name","role":"primary"}],"rules_profile":[{"id":"profile","region":"jp","format_code":"standard","name_unit_id":"t:ja:6a34655cc94a4524","revisions":[{"id":"profile_revision","effective_from":"2026-09-29","effective_until":null,"cr_version_id":null,"default_copy_limit":null,"construction_rules_ref":null}]}],"restriction":[{"id":"restriction","profile_id":"profile","announced_on":null,"effective_from":"2026-09-29","effective_until":null,"kind":"copy_limit","state":"confirmed","max_copies":0,"max_selected_groups":null,"members":[{"rules_name_id":"rules_name","choice_option":0,"deck_scope":"all"}]}],"card_related":[{"id":"related","from_card_id":"old_card","to_card_id":"card","relation":"same_rules_reskin","suggested_count":null,"applicable_regions":["jp"]}],"digital_card":[{"id":"digital","game":"sv1","official_id":"100000001"}],"digital_art":[{"id":"digital-art","style_key":"standard","phase":"normal","digital_card_id":"digital"}],"digital_link":[{"id":"digital-link","card_id":"card","face_id":"face","digital_card_id":"digital","relation":"same_card","effect_similarity":null,"review_level":"confirmed","digital_phase":"normal"}],"digital_art_link":[{"art_id":"art","digital_art_id":"digital-art","relation":"same_art","review_level":"confirmed"}],"digital_link_coverage":[{"card_id":"card","game":"sv1","state":"reviewed_matches","as_of":"2026-09-29"}],"voice":[{"id":"voice","digital_card_id":"digital","lang":"ja","kind_code":"play","variant":null,"interaction_target_id":null,"label_raw":null,"source_url":"https://example.invalid/audio","asset_path":null,"mime":null,"bytes":null,"duration_ms":null,"availability":"remote_only","digital_phase":"normal"}],"card_voice":[{"card_id":"card","voice_id":"voice","usage":"browse"}],"keyword":[{"id":"keyword","code":"synthetic","kind":"mechanic","definition_unit_id":"t:ja:6a34655cc94a4524","translations":[],"name_unit_id":"t:ja:b203904f7511cdf8","actions":[{"action":"produce","label_unit_id":"t:ja:6a34655cc94a4524"}]}],"mechanic_projection":[{"card_id":"card","keyword_id":"keyword","scope":"shared","relations":["grants"],"actions":[]}],"card_mechanic_coverage":[{"card_id":"card","scope":"shared","complete_all":false,"complete_mode":"include","complete_keyword_ids":[],"partial_mode":"exclude","partial_keyword_ids":[]}],"vocabulary":[{"kind":"class","code":"synthetic","label_unit_id":"t:ja:6a34655cc94a4524","active":true,"translations":[]},{"kind":"frame","code":"synthetic","label_unit_id":"t:ja:6a34655cc94a4524","active":true,"translations":[]},{"kind":"rarity","code":"synthetic","label_unit_id":"t:ja:6a34655cc94a4524","active":true,"translations":[]},{"kind":"special_kind","code":"synthetic","label_unit_id":"t:ja:6a34655cc94a4524","active":true,"translations":[]},{"kind":"title","code":"synthetic","label_unit_id":"t:ja:6a34655cc94a4524","active":true,"translations":[]},{"kind":"trait","code":"synthetic","label_unit_id":"t:ja:6a34655cc94a4524","active":true,"translations":[]},{"kind":"type","code":"follower","label_unit_id":"t:ja:6a34655cc94a4524","active":true,"translations":[]}],"search_alias":[{"kind":"card","code":"card","lang":"ja","text":"Synthetic alias","normalized":"synthetic alias"}],"text_symbol":[{"id":"symbol","code":"synthetic","parameter_schema":{"parameters":[]},"keyword_id":null,"spellings":[{"lang":"ja","literal_prefix":"{Q}","literal_suffix":"","parameter_name":null,"parse_kind":"literal"}],"localizations":[{"copy_pattern":"{Q}","lang":"ja","name":"Synthetic symbol","tooltip":""}]}],"card_route_alias":[{"namespace":"provisional","old_key":"20001","target_namespace":"official","target_key":"TEST-001%E2%93%88a","reason":"provisional_corrected"}],"route_override":[{"route_key":"TEST-001%E2%93%88a","printing_id":"printing"}],"image_asset":[{"id":"image","source_src_raw":"../source.png","source_url":"https://example.invalid/source.png","width":459,"height":641,"format":"png"}],"printing_image":[{"printing_id":"printing","face_id":"face","image_id":"image","publication_state":"approved","availability":"available"}],"image_variant":[{"image_id":"image","size_key":"art_m","format":"webp","path":"images/sha256/07/0706097a45a595d2cfaa717a5af81d06a5d49d2e299b70c24fc94ff238cf6c21.webp","width":132,"height":99,"bytes":1046},{"image_id":"image","size_key":"art_s","format":"webp","path":"images/sha256/07/0706097a45a595d2cfaa717a5af81d06a5d49d2e299b70c24fc94ff238cf6c21.webp","width":132,"height":99,"bytes":1046},{"image_id":"image","size_key":"card_l","format":"webp","path":"images/sha256/2a/2a07a048bc817c618bf14b77f9ee00425134c720c67ecbdaf33d65272308a584.webp","width":160,"height":224,"bytes":1938},{"image_id":"image","size_key":"card_m","format":"webp","path":"images/sha256/2a/2a07a048bc817c618bf14b77f9ee00425134c720c67ecbdaf33d65272308a584.webp","width":160,"height":224,"bytes":1938},{"image_id":"image","size_key":"card_s","format":"webp","path":"images/sha256/bd/bd5a452545792130d7af4a3cc027ce7079f7d2ce9491602a6a4ac3f67ea126f1.webp","width":128,"height":179,"bytes":1498}],"card_engine_support":[{"card_id":"card","shared":{"status":"missing_dsl","dsl_status":null,"dsl_version":null,"dsl_id":null,"validation_state":"not_applicable","reasons":["missing_dsl"],"reason_detail":null,"program_ref":null,"ruling_revision_ids":[]},"overrides":[],"region_blocks":[{"region":"en","reasons":["mapping_unconfirmed","missing_region_source","region_text_unreviewed"]}]},{"card_id":"old_card","shared":{"status":"missing_dsl","dsl_status":null,"dsl_version":null,"dsl_id":null,"validation_state":"not_applicable","reasons":["missing_dsl"],"reason_detail":null,"program_ref":null,"ruling_revision_ids":[]},"overrides":[],"region_blocks":[{"region":"en","reasons":["mapping_unconfirmed","missing_region_source","region_text_unreviewed","retired"]},{"region":"jp","reasons":["missing_region_source","retired"]}]}],"annotation_set":[{"id":"ann:0cef6ab6f6432acc17b2f5f594544615f3c722f8d1e7ec7ec5320f6efd607b8d","occurrences":[{"bold":true,"ordinal":0,"ranges":[{"end":9,"start":0}],"reference":{"key":"term","kind":"glossary"}}],"text_unit_id":"t:zh-Hant:dfec68408c771a79"},{"id":"ann:f19953b58771c284896a9eb3ee6c14c72684af18780ae8393546232457d4d72b","occurrences":[{"bold":true,"ordinal":0,"ranges":[{"end":9,"start":0}],"reference":{"key":"term","kind":"glossary"}}],"text_unit_id":"t:ja:6a34655cc94a4524"}],"annotation_concept":[{"card_ids":[],"category":"keyword","explanations":[{"id":"keyword","kind":"keyword"}],"id":"term"}],"field_annotation":[{"annotation_set_id":"ann:f19953b58771c284896a9eb3ee6c14c72684af18780ae8393546232457d4d72b","field":"name","ordinal":null,"owner":{"id":"revision","kind":"face_revision"}}]},"config":{"format_version":"3.0.0","languages":[{"code":"ja","fallback_order":[],"display_name":"Synthetic language"},{"code":"zh-Hant","fallback_order":["ja"],"display_name":"Synthetic language"}],"digital_endpoints":[{"game":"sv1","card_url_template":"https://example.invalid/card/{official_id}?lang={provider_lang}","language_map":{"ja":"ja","zh-Hant":"zh-tw"},"status":"unknown","refresh_policy":"frozen"}],"shop_links":[{"id":"shop","url_template":"https://example.invalid/shop/{card_no}?region={region}","parameters":["card_no","region"],"feature_key":"synthetic","enabled_dev":true,"enabled_prod":false}],"image_sizes":[{"key":"art_m","max_height":288,"max_width":384,"purpose":"art"},{"key":"art_s","max_height":120,"max_width":160,"purpose":"art"},{"key":"card_l","max_height":641,"max_width":459,"purpose":"card"},{"key":"card_m","max_height":447,"max_width":320,"purpose":"card"},{"key":"card_s","max_height":179,"max_width":128,"purpose":"card"}],"search":{"grammar_version":"synthetic-v1","normalizer_version":"synthetic-v1"},"catalog_feedback_url":"https://example.invalid/feedback","third_party_image_policy":"mirror_reviewed","deck_eligibility_policy":"regional_decklog"},"metadata":{"qa_card_ids":["card"],"errata_card_ids":["card"],"source_windows":[{"kind":"cardlist","region":"jp","scope_key":"region:*","from_date":"2026-09-29","until_date":null,"as_of":"2026-09-29","state":"partial","source_url":"https://example.invalid/card"}],"restriction_coverage":[{"profile_id":"profile","from_date":"2026-09-29","until_date":null,"state":"partial","source_url":"https://example.invalid/card"}],"mechanic_universe_id":"sha256:9fe6af2d65eb176a5d58cb5268487186599a2bf6e5024f29e13bccee83bd2ea3","coverage":{"reviews":[],"translations":[],"mechanics":[{"region":"jp","scope":"shared","total_cards":1,"any_annotated_cards":1,"fully_annotated_cards":0,"unknown_cards":0,"eligibility":"all_non_retired_cards_in_region","by_keyword":[{"keyword_id":"keyword","complete_cards":0}]}]},"engine_support_target":{"engine_version":null,"engine_build_hash":null,"validation_policy_id":null}}}'
OWNERSHIP = b'{"printing": "family"}'


def png(
    width: int,
    height: int,
    *,
    alpha: bool = False,
    orientation: int | None = None,
    icc: bool = False,
) -> bytes:
    mode = "RGBA" if alpha else "RGB"
    image = Image.new(mode, (width, height), (24, 75, 131, 0 if alpha else 255))
    draw = ImageDraw.Draw(image)
    draw.rectangle(
        (width // 5, height // 5, 4 * width // 5, 4 * height // 5), fill="red"
    )
    draw.line((0, height - 1, width - 1, 0), fill="white", width=2)
    options: dict[str, bytes] = {}
    if orientation is not None:
        exif = image.getexif()
        exif[274] = orientation
        options["exif"] = exif.tobytes()
    if icc:
        options["icc_profile"] = ImageCms.ImageCmsProfile(
            ImageCms.createProfile("sRGB")
        ).tobytes()
    buffer = BytesIO()
    image.save(buffer, format="PNG", **options)
    return buffer.getvalue()


def source(data: bytes, *, image_id: str = "img:jp:1") -> ImageSource:
    return ImageSource(
        image_id=image_id,
        source_bytes=data,
        source_sha256=hashlib.sha256(data).hexdigest(),
        source_src_raw=RAW_SRC,
        asset_kind="sve_card",
        publication_state="approved",
        availability="available",
    )


@dataclass(frozen=True)
class PublicImages:
    projection: Projection
    ownership: Ownership
    library: Path

    def plan(self, tables: dict[str, list[Record]] | None = None) -> MediaPlan:
        projection = (
            self.projection
            if tables is None
            else replace(self.projection, tables=tables)
        )
        return prepare_media(projection, self.library, revision=1)

    def snapshot(self, tables: dict[str, list[Record]] | None = None) -> Snapshot:
        return export_snapshot(self.plan(tables).projection, self.ownership, BATCH)

    def tables(self) -> dict[str, list[Record]]:
        return deepcopy(self.projection.tables)


@pytest.fixture(scope="module")
def images(tmp_path_factory: pytest.TempPathFactory) -> PublicImages:
    root = tmp_path_factory.mktemp("public-image-library")
    result = build_variants(
        source(png(160, 224)), blob_root=root / "library", cache_root=root / "cache"
    )
    data = object_value(parse(PROJECTION))
    tables = {
        name: [object_value(row) for row in array(rows)]
        for name, rows in object_value(data["tables"]).items()
    }
    tables["image_variant"] = [
        {
            "image_id": "image",
            "size_key": v.size_key,
            "format": v.format,
            "path": v.path,
            "width": v.width,
            "height": v.height,
            "bytes": v.bytes,
        }
        for v in sorted(result.variants, key=lambda item: item.size_key)
    ]
    projection = Projection(
        tables, object_value(data["config"]), object_value(data["metadata"])
    )
    return PublicImages(
        projection,
        Ownership(
            {
                name: string(value)
                for name, value in object_value(parse(OWNERSHIP)).items()
            }
        ),
        root / "library",
    )
