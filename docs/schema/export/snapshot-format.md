# 卡表快照格式 v2

引用與授權：範例中沿用的官方卡名、商品名、詞彙及卡文片段不在本專案授權內；
專案欄位、合成值、中文說明與資料規則依文件授權。來源及適用範圍見[文件引用說明](../../quotations.md)。

卡表快照是由[建置資料庫](../build/build-db.md)投影出的精簡契約。表名相同不代表欄位相同；本文件是出貨欄位白名單。精確 JSON 形狀、欄序、版本及分片規則見 [傳輸契約](snapshot-transport.md)。沒有指定的建置資料庫欄位不出貨，尤其 decision、`source_record`、逐列 hash、翻譯依賴、載入/考題報告與巨集。保留玩家可見的來源 URL、Q&A/CR 引文、印刷歷史、更正原值，不提供建置稽核包。

本文件的 2.0.0 圖片與有限保留契約依 [ADR-0015](../../adr/0015-image-url-version.md)／[ADR-0016](../../adr/0016-snapshot-retention.md)。1.x 已退役，不保留相容讀寫。

四層翻譯的來源政策已改依[翻譯契約 §1／§7.2](../domains/translation-contract.md#1-來源與顯示原則)：
有效且已確認同卡同面的 JP 可供繁中，divergence 不構成此顯示門檻。
本文既有 basis 列及 tuple 仍描述切換前的 wire；新版 basis／annotation／版本由
[#496](https://github.com/gbaian10/sve-kit/issues/496) 定義後同步 producer／reader，不能用舊值冒充新政策。

## 1. 快照清單（manifest）、版本與容器

`format_version` 是傳輸格式 SemVer；`data_version` 是資料批次識別，正式版表示 UTC 發布批次，`preview-` 前綴保留給不發布的預覽批次、不得用於正式版；`dsl_version` 是 DSL 主版.次版，不能混用。快照清單一份釘住 `format_version,data_version,published_at,regions,languages,min_reader_version,required_capabilities,engine_support_target,files,config_ref,text_all,partitioning,coverage,mechanic_universe_id,restriction_coverage,source_windows,qa_card_ids,errata_card_ids,changes_ref`；完整型別及命名規則見 [傳輸契約 §1–2](snapshot-transport.md#2-快照清單與檔案描述)。

`engine_support_target` 恰含 `engine_version,engine_build_hash,validation_policy_id`，三欄全 null 或全有值；逐卡狀態不重複它。files 的完整形狀、hash／壓縮大小及依賴規則見 [傳輸契約 §2](snapshot-transport.md#2-快照清單與檔案描述)。

傳輸容器採 `{format_version,types,tables:{table_name:[Fragment]}}`；Fragment 完整包含 `owner,bucket,partition,base,columns,rows`，精確身分與欄序見 [傳輸契約 §4](snapshot-transport.md#4-fragment-容器與-join)。這是具名 schema 的 row tuple 編碼，不是欄式分析資料庫。邏輯欄位仍以下表為權威。§2 是 join 後邏輯白名單；實際 columns 必須符合 §3.1 固定的啟動包／詳情分片欄位分割，不可任意省略 required/null 欄。manifest 宣告 `required_capabilities` 至少含 `column-partition-v1` 與 `fragment-container-v1`；舊 reader 不支援時拒絕載入此傳輸格式。

巢狀的 RegionView/PrintingFace/Section/FieldTranslation/Correction/Support 等記錄同樣用 tuple，types 以具名型別→columns 順序及引用型別描述（固定於 format）供載入器驗列長度；producer 以本文件的具名型別作型別名。`parameter_schema/corrected_from` 等值保持受限 JSON，不轉成位置陣列，值域依 [傳輸契約 §3.2–3.3](snapshot-transport.md#32-公開參數宣告)。純 ID/code 陣列亦保持原樣。每個分片只附用到的 types，producer 驗其與 format 定義一致；consumer 不執行資料提供的轉換程式。

reader 編譯具型別 accessor，詳情分片保留 tuples＋ID→row 索引；啟動包轉 typed 索引後釋放原 tuples，只在畫面當前項目建立 view，不能全量展開成物件再多存一份。這項編碼主要節省未壓縮傳輸/快取大小；原型量測顯示 heap 並未因此降低，記憶體要靠 §3 的逐片解析/淘汰。每表按穩定主鍵排序、集合陣列按 ID/code 排序、有序段落保留 ordinal。payload 不含 `data_version/published_at`，未變內容跨版 bytes/hash 完全相同。完整文字包是同一分片 payload 的容器聯集，不能另做另一套 carddb。公共永久 ID 保持不透明字串；text ID 固定為 `t:{lang}:{sha256(exact UTF-8 text)[:16]}`，同一份快照內同鍵不同內容即停止匯出，不能重配舊鍵或自動加長，快照不附完整 hash。不另保存跨版本的已發布文字鍵索引（[ADR-0020](../../adr/0020-upload-from-export.md)）。

下面列出的欄位全部存在，`?` 表示可 null，不表示任意省略。內嵌同型陣列可以空；未知與空的規則在 [build-db.md](../build/build-db.md) 定義。顯示 label 參照 `text_unit`，介面通用提示留 app i18n。枚舉值與型別沿建置資料庫同名定義，投影新增型別於下節明列；不可帶出建置資料庫未列欄位。

職業／卡種正式採納不改欄位或格式版本，但公開資料內容會變：class_code／type_code 由暫碼變正式代碼、special_kinds 開始有值、公開 vocabulary 新增 special_kind 類別；資料版本須更新。producer／reader 的引用閉包驗證須涵蓋 vocabulary(kind=special_kind)，不能把新標記當成 type 或忽略未定義引用。

## 2. 公開表、完整欄位與玩家用途

| 集合                     | 公開欄位                                                                                                                                                                                                                                                                                                                   | 鍵與玩家用途（未註明 PK 者以首欄 `id` 為 PK）                                                                                                                                                                                                                                                           |
| ------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `card`                   | `id, layout, identity_state, home_set_id, faces:[face_id], regions:[RegionView]`                                                                                                                                                                                                                                           | 每卡一格、各區預設入口、發行/對照狀態                                                                                                                                                                                                                                                                   |
| `face`                   | `id, card_id, ordinal, side, current:[{region,revision_id,basis}], wording:[WordingView]`                                                                                                                                                                                                                                  | 雙面切換，兩面算同一實物                                                                                                                                                                                                                                                                                |
| `printing`               | `id, card_id, region, card_no, card_no_state, catalog_state, listing_confidence?, review_level, reference_urls:[url], variant_key, rarity_code?, rarity_raw, premium?, serial_total?, int_id, decklog_available, decklog_verification:unverified/verified, decklog_source_url?, decklog_checked_on?, faces:[PrintingFace]` | 每版次一格、版本區、分享碼解碼、圖像定位                                                                                                                                                                                                                                                                |
| `product_family`         | `id, code, public_code, kind, name_unit_id, translations:[FieldTranslation]`                                                                                                                                                                                                                                               | /sets 與 set 搜尋                                                                                                                                                                                                                                                                                       |
| `product`                | `id, family_id?, region, product_code?, name_unit_id, product_type?, released_on?, date_precision, date_raw?, translations:[FieldTranslation]`                                                                                                                                                                             | 單卡頁補充的商品名稱與日期                                                                                                                                                                                                                                                                              |
| `printing_product`       | `printing_id, product_id, available_on?, date_precision?, date_raw?, inclusion_kind, note_unit_id?, first_inclusion_state`                                                                                                                                                                                                 | 複合 `PK(printing_id,product_id)`；單卡頁列出 PR／再錄收錄；初收錄（首次／再錄）篩選                                                                                                                                                                                                                    |
| `identity_change`        | `id, kind, old_card_id, new_card_id, printing_id?, data_version, reason`                                                                                                                                                                                                                                                   | 舊牌組身分修復提示、split 不猜                                                                                                                                                                                                                                                                          |
| `art`                    | `id, card_id, face_id, classification, review_level, regions:[region], artists:[{artist_id,role}]`                                                                                                                                                                                                                         | 每圖一格、只看異畫、繪師瀏覽                                                                                                                                                                                                                                                                            |
| `artist`                 | `id, display_name`                                                                                                                                                                                                                                                                                                         | 畫師名稱/篩選                                                                                                                                                                                                                                                                                           |
| `stamp`                  | `id, code, series_code?, text_raw, kind, displayed_year?, review_level`                                                                                                                                                                                                                                                    | 大賽標誌搜尋，不捏造場次                                                                                                                                                                                                                                                                                |
| `text_unit`              | `id, lang, text`                                                                                                                                                                                                                                                                                                           | 全卡原文/譯文/問答/詞彙字串去重                                                                                                                                                                                                                                                                         |
| `face_revision`          | `id, face_id, region, revision, effective_from?, effective_until?, temporal_status, change_kind, name_unit_id, effect_unit_id, class_code?, type_code, cost?, attack?, defense?, traits:[code], titles:[code], special_kinds:[code], sections:[Section], translations:[FieldTranslation], corrections:[Correction]`        | 現行與歷史卡文、數值、翻譯、更正徽章                                                                                                                                                                                                                                                                    |
| `translation`            | `id, source_unit_id, target_lang, text_unit_id, origin, authority, low_confidence`                                                                                                                                                                                                                                         | 只收當前有效選用譯文；來源類別與 authority 分開，低信心仍顯示並標待校對                                                                                                                                                                                                                                 |
| `qa`                     | `id, region, official_number?, source_url, current_version_id?`                                                                                                                                                                                                                                                            | 官方問答編號與連結                                                                                                                                                                                                                                                                                      |
| `qa_version`             | `id, qa_id, revision, published_on?, updated_on?, date_raw?, question_unit_id, answer_unit_id, state, cards:[card_id], translations:[FieldTranslation]`                                                                                                                                                                    | 問答全文/卡片關聯/同日歷史更新                                                                                                                                                                                                                                                                          |
| `cr_version`             | `id, region, version, published_on?, effective_on?, source_url`                                                                                                                                                                                                                                                            | 輔助提示的規則版本                                                                                                                                                                                                                                                                                      |
| `cr_clause`              | `id, cr_version_id, number, text_unit_id`                                                                                                                                                                                                                                                                                  | 離線逐字證據與條號                                                                                                                                                                                                                                                                                      |
| `errata`                 | `id, region, official_url, versions:[ErrataVersion]`                                                                                                                                                                                                                                                                       | 勘誤徽章、公告/生效分開、before/after/適用印刷                                                                                                                                                                                                                                                          |
| `ruling_revision`        | `id, ruling_id, revision, question_unit_id, decision_unit_id, strength, decided_on, review_state, active_scopes:[text_unit_id], replaced_scopes:[{scope_unit_id,replacement_revision_id}], cards:[card_id], evidence:[Evidence], hints:[{lang,text_unit_id,parameter_schema}]`                                             | 輔助說明、證據、部分取代與待重審警示                                                                                                                                                                                                                                                                    |
| `rules_name`             | `id, region, official_name`                                                                                                                                                                                                                                                                                                | 同名限張及禁限目標                                                                                                                                                                                                                                                                                      |
| `face_rules_name`        | `face_id, region, rules_name_id, role`                                                                                                                                                                                                                                                                                     | 複合 `PK(全部欄)`；同名/合作名/`treated_as` 構築                                                                                                                                                                                                                                                        |
| `rules_profile`          | `id, region, format_code, name_unit_id, revisions:[{id,effective_from,effective_until?,cr_version_id?,default_copy_limit?,construction_rules_ref?}]`                                                                                                                                                                       | 各區各賽制/日期規則                                                                                                                                                                                                                                                                                     |
| `restriction`            | `id, profile_id, announced_on?, effective_from, effective_until?, kind, state, max_copies?, max_selected_groups?, members:[{rules_name_id,choice_option,deck_scope}]`                                                                                                                                                      | 禁止、限張、二擇一/多選項；不以 `card_id` 漏限                                                                                                                                                                                                                                                          |
| `card_related`           | `id, from_card_id, to_card_id, relation, suggested_count?, applicable_regions?:[Region]`                                                                                                                                                                                                                                   | 進化/相關卡與牌組追加區建議；`same_rules_reskin` 為換皮卡指向原卡的「規則相同」提示：`applicable_regions` 必填（非空、去重、排序），由建置逐地區驗證結果填入，reader 只在目前卡面地區包含於其中時顯示，結果為空則不投影；其他 relation 為 null。不合併構築張數，reader 不得據此推導 DSL 共用或跨區等義  |
| `digital_card`           | `id, game, official_id`                                                                                                                                                                                                                                                                                                    | 官方數位頁連結，僅 SVE 所需閉包                                                                                                                                                                                                                                                                         |
| `digital_art`            | `id, digital_card_id, phase, style_key`                                                                                                                                                                                                                                                                                    | 數位進化面/styles/異畫對照                                                                                                                                                                                                                                                                              |
| `digital_link`           | `id, card_id, face_id?, digital_card_id, digital_phase?, relation, effect_similarity?, review_level`                                                                                                                                                                                                                       | 區分 `same_card/character/name_only`                                                                                                                                                                                                                                                                    |
| `digital_art_link`       | `art_id, digital_art_id, relation, review_level`                                                                                                                                                                                                                                                                           | 複合 `PK(art_id,digital_art_id)`；實體圖對數位圖                                                                                                                                                                                                                                                        |
| `digital_link_coverage`  | `card_id, game, state, as_of`                                                                                                                                                                                                                                                                                              | 複合 `PK(card_id,game)`；尚未核對與核對無結果分開                                                                                                                                                                                                                                                       |
| `voice`                  | `id, digital_card_id, digital_phase?, lang, kind_code, variant?, interaction_target_id?, label_raw?, source_url, asset_path?, mime?, bytes?, duration_ms?, availability`                                                                                                                                                   | 語音瀏覽與來源切換；不預載音檔                                                                                                                                                                                                                                                                          |
| `card_voice`             | `card_id, voice_id, usage`                                                                                                                                                                                                                                                                                                 | 複合 `PK(card_id,voice_id,usage)`；可播放的已採納對照/場景                                                                                                                                                                                                                                              |
| `keyword`                | `id, code, kind, name_unit_id, definition_unit_id?, actions:[{action,label_unit_id}], translations:[FieldTranslation]`                                                                                                                                                                                                     | 關鍵字/資源及產生消耗篩選                                                                                                                                                                                                                                                                               |
| `mechanic_projection`    | `card_id, keyword_id, scope, relations:[code], actions:[code]`                                                                                                                                                                                                                                                             | 複合 `PK(前三欄)`；has/grants/refers/counts 與三態搜尋                                                                                                                                                                                                                                                  |
| `card_mechanic_coverage` | `card_id, scope, complete_all, complete_mode:include/exclude, complete_keyword_ids:[id], partial_mode:include/exclude, partial_keyword_ids:[id]`                                                                                                                                                                           | 複合 `PK(前兩欄)`；稀疏三態搜尋的完整性，不展開 absent 列                                                                                                                                                                                                                                               |
| `card_engine_support`    | `card_id, shared:Support, overrides:[{region,support:Support}], region_blocks:[{region,reasons:[code]}]`                                                                                                                                                                                                                   | PK(`card_id`)；未實作頁/單卡徽章/建牌數量/手動回退                                                                                                                                                                                                                                                      |
| `vocabulary`             | `kind, code, label_unit_id, active, translations:[FieldTranslation]`                                                                                                                                                                                                                                                       | 複合 `PK(kind,code)`；穩定英文 filter code，多語 label 經 translation                                                                                                                                                                                                                                   |
| `search_alias`           | `kind, code, lang, text, normalized`                                                                                                                                                                                                                                                                                       | 複合 `PK(kind,code,lang,text)`；三語輸入/Discord 暱稱                                                                                                                                                                                                                                                   |
| `text_symbol`            | `id, code, parameter_schema, keyword_id?, spellings:[Spelling], localizations:[Localization]`                                                                                                                                                                                                                              | 自製圖示、aria、tooltip、複製文字                                                                                                                                                                                                                                                                       |
| `card_route_alias`       | `namespace, old_key, target_namespace, target_key, reason`                                                                                                                                                                                                                                                                 | PK(`namespace`,`old_key`)；改卡號永久轉址；official 路由由已確認 `card_no` 推導；provisional 用 `int_id` 保留命名空間                                                                                                                                                                                   |
| `route_override`         | `route_key, printing_id`                                                                                                                                                                                                                                                                                                   | PK(`route_key`)；僅多 variant 的穩定入口例外，正常卡不重複 route 列                                                                                                                                                                                                                                     |

上述共 40 個公開陣列；這是物件投影的集合數量，不是建置表數。config 是單一設定物件，精確型別見 [傳輸契約 §3](snapshot-transport.md#3-config-與巢狀-tuple-型別)：languages（`code/fallback_order/display_name`）、`digital_endpoints`（`game/card_url_template/language_map/status/refresh_policy`）、`shop_links`（`id/url_template/parameters/feature_key/enabled_dev/enabled_prod`）、`image_sizes`（key/purpose/max_width/max_height）、search（`grammar_version/normalizer_version`），另含 `catalog_feedback_url`、`third_party_image_policy`（固定 `mirror_reviewed`）、`deck_eligibility_policy`（固定 `regional_decklog`）；v1 producer/reader 必須實施這兩項已決政策。不含作者/hash/審核流程。僅 `image_sizes` 在影像清單中也會被引用；避免兩份獨立設定，快照清單指向同一 config 檔。

| 巢狀型別         | 完整欄位與語意                                                                                                                                                                                                                                                                                                                         |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------              |
| RegionView       | `region, release_state:released/unknown/announced/not_released_confirmed, mapping_state:unmapped/pending/confirmed/confirmed_none, as_of, mapping_as_of?, mapping_scope?, default_printing_id?, default_method?, deck_role?, debut_product_ids:[id], debut_state:known/unknown`；來自建置資料庫推導，無法判最早時 unknown              |
| PrintingFace     | `face_id, art_id?, frame_code?, signed?, embellishment_state, printed_name_unit_id?, printed_effect_unit_id?, flavor_unit_id?, printed_text_state, observations:[ObservedText], sections:[Section], stamps:[{stamp_id,position?,color?}], translations:[FieldTranslation], corrections:[Correction]`；不帶 `card_id/credit_raw/source` |
| WordingView      | `region, state:pending, display:{revision_id?,basis:current/latest_known_release/candidates}, candidates:[{printing_id,revision_id?}], undated_printing_ids:[id]`；暫顯不等於 current，見 §2.3                                                                                                                                         |
| ObservedText     | `revision_id?, state:available/missing_effect/correction_conflict, source_url`；同版次觀測，不等於 printed，見 §2.3                                                                                                                                                                                                                    |
| Section          | `ordinal, text_unit_id, kind`                                                                                                                                                                                                                                                                                                          |
| FieldTranslation | `field:name/effect/flavor/question/answer/section/label/action_label, ordinal?, target_lang, translation_id, basis:own_source/shared_jp/shared_jp_unchecked/official_counterpart`；逐 card/face/region 選用，不能只以共享文字單元判同語義                                                                                              |
| Correction       | `field, corrected_from:JSON, is_corrected:true, reason, source_url?`；綁引用者，不標污染去重文字單元                                                                                                                                                                                                                                   |
| ErrataVersion    | `id, revision, announced_on?, effective_on?, date_raw?, reason_unit_id?, exchange_offered?, changes:[{face_id,field,before:JSON,after:JSON}], printings:[{printing_id,scope}]`；版本不可變，`scope=listed/confirmed_applies` 沿建置資料庫範圍，面由 `changes.face_id` 定位                                                             |
| Evidence         | `qa_version_id?, cr_clause_id?, source_url?, role, quote, locator?`；前三欄恰一；非 QA/CR 的官方來源仍保留 URL/quote                                                                                                                                                                                                                   |
| Support          | `status, dsl_status?, dsl_version?, dsl_id?, validation_state, reasons:[code], reason_detail?, program_ref?, ruling_revision_ids:[id]`；automatic 由套用區域 block 後 `effective_status=engine_passed` 推導，不另存重複布林                                                                                                            |
| `program_ref`    | `file_key, entry_id`；由快照清單得檔 hash，entry 是已展開 AST，無 `author_macro`；draft/invalid 可 null；機械 verified/fresh 門檻只在建置資料庫計算                                                                                                                                                                                    |
| Spelling         | `lang, literal_prefix, literal_suffix, parameter_name?, parse_kind:literal/uint/variable`                                                                                                                                                                                                                                              |
| Localization     | `lang, name, tooltip, copy_pattern`；純字串，以 `parameter_schema` 驗參數                                                                                                                                                                                                                                                              |

卡表快照的 `card_engine_support.shared` 可以是 `missing_dsl`；EN-only 文件放 overrides；`region_blocks` 至少含未確認對應/語義差異/未有該區來源的理由。消費優先選 region override，否則 shared，最後套 `region_blocks`；block 強制手動且合併 reasons。若選中 `status=engine_passed` 但有 block，該區 `effective_status` 降為 reviewed（顯示「共用實作已審核，此區待核對」），不能在未實作清單顯示此區已通過。其餘四態保留並附區域原因；automatic 恰為 `effective_status=engine_passed`。不得從沒有 override 推斷「英版已確認」；block 完整性是發布閘門。

translation 僅輸出上述欄位，用 `text_unit_id` 取譯文；同一 chosen translation 可供多個引用者。`origin` 固定為 `official/project/machine`，`authority` 維持 `sve_official/digital_official/unofficial`；數位官方名稱不因此取得實體官方 counterpart 資格。`low_confidence` 是必填 Bool，原樣保留 current producer 的結果；true 直接顯示譯文、標「待校對」並提供該 owner 的原文，false 不表示逐筆人工確認。不輸出或查詢舊 `status`，不把 false 轉成 reviewed，也不把所有 machine 改成 true。官英/官方日文 counterpart 由建置產生選用記錄（`origin=official, authority=sve_official`），無需人工 equivalence 表。繁中依據的 `source_unit_id` 仍是 JP 來源；本文的 shared_jp／shared_jp_unchecked 是切換前 basis，四層的選用政策依翻譯契約、新版 basis 由 #496 定義。官方 counterpart 逐 owner 直接引用，不占共用 translation_selection。詞彙與商品標籤亦在引用者存 FieldTranslation，不以共享 `source_unit` 字串去猜唯一翻法。keyword action 用 `field=action_label`、ordinal 對應 actions 順序；其他標籤 field=label。

`keyword.name_unit_id` 由建置資料庫的 `glossary_term.source_ja` 建文字單元，translations 選同概念多語名稱；`definition_unit_id` 另供說明。relation/action 篩選的三態必合併兩個稀疏集合：指定 relation/action 在 projection 集合內→present；無吻合但 `coverage.complete_all=true` 或 keyword 在依 `complete_mode` 解碼的 complete 集合→absent；否則 unknown。另一 relation 已 present 不能推本項 absent。未列 coverage 等於尚未檢查；`partial_keyword_ids` 不代表完整。只有 fresh 標籤進 projection，universe/source/producer 改版重算 coverage；EN region block 使不適用的 shared 結果 unknown。config.search 的版本必與 reader 能力匹配，normalized 別名與使用者輸入用同一規則。

### 2.1 獨立影像清單與 DSL 附件

2.0.0 的三個影像集合如下。

| 集合 | 公開欄位 | 鍵與用途（未註明 PK 者以首欄 `id` 為 PK） |
| --- | --- | --- |
| `image_asset` | `id, source_src_raw, source_url, width?, height?, format?` | PK(id)；來源詳情，沿既有型別 |
| `printing_image` | `printing_id, face_id, image_id, publication_state, availability, card_version?, art_version?, variants` | PK(printing_id,face_id)；卡包 media 的當頁顯圖投影 |
| `image_variant` | `image_id, size_key, format, width, height, bytes` | PK(前三欄)；當次輸出的詳情尺寸／大小，不含公開 path、不阻擋首圖 |

publication_state 為 `pending|approved`，availability 為 `available|missing|unfetched`；公開格式只在 printing_image 放狀態，image_asset 不重複。兩個 version 為正安全整數或 null。
variants 是按 size_key 排序唯一的 `ImageDisplayVariant=[size_key:Code,width:UInt,height:UInt]` tuple 陣列，尺寸皆 >0。
approved 且 available 時 card_version／art_version 均非 null、恰有五檔；其他狀態兩者均 null、variants=[]，不得組可用 URL。
每個 display variant 與 image_variant 的尺寸一致；完整 bytes／SHA 由發布器核實，不能只驗 metadata。
同 image_id 被不同版次引用時，各版次的路徑及版本獨立，來源詳情可共用。

卡圖 URL 為 `images/<size>/<int_id>[-f<ordinal>].webp?v=<version>`；ordinal=0 省略後綴，其餘使用永久 face.ordinal，不用陣列位置。
int_id 讀 printing 的永久配號，不能由卡號或號段推 region；image_id 只供來源／詳情 join。
card_s／m／l 取 card_version，art_s／m 取 art_version；ID、ordinal、版本皆用無前導零的十進位整數。
query 由 reader 附加，不寫入 File.path；背景資源 URL 與玩家卡片頁路由分開。
版本配發、產製／覆寫及新鮮度規則見 [卡圖衍生檔契約](../images/image-variants.md)。

printing_image 唯一存於 printing.home_set 的 media 分片，取得文字啟動包及一層卡包 metadata 後即可組圖；
image_asset／image_variant 是按需詳情，不再為顯圖預取全域影像表。裝檔與精確依賴見[傳輸契約 §5.4](snapshot-transport.md#54-format-200-卡包-media-與-id-圖片)。
普通 img 成功不宣稱端到端 SHA 校驗；歷史圖片 metadata 不保證目前同 key 仍有歷史 bytes。
JSON hash、當前 metadata FK 與來源核可檢查不變；建置／發布計畫仍保存完整輸出 hash 與本機內容定址位置。

DSL 程式包（`dsl-programs`）不是集合；封套及 AST 驗證統一依 [傳輸契約 §3.4](snapshot-transport.md#34-dsl-程式包與版本准入)。
所有 reference（含 JSON 內 ID）由 producer 驗閉包；瀏覽器不全量驗 FK。voice.asset_path 仍是不透明 CDN path；本次只改卡圖，不由卡號推算官方來源網址。

### 2.2 留在建置端與延後項

建置資料庫才有 `source_record`、`printing_face_observation`、`translation_context/use`、`decision/decision_source`、`correction_evidence/application`、所有模板/術語/translation binding/history、ruling review/supersession/template、DSL load/exam/review/source/macro、`card_mechanic/coverage/override`、`digital_text/face/source` 對齊、`restriction_event`、`build_issue`。卡表快照的引用均投影成公開 ID/文字，不殘留指向建置資料庫的 FK。

只保留玩家查閱需要的歷史：face revision、勘誤、QA 歷史、被引用的 CR、裁定有效片段與被替代鏈。完整 CR 當前版與裁定引用的歷史 CR 條文進文字包；未被引用的舊 CR 全文可不出貨。已不用的翻譯/文字單元不出貨，避免累積所有建置歷史。

延後 `event/printing_event`、`signature/printing_signature`、`artist_link/art_post`、`query_alias`、Decklog 外部 ID 格式；不是先做空表出貨。provenance 不是離線文字包，也沒有預設的公開按需稽核包；如未來要開發者除錯另設發行產物，不能偷偷加回玩家快照清單。

### 2.3 表記未定的公開呈現

**使用者 2026-10-01 核可本節暫顯規則**：觀測有差異、沒有舊 current 的卡仍可讀，不因未採納排除。以下公開欄位擴充對應 §2 白名單。公開形狀、Schema／types／golden 與 reader 依目前 2.0 的 [snapshot-contract](snapshot-contract.md) 同步維護。

- `PrintingFace.observations` 是 `{revision_id?,state,source_url}` 陣列；state=`available/missing_effect/correction_conflict`。available 必有同 face、同 printing.region 的公開 revision；missing_effect 的 revision_id=null，表示原觀測主文未知且無法建 revision；correction_conflict 可有原觀測 revision 或 null，只供帶警告查閱，不能作可信暫顯。source_url 是該 printing 的原始來源 URL，不出 source ID、hash 或決定。完整三欄 exact 相同才去重；按來源 URL／狀態／revision ID 的固定字串序儲存**不表示年代**。
- `face.wording` 是稀疏陣列，只對表記未定的公開 face-region 各出一項，恰有 `{region,state,display,candidates,undated_printing_ids}`。state 固定 `pending`；display 恰有 `{revision_id?,basis}`，basis=`current/latest_known_release/candidates`。candidates 是 `{printing_id,revision_id?}` 陣列，恰列仍需核對的候選版次／revision，去重並按 printing_id／revision_id 固定排序，null 為尚無可表示 revision；undated_printing_ids 列無可信完整日精度收錄日的候選版次，去重排序。這些 ID 排序僅供穩定序列化，UI 的年代由日期證據顯示，不能用 ID 補順序。
- 已有有效 current 且無待核對候選的 face-region 為 settled，省略該 region 的 wording 項目，不複製 current 的 display；所有 region 均 settled 時 `wording=[]`，固定 tuple 欄位仍保留。reader 由同 region 有 current 且無 wording 項推得 settled；沒有 current 不能藉省略項目冒充 settled。pending 保留所有待核對候選，即使 display 能選出暫顯值也不能省略；其中每項須能連回同一 printing face 的 observations，非 null revision 必同 face／region。undated_printing_ids 必為 candidates 的 printing 子集，與可信日期判別一致。

producer 按下列規則計算 pending 的 display，reader 不自行推論 current：

1. 有仍有效的已採納／機械 current，display 指它、basis=current；新候選不覆蓋它，也不能因 current 存在把 pending 改成 settled。
2. 無 current 時，在所有有可信完整日精度首次收錄／發售日的候選版次中比較，包含主文待確認／更正衝突者，不先丟掉問題版本。日期讀 product／printing_product，依既有 precision 繼承與 unknown 覆寫規則；多重收錄取可驗首次取得日。若其他收錄日期未知而無法證明首次日，該 printing 亦列 undated，不任取已知商品日期。只讀「目前已知日精度版本的最新值」，不是全部版次的官方最新表記。
3. 最大日期涉及的觀測全部 available 且投影內容 exact 相同，display 指其中一個 revision、basis=latest_known_release。多個 exact 同內容 revision 取 ID 字串序第一個僅供內容定位；仍保留所有版次／觀測，不由 ID 斷言先後。同日不同內容、同 printing 有多個無法排序內容、或沒有上述可用日期時，display.revision_id=null、basis=candidates，列全部候選。已知日期最新的版次若仍有 missing_effect／correction_conflict，也不能退取較舊版假稱「已知最新版」；改列候選與問題。
4. 未知／月年精度版次獨立顯示日期未定清單，即使第 3 步有暫顯值也不隱藏，交 Artifact 頁詢問使用者。不補一號、不以 fetched_at／卡號／hash 排序；目前沒有採納輸入，不能由 UI 點選候選靜默建立 current。

printing 頁顯示自己的 observations 文字，並標為官網觀測；有多種內容且沒有可驗來源更新順序時列全部，不任取最後爬到者。不將它填進 printed_name_unit_id／printed_effect_unit_id 或提升 printed_text_state；有成功 source correction 時顯示該來源的更正後 revision 與既有 Correction，原始來源觀測仍留建置歷史。沒有 current 的 card／face 顯示上述暫顯值與「表記未定」，basis=latest_known_release 另標「依已知發售日暫顯」；basis=candidates 則列候選，文字未知者顯示待確認而非無能力。雙面各自保留狀態，不能只出一面。

所有非 null revision／printing／文字引用均須在同份快照可達，候選不指向 report 或不存在的 revision。未解表記不刪 route、預設版次或 Decklog 可用性，也不能拿 #144 診斷排除閉包決定發布範圍。沒有 current 的 pending region 在 card_engine_support.region_blocks 加 `wording_pending`，強制手動並禁止由暫顯值自動繼承 DSL／跨區翻譯；有舊 current 時照既有 freshness／衝突檢查，不一律宣告新候選已通過。真正來源損壞、錯身分與勘誤／更正衝突仍按各自閘門處理。

分片時，稀疏的 `face.wording` 隨 face 進啟動包，帶未定狀態、暫顯／候選 revision 引用及未知日期版次 ID；PrintingFace.observations 進 printing 詳情。`current_ref` 為所有 face.current.revision_id 的去重集合；`display_ref` 為 current_ref 加上 pending wording.display 非 null revision ID 的聯集，**不加入其餘 candidates**。每個 pending face-region 最多一筆 display revision，其 §3.1 輕量投影、名稱及可用名稱翻譯的文字閉包均進 bootstrap，讓卡表、名稱搜尋與 facet 首屏即可使用暫顯卡名／數值；其餘欄位仍走 detail。這只是顯示分割，不建立 current 或採納。

不在 display_ref 的候選及其他 revision 沿用 `history` 分片保存完整列，所需文字／翻譯沿既有 bucket 分片閉包按需載入。history 是傳輸分區名稱，不宣稱候選年代較舊；候選恰與 display_ref 共用 revision 時沿用 bootstrap/detail 的唯一儲存，不另存複本。啟動包的 dependencies 不得因其餘候選引用而強制預載 history；producer 仍驗同快照的完整引用閉包。

開啟卡文詳情、候選比較或搜尋需要其餘候選內容時，reader 依 face 所屬 card 的穩定 owner 載入相應 detail/history 與文字依賴；既有背景下載與 PWA 全預取照常，但不作首次顯示的阻擋條件。有 display 的卡首屏使用其名稱與數值，標表記未定；沒有唯一 display 時保留卡號與「表記未定／候選載入中」，不能視為空文字、無候選或排除該卡。沒有唯一 display 的 card facet，候選載入後只對全候選一致的欄位給單值，其餘保持未定；尚未載入時標部分索引與進度，不把缺少值當篩選不符。名稱搜尋先用 current／display 名稱，需搜尋其餘候選名稱與全文時逐片載入並取可讀候選聯集，未完成明示部分結果，結果標表記未定，不使用某候選數值作已採納規則。

須以當次日英與實際可用名稱翻譯閉包，分別量測所選版本的啟動 Brotli：manifest、config、首屏必載 bootstrap（含暫顯 revision 的輕量投影、名稱／可用名稱翻譯文字閉包）與稀疏 wording 引用開銷，並分列候選按需分片容量。約 1 MiB 是盡量達成的目標，2 MiB 可接受；超過 2 MiB 停下交維護者決定，依 [size-budget.md](size-budget.md)。混區檔依實際整檔計費，不能只量單片、按語言比例拆帳或以候選未下載宣稱全庫索引完成；本文件不宣稱手機或完整三語驗收已通過。

## 3. 分檔、下載順序與記憶體

穩定 owner 是資料分片規則，與商品收錄分開：card/revision/support 跟 `card.home_set_id`；printing 跟第一次配發的 `printing.home_set_id`；再錄新 printing 到新 owner，舊列不搬。`printing_product` 新關係跟 printing owner，可引用舊 card，不能複製舊 card。shared 字典/QA/CR/`text_unit` 依固定 ID bucket；`text_unit` 不按「最新使用者」移動 owner。各 family 過大時依已固定 ID bucket 拆子片，快照清單決定檔案；翻譯/勘誤只更動涉及的片。

三語**全部文字**首次即排入下載，PWA 安裝全預取；優先順序：快照清單/config → 全域輕量名稱/卡號/數值索引與目前卡面語言 current 片 → 背景補其他語言/歷史/QA/裁定/metadata。啟動包是同一公開列的欄位主儲存，主表詳情只存其餘欄位；其完整契約見 §3.1，producer 比對與主投影一致。直接連結單卡先拉所需 owner。facet 索引閉包完成後，全庫篩選不需解析卡文片；尚未完成顯示部分結果及進度。效果全文才逐片掃描。

**下載完成不等於全部常駐 JS heap。** Service Worker 將已驗 bytes 存 CacheStorage；用 Worker 逐片 JSON.parse、只將當前 UI/查詢所需的面、語言、索引保留。跨全庫全文搜尋由 Worker 逐片掃描（去抖動、取消舊查詢、傳部分結果）或使用持久化全文索引；結果頁只傳 id/摘要，LRU 釋放詳情，不把整份複製給主執行緒。預取不呼叫全包 JSON.parse。離線切語言從本機 cache 讀，不再等網路。facet 索引必須建置並持久化至 IndexedDB，按快照清單 hash 隔離；拒絕持久儲存時可重建記憶體索引並標示離線限制，不維護獨立資料真值。

完整文字包 `text_all` 僅供完整下載/工具匯入，可串流讀；瀏覽器預設用分片。完整文字包與分片是替代下載方式，不雙倍下載。快照清單的 `text_all.contains` 列出全部 required text keys，closure hash 皆驗過才標「文字離線備妥」。首訪部分可讀狀態不等於 active 完整離線版。

容量與記憶體預算、量測方法見 [size-budget.md](size-budget.md)。容量門檻：卡表快照（完整文字分片合計，不含卡圖、語音與 DSL 程式包）Brotli 壓縮後 ≤ 8 MiB（gzip 傳輸時 ≤ 10 MiB）、解壓後 ≤ 40 MiB，啟動包依所選日版／英版的 Brotli 約 1 MiB 為目標、非硬門檻，2 MiB 可接受，更大需先由維護者決定。記憶體目標：卡表常駐 heap ≤48 MiB、更新峰值 ≤80 MiB（不含 WebGL/圖片/app）；單片解壓 ≤512 KiB、背景解析單段主線程工作≤50 ms、完整可搜尋資料解析累計≤1秒，這些是待手機實測驗收值。完整文字／資料單片超標停止發布效能驗收，調整投影/固定配置；啟動停點依上述規則，不能只有 gzip 小就宣稱手機順暢。

### 3.1 啟動包是欄位主儲存

§2 的 40 個文字集合是完整**邏輯讀取視圖**，不是額外下載的全欄主表。傳輸以同一 table 的固定欄位分割（fragment）分成啟動包（bootstrap）和詳情分片（detail），跨表 FK 保留永久 ID；同表詳情分片不重複永久 PK，而以片內 `row_index` 指向啟動包列（printing.faces 用 `face_ordinal`），每個實體欄位值只在一處。以下為完整分割規則；「其餘」精確指 §2 白名單扣去該列啟動包欄，非任意省欄。

| 邏輯集合/列範圍 | 啟動包唯一儲存欄位 | 詳情分片唯一儲存欄位 |
| --- | --- | --- |
| card、face、`product_family`、product、`printing_product`、`identity_change` | 全部欄位 | 無資料列，不發僅 PK 的空殼 |
| `printing` | id 及除 faces 外全部欄位；faces 的裝飾欄位分割為 `face_id/art_id/frame_code/signed/embellishment_state/stamps` | id；faces 的文字欄位分割為 `face_id/printed_name_unit_id/printed_effect_unit_id/flavor_unit_id/printed_text_state/observations/sections/translations/corrections` |
| `face_revision`：§2.3 display_ref 的現行／暫顯列 | `id/face_id/region/name_unit_id/class_code/type_code/cost/attack/defense/traits/titles/special_kinds`；translations 中 field=name 的列 | id＋其餘白名單欄；translations 僅非 name 列 |
| `face_revision`：display_ref 外的候選／其餘觀測／歷史列 | 無；以 §2.3 的 `display_ref` 區分，不另出貨集合 | history 的完整列，按需載入（含其餘候選，不是現行／暫顯複本） |
| `card_engine_support`、`mechanic_projection`、`card_mechanic_coverage` | 全部欄位；status/reasons 不複製進 `card_facet` | 無 |
| `rules_name`、`face_rules_name` | 無 | 全部原欄位，global detail |
| `rules_profile`、restriction、vocabulary、`search_alias`、keyword、stamp | 全部欄位 | 無 |
| `text_unit` | 現行／暫顯名字、可用名字翻譯與 facet 字典引用的 ID 閉包；每個文字 ID 只在其固定 bucket 的啟動包或詳情分片一邊 | 非上述閉包的其餘文字列 |
| `translation` | 現行／暫顯 name 與 facet labels 使用的翻譯列 | 其餘翻譯列；同 ID 兩用途時歸啟動包，只存一次 |
| 其餘文字集合 | 無 | 完整白名單列 |

`rules_name`／`face_rules_name` 的全部原欄位在 2.0 唯一存於 global detail，依 [傳輸契約 §5.1](snapshot-transport.md#51-format-200-固定配置) 定位。兩表的 PK、欄序、型別及邏輯參照不變，也沒有 row_index/base。名稱搜尋與一般 facet 仍用 current／display 的名稱閉包；同名規則／構築功能按需取兩表，未完成須標「規則資料載入中／未備妥」，不能當作沒有同名限制或完整合法性。其他表與 printing 診斷／art_id／printing_product／support 的存放均不變。

表中 `id/face_id` 是 join 後欄名；詳情分片傳輸以 `row_index/face_ordinal` 取代這些重複鍵。`row_index` 是該啟動包欄位分割已排序 rows 的位置，不是永久 ID；快照清單 dependencies 必釘精確啟動包 key/hash，錯版本/越界/同 ordinal 重複皆拒絕。當啟動包排序改變，相關詳情分片必重建，不能沿用舊 `row_index`；這會增加更新片數，是省去複本鍵的明示取捨。display_ref 外的 `face_revision`（含其餘候選）使用獨立的 history 分片與完整列，不混在現行／暫顯詳情分片的 columns。欄位分割的 columns/type 白名單由 format 固定，files.role 指儲存層，`row_counts` 按欄位分割實際列數計；producer 另外驗 join 後邏輯主鍵唯一、必填欄齊與無欄位重複。printing.faces 兩片以 `(printing.id,face_id)` 一對一合併；translation 子陣列按 `field/ordinal/target_lang` 合併且不重複。完整文字包只是這些欄位分割的容器聯集，仍維持分割，不額外打包全欄複本。owner/bucket 穩定，不因分片切換改永久 ID；字典選用狀態變更可讓該 bucket 的欄位分割內容更新。

`card_facet/printing_facet/rules_facet` 只是 reader 建出的索引視圖，**不出貨三份 facet 列**。職業/作品/類型/數值/特性從現行 `face_revision`；無 current 的表記未定依 §2.3 的暫顯／候選及未定 facet 規則，不因此隱藏卡片；稀有度/標誌/圖從 printing 與裝飾片；三語名字從 name 的 `FieldTranslation→translation→text_unit`，缺譯 state=missing 並回原文；引擎五態按 `Support/region_blocks` 推導。Q&A/errata 的存在與否用快照清單的兩個稀疏 card ID 集合 `qa_card_ids/errata_card_ids`，加 `source_windows` 覆蓋判 present/absent/unknown，無閉包不能假 absent。兩集合是唯一額外 facet 摘要，容量另量。`rules_facet` 從已下載 rules/profile/restriction 與指定日期計算，不存第二份每卡/賽制摘要；也不能把單卡摘要當整副牌合法性。

依使用者 2026-09-30 的規格變更（build-db §15），卡包（`set=`）facet 由已登錄的 `printing.home_set_id` 建立，以 `product_family.code` 為篩選值，並限定 `printing.region` 為全站目前選定的版本（`jp`／`en`），兩區結果不混。合併卡片顯示也只使用命中的版次；`card.home_set_id` 不代替這項篩選。商品與收錄（`product`／`printing_product`）供單卡頁補充資訊與連結，不作卡包（`set=`）facet，也不由卡號前綴推斷商品收錄。初收錄（首次／再錄）是獨立 facet，可依收錄資料建立；資料缺少或狀態未知時標示 coverage，不因此隱藏卡片（build-db §15 的協調者決定）。

啟動包支援 current／display 的名稱與全部 facet；無唯一 display 或其餘候選的 facet 依 §2.3 按需補載，未完成須標部分索引；效果全文仍 Worker 掃詳情分片，查圖分組另載 art。reader 建完 typed posting indexes 後**丟棄啟動包 tuple 陣列與 JSON 解碼字串**，數值進 TypedArray、code 進小整數字典，文字/不可丟欄位進唯一字串池或緊湊欄式 store；view 僅持 ordinal/ID，不能閉包引用原 row。IndexedDB 索引依快照清單 hash 隔離，不能形成第二真值。手機驗收量「建索引後常駐」JS heap＋ArrayBuffer/字串池、詳情 LRU/當頁 view 與更新期間峰值；48/80 MiB 門檻待手機實測驗收。

`complete_mode=include/exclude` 先在快照清單 keyword universe U 解碼 complete；`partial_mode` 再於 `U\complete` 解碼 partial。各集合選較短的正集/補集，同長選 include，列表長度不得超過其基底一半；兩集合解碼後互斥。`complete_all=true` 時 mode 皆 include、列表皆空。未知=`U\(complete∪partial)`，不因壓縮改成 absent。這個編碼也需 column-partition-v1 reader 能力，不能讓舊 reader 把補集當正集。

## 4. 更新、相容性與離線

比前一快照清單的檔 hash，重用未變片；新片在 staging 驗完完整 required 閉包後原子切 active。失敗/中止/空間不足保留舊完整 active，首訪失敗不稱離線備妥。更新時不把新舊兩版全解析到 heap；磁碟峰值另算本機 active＋更新 staging 新片，CacheStorage 容量失敗須有可恢復訊息。

row delta 延後，以替換不可變 JSON 分片更新。未知 format／規則 enum／能力不能默默忽略；只在 current／previous 或本機完整 active 中找相容者，無相容版則提示更新 reader，不從全歷史索引搜尋。保留本機舊資料時明示時效，合法性 unknown，自動對戰拒絕不支援能力。對局重播與歷史牌組還原留待另訂，不要求公開快照或 WebP 永久可取回。

card images 依完整 URL（含 v）快取或供已選牌組離線使用；啟用新版時改用新版 v，不回退舊圖，下載失敗明示 placeholder。尚未連線取得新版者明示資料時效，不宣稱圖片最新；看過不等於所有圖片離線備妥；雙面兩張皆列，追加區只抓玩家實選。音檔與圖片各自狀態，不影響文字完成判定。不預抓隱藏對手的卡，也不把未實作提示洩漏其牌組。

身分修復的永久 printing／int_id、卡片入口舊 URL 與 split 玩家選擇，沿 build-db §13／§15；快照保留僅依下述 §4.1，
建置端的追加封套與公開事件映射另見 [身分修復契約 §6](../domains/identity-repair.md#6-公開事件墓碑與路由)。
§2 表格列現行 2.0 的公開形狀；實作撤回時在原欄序尾端
追加 reverts_id，identity_change 新增 kind=revert 與 required nullable reverts_id，
一般事件填 null，撤回列指原公開事件並保留原 old/new／printing 欄位，不代表反向邊。
reader 先移除被指名的有效事件再解析修復圖；原事件與撤回事件皆保留，不改舊快照。
format `2.0.0` 仍為候選時，依 [機器契約的候選期規則](snapshot-contract.md) 在候選內同步修訂
Schema、欄序、golden 與 reader，不要求額外升版；正式凍結後才至少升 minor、加入
`identity-revert-v1` capability 並提高 min_reader_version。未支援的 reader 依 §4 只選 current／previous 或本機 active 的相容版，否則提示更新。
不將 decision、完整移轉清單或逐列稽核 hash 出貨。

### 4.1 發布窗口、圖片新鮮度與回收

公開 CDN 的卡表快照只保留**最新版（current）及其直接前一發布版（previous）**，previous 僅供更新過渡。首次發布 previous 為 null。每一保留版本的清單、文字／其他分片、完整文字包（若有）、config、影像 metadata、programs、changes 及所列壓縮表示，均保留其必要 JSON／檔案閉包；共同內容依 hash 去重。清單及內容定址 JSON 一經寫入不可覆寫，但輪替出保留窗口後可依引用集合回收。「內容不可變」不表示「永久保存」。

**卡圖 WebP 僅提供當前圖片，不保存歷史圖片版本。** previous 或更舊 metadata 中的圖片引用不使舊 WebP 成為必留資產；其 image hash／bytes 不構成永遠可取得同一圖片的承諾。圖片產製、來源核可、格式／尺寸／內容檢查仍在發布前驗證。新版快照須提供足以選取新圖片的版本 token 或新 path；client 啟用新版後，瀏覽器／CDN／SW 不得以舊版本圖片作快取回退。未完成下載顯示載入狀態，失敗明示，不以舊圖充作新版圖。

版本入口是可更新的 current／previous 發布索引，revision 單調增加，不維護 append-only 全歷史 pages 或每代索引的公開歸檔。每個 entry 指向不可變 manifest 的 path／hash，並提供格式、最低 reader 版本與所需能力等准入資訊。data_version 不重用，同一版號不可改指另一份 manifest；上傳只能對照遠端 current／previous，不保存更早的歷史。索引必在新版所需內容及圖片新鮮度驗證完成後，以條件寫入原子切換；索引不得設為 immutable 快取。

客戶端由 current 更新；若其本機版本恰為 previous，可使用對應 changes 摘要及未變 hash 重用分片。落後多版、所需舊片已回收或缺少相容前版時，直接取得 current 的完整所需閉包，不要求補齊歷史鏈；未知格式／能力時提示更新 reader，不猜讀。previous 是有限的過渡退路，不保證永遠有可供舊 reader 下載的相容版本。裝置已有的完整快照可在更新失敗時保留為本機 active 並明示時效，但不使伺服器延長歷史保留。

新版本提交成功後，回收不再被 current／previous 引用的公開 snapshot 產物；卡圖回收只依**當前圖片集合**判斷。回收在每次刪除前重讀索引，索引變動即停止；不在上傳進行中回收，因尚未進索引的新檔會被當成未引用。中斷留下的未發布檔案可清理後重傳，不列為第三個保留版本。失敗不得把半套新版宣告為 current。

本階段不承諾任意歷史 data_version 的重新下載、對局重播或舊牌組載入當年卡表。相關能力待實際開發時另訂保留／匯入方式，不以此限制當前發布策略。永久 ID／int_id、必要路由 alias、authored 採納及凍結來源歸檔仍依各自契約，不因公開 snapshot 回收而改寫或刪除。

索引入口為 `snapshots/versions/index.json`，形狀恰為
`{index_format:2,revision:UInt,current:Entry,previous:Entry|null}`。
Entry 恰含 `data_version,published_at,format_version,min_reader_version,required_capabilities,manifest_path,manifest_sha256,engine_support_target`，型別沿 manifest；
manifest_path 為 `snapshots/manifests/<64hex>.json`，manifest_sha256 為其 canonical bytes 的 Hash。
payload 沿 `snapshots/blobs/<64hex>.json` 及 `.br`／`.gz`；兩版共用 blob 只存一次。
revision 是正安全整數，current／previous 的 data_version 不同，previous 恰為直接前一發布版；首版為 null。
卡圖版本號由匯出端的高水位整數配發，失敗的匯出也消耗號碼，不得重用；不另保存發布收據或首次事件收據。
reader 只從 current、previous 或本機已驗完整 active 選相容者；index_format 不支援時提示更新，不能猜讀。

changes 是相鄰發布摘要，不是重建鏈。previous manifest 引用的 changes blob 仍保留，
但其 from_data_version 只是批次識別，可指向已回收版本，不遞迴保留第三版。
慢 client 遇到已回收分片須重讀索引並更新；不以無限延長保留期維持過時下載。
本機引擎的確定性重播與來源重算仍可保留，均不構成公開歷史資料下載承諾。
決策理由見 [ADR-0016](../../adr/0016-snapshot-retention.md)。

### 4.2 預覽快照

預覽快照是正式匯出器產生、與卡表快照同格式的開發產物；僅供非公開開發，不發布給使用者。匯出器的公開根由 `--preview-dir` 或 `SVE_EXPORT_DIR` 指定；web dev server 以 `SVE_EXPORT_DIR` 掛載 `/cdn`（未設定時為合成 fixture），另以 `SVE_PREVIEW_DIR` 掛載 `/cdn-preview`。reader 須明確選擇資料根，預覽與正式版的 IndexedDB／Cache namespace 分開。

預覽 `data_version` 使用 §1 定義的 `preview-` 命名空間，不屬於正式發布版號；匯出只寫隔離根的 `snapshots/preview/current.json`，不改正式 active，也不提供永久分享碼、公開 URL 或回放 pin 的相容保證。`sve-publish upload` 可把預覽上傳到開發桶，在該桶的 `snapshots/versions/index.json` 以 `preview-` 版號寫入 current／previous entry，Entry 形狀與正式相同；這是開發 entry，不是正式發布。正式發布須重新建置並通過完整發布閘門，不能直接將預覽升為正式版。

建置參數、隔離檢查與前端接線見 [preview 建置與前端接線](preview-handoff.md)。

預覽仍須驗已啟用能力、JSON Schema、公開引用閉包、分片 join、hash／counts，並提供容量與排除清單、尚未通過的正式閘門報告。來源覆蓋不足維持未知語意；`source_windows` 只用 §8 的 complete／partial 或空窗口，不因集合為空就宣稱 absent 或合法。已知且適用的更正仍須套用；真正不相容的勘誤／來源更正衝突仍按既有閘門隔離受影響結果並列原因。純觀測表記差異／順序未定依 §2.3 顯示，不因沒有 current 排除整卡；診斷排除集合不是發布閘門。

## 5. 語言矩陣與取用

來源方向依 **2026-10-10 的日文唯一來源決定**（[#495](https://github.com/gbaian10/sve-kit/issues/495)）：
已確認同卡同面且來源有效即以 JP 翻譯，已知日英差異不改取 EN。
具體選用責任依[翻譯契約](../domains/translation-contract.md)；新版公開 basis／呈現與版本交 #496。有效 selection／直接 owner 引用通過來源、context、語言與 owner 檢查即可出貨，machine 或低信心不排除；只有實際 FieldTranslation 引用的譯文及原文閉包出貨，未選候選與內部清冊不出貨。

機器與非官方來源仍須清楚標示；繁中可用「非官方翻譯」、機器另標「機器翻譯・非官方」，審過不改 origin。來源標示與 low_confidence 的「待校對」各自獨立；新版 JP 依據的標示另由 #496 定義，不能用顯示狀態抵銷規則核對；不把未核對譯文改成 aligned，也不放行對戰自動能力。合法低信心選用仍計 translated，沒有選用才標缺譯並回原文；來源失效或錯誤引用不得用低信心替代拒絕。

卡面 region 決定卡圖/原文；UI 語言決定翻譯列。指定 printing 不被語言切換偷偷換圖。官方 counterpart 只用已人工確認 card/face 且完成語義核對、無相關 divergence 的版本；否則用該原文的 project/machine 譯文或原文回退。官英到齊且核對通過才自動優先，當前譯文依有效來源、參數及選用資料重建，不使用舊翻譯收據或審核狀態作發布過濾；繁中來源與段落選用依翻譯契約 §7.2，取完整 JP 效果且不按 EN ordinal 拼接。區域規則與官方 counterpart 的資格另依 §7.1。

| UI      | JP 卡面                                           | EN 卡面                             |
| ------- | ------------------------------------------------- | ----------------------------------- |
| zh-Hant | 日文＋繁中，缺翻譯留日文並標示                    | 英文＋繁中，缺翻譯留英文並標示      |
| `ja`    | 日文                                              | 英文＋適用官方日文/譯文，缺則留英文 |
| `en`    | 日文＋適用官方英文，否則已審譯文/機翻，缺則留日文 | 英文                                |

日/英 UI 不回退繁中。`language.fallback_order` 只供 vocabulary/keyword 等介面詞彙。原文該區也缺時可顯示另一已確認區原文但明說，不自造英版 printing。printed 模式只用該印刷文字版本的 FieldTranslation，不拿 current 譯文冒充印刷翻譯；derived 狀態必標推定，unknown 仍可讀 current。逐欄 origin/authority 顯示，卡名官方數位譯名不把整段繁中效果變官方。

## 6. 未實作頁消費契約

前端從 support＋快照清單 target 即可列出未寫/草稿/已審/引擎通過/載入拒絕，**不需下載 AST 或 load 報告**。版本顯示為「此快照驗證的引擎版本」，target null 則「尚未指定引擎」。region override/block 的取用順序見 §2；missing row、未知 status、target 不符均視為未知與手動，不能誤算支援。

建牌提示計不同 card 數＋實際張數，不重複計雙面或版次；與禁限獨立。沙盒執行期再次載入檢查，與 metadata 不一致時實際拒絕優先，該局手動並報診斷，不改寫公開快照。/cards/unimplemented 是保留路由；篩選 reason/status 走同一 canonical q。

## 7. 發布閘門與變動報告

除了建置資料庫完整性，卡表快照需驗：所有表與內嵌 reference 有公開目標；40 個文字集合＋3 個影像集合逐欄白名單；current/translation 選用/region blocks 完整；每卡 support 恰一列；所有非 passed 有原因；passed 的建置資料庫證據吻合；完整文字包/分片/啟動包語義等價；bytes/hash/count/依賴 closure 正確；無來源本機路徑、作者候選 YAML、逐列稽核 hash。

changes 是內容定址的公開摘要 `{format_version,from_data_version,to_data_version,added,modified,retired,errata,new_qa_versions,identity_changes,coverage_changes,support_changes}`，元素型別與複合主鍵表示見 [傳輸契約 §7](snapshot-transport.md#7-changes-元素)；建置待辦與原報告留建置資料庫。只是抓取時間/ETag 改變而內容相同不列修改。引擎版本/政策變化即使卡文不動仍發布新 `support/data_version`。

驗收反例：英文翻譯變更只改相關文字/owner、support 不憑翻譯改；sections/數值改則 source bundle stale；AST 改參數使舊考題失效；同題最新 fail 降級；同 hash 模板 ID 不准覆寫內容；有 img src 無 blob 可 unfetched；英文宣布但沒對照顯示 announced/unmapped；新未實作卡有文字與 `missing_dsl`，不阻止卡表先上線。

## 8. 投影的補充約束

`RegionView.mapping_state`：unmapped 沒有對照候選；pending 已有未採納候選；confirmed 與另一區所有面已核對歸同 card；`confirmed_none` 指在 `mapping_scope/mapping_as_of` 的已核查範圍確認無對應（不是永遠不會發行）。v1 不產 `not_applicable`，未知不能假造不適用。release 與 mapping 正交。RegionView.region 表示正在看的區域，mapping 是該區卡向另一區的對應，`confirmed_none` 的 `review.target_region` 必為另一區；查核日期/範圍隨 view 帶出。跨區正對應仍只靠 `printing.card_id`；`region_mapping_review` 只存 pending/無對應的證據，不能另建第二張對應表。EN-only 有已採納 `confirmed_none` 及新鮮 `en_override` 時可 `engine_passed`，不因不存在 JP 而加 block；其餘尚未核對仍 block。

art.regions 由實際 `printing_face→printing.region` 唯一推導，`[en]` 顯示「目前收錄僅英版插畫」，不宣稱永不出日版。BP18-SP01 與 BP18-SP01EN 雖去尾碼相同仍是不同卡；Vania 必經人工連到 ヴァンピィ 的 card/face。精確 EN 原卡號保留，不能拿猜測後綴當已確認證據。

`printing.int_id` 所有出貨列必填，發布後永久不改；依地區分段配發（[authored-layout.md](../domains/authored-layout.md) §3.2），但號段只是配號容量安排，地區一律讀 `printing.region`，不從號碼推算；解碼時一律查表取得版次，查無對應（含保留或未分配號段）即依下述未知 int 處理。`catalog_state/card_no_state/identity_state` 是獨立軸。`review_level=unreviewed/model_reviewed/sampled/confirmed`，由適用的 decision 推導，官方直抓未另審為 unreviewed，不把官方來源當人工確認。unlisted 才出已審可公開 `reference_urls`（至少一個）；official 為 []，一般來源圖網址另在影像清單。信心 `listing_confidence` 不等於 `review_level`。暫定號碼用 `/cards/_provisional/{int_id}`；官方 raw `card_no` 用 exact route；兩者改號 alias 分 namespace。非官方圖依 [build-db.md](../build/build-db.md) §17 採 `mirror_reviewed`，目前只產生官方圖：卡表快照 printing_image 的 `publication_state` 為 pending/approved，待確認不能出 variant/path；來源 URL 保留在 image_asset。建牌准入採 `regional_decklog`：依 printing.region 與 `decklog_available` 判斷，provisional 卡號/身分不擋加入、分享或匯出。verified 必有 `decklog_source_url` 與 `decklog_checked_on`；unverified 的日期為 null，available 依 `catalog_state=official` 預設 true，unlisted 預設 false，來源 URL 可空且 UI 明示未查證。建置資料庫的來源 FK 投影成可公開來源網址，不出完整稽核紀錄。Decklog 證據覆蓋預設，日英不得互相套用。這四欄屬 printing 頂層啟動包主儲存，詳情分片不重複，邏輯欄位白名單必填 Bool 與 verification，nullable 來源/日期依上述約束。

人工限量序號版次沿 [manual-printings-v1](../domains/manual-printings.md#5-重建freshness-與公開呈現)：未確認一般版對應者仍有 provisional card／face，印刷原文狀態 unknown，不以人工名稱造官方文字或 current。官方 PR（含 PR-442）照常顯示卡文。人工名稱／公開來源類別與查核日的欄位方案只在該契約列為待實作；本 PR 不改公開白名單或 printing 欄序，須後續機器契約審核與版本／能力協商再實作。

舊碼照常解碼，既有牌組照常開啟保留原 `int_id`/數量/區域/位置；目前 unavailable 顯示 `decklog_unavailable` 警告與可用同地區同名版次替換建議，不能自動換卡/丟行。所有區域禁止新加入 unavailable 版次；新的分享碼與任何牌組匯出拒絕整次操作，舊解碼及本機牌組開啟/編輯不受此拒絕。未知 int 顯示 `needs_update` 並保留，不能猜版次後輸出。反向恢復可用即可再加入/分享/匯出；其他牌組合法性另驗。永久 `int_id` 不因此移除。

卡表快照的 `printing_product.available_on/date_precision/date_raw` 分別映射建置資料庫的 `first_available_on/first_available_precision/first_available_raw`（都是覆寫值，不重複有效日期），有效日由例外否則 product 求得；`inclusion_kind` 與 note 直接投影。`product.date_precision=day/month/year/unknown`；day 才有 `released_on`，月年保持 `date_raw`；`printing_product.date_precision=null` 代表沒有例外、沿用 product 日期，unknown 表示確有例外但日期不明。不得用 1 號/1 月補日期或假稱初收錄。`inclusion_kind` 含 `qr_redemption`；`serial_total` 已知必 >0，null 是未查，不記持有人第 n 號。

`source_windows` 是 `[{kind:errata/qa/cardlist/cr,region,scope_key,from_date,until_date?,as_of,state:complete/partial,source_url}]`，由建置資料庫的 `source_coverage` 投影；`scope_key` 採 `region:*` 或 `product:<id>` 白名單。complete 只證明已抓完指定官方範圍截至 `as_of`，until=null 不涵蓋未來。印刷推導需期間涵蓋該 printing 可能勘誤期間，且有頁面未被回寫的核對，不因來源覆蓋存在就自動 verified。

`manifest.mechanic_universe_id` 是本次 keyword 的 canonical 集合（id、kind、`definition_unit_id`、actions）的 hash，含規則定義/動作變化；coverage.mechanics 為 `[{region,scope,total_cards,any_annotated_cards,fully_annotated_cards,unknown_cards,eligibility,by_keyword:[{keyword_id,complete_cards}]}]`。eligibility 固定 `all_non_retired_cards_in_region`（含 provisional，未可判者列 unknown），不以牌組政策偷偷改分母。`any_annotated_cards` 是有 fresh 標籤或 partial/complete 查核的去重卡數；`unknown_cards=total`−any；fully≤any≤total。`by_keyword` 不展開 card×keyword，最多 keyword 數筆。EN blocks 的卡不算 full，共用新鮮且可適用才算已查。

卡表快照的 `translation.source_unit_id` 由建置資料庫的 `translation.context_id→translation_context.source_unit_id` 投影；context/owner bindings 不出貨。不同 context 選出的 translation.id 可以指相同 `source_unit`，但 FieldTranslation 明確指定所用 translation，不全域依 `source_unit` 找唯一譯文。`stamp.series_code` 指 `vocabulary(kind=stamp_series)`；`search_alias.kind=stamp` 指單一 stamp.code，`kind=stamp_series` 指系列 vocabulary.code。

FieldTranslation 的逐 owner 來源檢查依[翻譯契約 §7.2](../domains/translation-contract.md#72-逐-owner-的顯示選用)。
JP 依據不要求 aligned、不偽造 EN 欄位擁有 JP 原文字串；官方 counterpart 仍須自己的 fresh 核對與適用資格。
新版 basis 與公開欄位由 #496 定義；這項建置驗證不出貨 dependency 表。

Spelling 與 RulingHint 的參數宣告、值域及拼法驗證依 [傳輸契約 §3.2](snapshot-transport.md#32-公開參數宣告)。`{Q}` 先登錄 literal、原樣文字顯示與複製，語意未查明前不賦予機制/引擎含義。文字 roundtrip 不以語意猜測為前提。

不再公開的圖片 variants 為空。舊快照不可變，已下載舊副本不保證立即移除；current 提交後按 §4.1 清理不再使用的卡圖 key；不因 previous metadata 引用而保留舊 WebP。`route_override` 僅作用於 official namespace，provisional 路由禁止覆寫。

**使用者 2026-10-01 核可（身分修復投影）**：identity_state=retired 的墓碑 card 與原 faces
只供 identity_change／歷史引用閉包，不進一般卡表、搜尋、卡包或插畫／繪師瀏覽；support 仍有 required
列但沒有自動能力。有效 printing_face 沒有引用的歷史 art 不進公開 art，對應 art_artist 不投影成 art.artists，
僅被排除 art 引用的 artist 不出貨；仍被現行 art 引用的 artist 保留。
公開 baseline 與其他引用依白名單留 null／省去相應列，不能指向被排除 art。
判定 uses 僅看此次地區投影的現行 printing_face，不把建置歷史或墓碑當現行用途。
同圖修復後的新 art 可展示，舊 art 留建置歷史；保留窗口內舊快照 JSON 不回寫，輪替後可回收，不保證舊圖片閉包。

### 卡片頁補充資料的離線投影

雙區離線候選 recipe 明確釘 JP／EN 卡片與更正證據圖片批次，重用同一套身分、文字及商品匯入，再加入卡片頁內 Q&A／相關卡；原 JP preview recipe 仍禁止未請求的補充資料。Q&A 依 `(region, official_number)` 分隔，去重保留所有卡片關聯及私有來源用途。卡片頁沒有 Q&A 或只有勘誤引用，不表示該類來源已完整；`source_windows` 與禁限覆蓋仍未知。

`Decisions.supplemental_restrictions` 由 `require_card_extras_ready` 的已驗 DB 結果提供；公開 `card_engine_support.region_blocks` 合併 `errata_current_pending`／`source_printing_missing` 與既有原因。尚未接入正式公告證據時另加 `errata_source_missing`；已接入公告但文字未核對則保留 `errata_current_pending`，兩者不表示封存庫是否已有 raw。限制以卡片／區域阻止自動操作，即使仍有舊 current 也成立；不刪卡、面、路由、已採納版次或預設候選。受影響面與精確來源缺口在私有報告，不公開 issue／decision／來源 hash。

勘誤巢狀 `versions[].changes[]` 的 `before`／`after` 是公告提供的逐欄位變更片段，消費端須明示是片段，不補全文或還原舊版次印刷文字。保留同一卡全部已知公告；`announced_on` 為 null 時省略日期，不能拿抓取／生效時間代填。只有引用時保留能力限制，不造空公告，也不將其列入正式 `errata_card_ids`。

逐區 reskin 在實際來源、current 與人工證據入庫後重新計算，透過 `Decisions.related_regions` 發布。只有一區成立時只公開該區關係，不繼承 DSL 或建牌身分。本離線入口只輸出勘誤待核對限制，沒有確認或解除接點；確認契約與核對結果另行接入，不建立新的 current 採納。

### Standard 構築資料與固定引用的實作邊界

[construction-adoption-v1](../domains/construction-adoption.md) 定義首批 JP／EN Standard 的來源、兩模型核對採納與後續必要 CR 引用，不承諾整副牌合法性。首發先上禁限資料，CR 條文引用等 #48；其間 cr_version_id／construction_rules_ref 可為 null，固定 ref 尚未兌現，介面如實標示。已知限制可查，partial／未知日期或 evaluator 未支援 ref 時仍 unknown；不因入口沒公告連結而推無禁限。CR context_key、profile revision／restriction source_urls、coverage as_of、config.construction_refs 只在該契約列為待實作方案，本次不改公開表格／欄序。後續須與機器 Schema／types／reader、格式版本／capability 協商同步，才可宣稱出貨支援。

## 9. format 2.0.0 同名規則瀏覽

[數位名字政策](../domains/digital-name-policy.md) 的 same_name 卡層瀏覽使用
[傳輸契約 §5.4](snapshot-transport.md#54-format-200-卡包-media-與-id-圖片) 的 2.0 配置。
公開欄序與引用閉包依 §2，relation 白名單、typed projector、Schema 與 reader 同步驗證。
不另設中間格式或相容 reader。

same_name 的公開欄位仍依 §2 的 digital_link 欄序：
`id, card_id, face_id, digital_card_id, digital_phase, relation, effect_similarity, review_level`。
face_id／digital_phase 皆 null、effect_similarity=null、review_level=unreviewed；
它是一張實體卡到一個數位 game／official_id 的卡層連結，不展開面×phase。
reader 必須拒絕 same_name 帶非 null 面／phase、效果相似度或非 unreviewed 的列。
規則連結沒有真人決定，不能經全域 Source.review 投成人工確認；其他 relation 的 review 映射不變。
畫面標「同名規則視為同卡，未逐筆確認」，不能拿它供官方取名、同概念、效果等義、DSL、art 或 voice。
不增加 policy_checked、政策／收據欄位或新的公開 review_level。

所有符合政策的數位 ID 兩代全列，每個 `(card_id,game,official_id)` 的規則連結最多一筆，
wire 依既有 link.id 排序；UI 先 sv1 再 svwb，各代按原始 ASCII official_id 排序。
id 仍為 `dl:`＋H(`["digital-link-v1",subject]`)，subject 依 digital-link-adoption 的完整卡層鍵；
relation、時間、政策／排除清單 hash 不參與。政策卡層升真人且 subject 完全相同時沿 ID，
精確面的人審 subject 是另一 ID。有效真人同 card/game/ID 的任何合法關係優先，規則不得另出重複列；
刪除真人關係不會自動停用規則連結；要停用某卡對須在 links 政策的 excluded_targets 列出。

names 與 links 政策／排除互相獨立。被排除的名字不能供規則連結證據；
同一卡對仍有另一個未排除名字的有效證據時可保留，excluded_targets 則移除整個規則卡對。
此處指 links 的名字排除；names 的取名排除不撤真人連結，也不自動代簽 links 排除。
排除與有效真人關係重疊只報告，真人按自己的 review_level 顯示。

建置 digital_endpoint 不是新公開表：投影成既有 config.digital_endpoints，固定 sv1／svwb 兩列，
欄位、排序、模板與語言對照依傳輸契約 §3／§5.4；不連網就保持 status=unknown。
只公開有效關係所需的 digital_card 與必要引用閉包，不把完整凍結目錄全庫出貨；
digital_face／digital_text 留建置端，沒有資產採納就不出 digital art／voice。
純名字政策仍只用既有 translation／FieldTranslation；連結是否出貨依獨立能力。
首批不採納 coverage，digital_link_coverage=[] 是未知，不表示查無或真人查完。

| 驗收反例 | 結果 |
| --- | --- |
| 未知格式，或 2.0 缺 capability／最低版本不足 | 拒絕整份快照，不能丟掉列後降版 |
| 2.0 零配對便省 capability，或 config／fragment／programs／text_all 仍寫舊版本 | 拒絕，不因空列放寬准入 |
| same_name 帶面／phase、effect_similarity、sampled／confirmed review | producer 與 reader 各自拒絕 |
| same_name 因不同面或 phase 重複出列，或同組真人與規則並存 | 拒絕重複規則列；建置端保留全部匹配來源，真人精確面的合法多筆沿自己的 subject |
| digital_card／link／endpoint 引用缺目標，或把凍結目錄全部當必要閉包 | 拒絕缺引用；只投影實際有效關係所需內容 |
| 同名連結被拿來授官方譯名、概念、圖或語音，或空 coverage 被說成無對應 | 不授權；各入口仍驗自己的採納／來源條件，未知如實呈現 |

正式容量與變動報告依 2.0 固定配置及既有預算量測；超出上限須交維護者決定，
不自動改 N／格式。保留窗口依 §4.1，不永久保存歷史快照。
