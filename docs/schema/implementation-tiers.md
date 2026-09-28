# 建置表實作分期

這是邏輯契約的實作次序，不是要求第一次發布就完成 121 表，也不是整張表隨意缺失時繼續執行 FK。[build-db.md](build-db.md) §18 的能力/依賴閉包規則適用；一次只實作受支援的子集與其匯入器/驗證器，編譯該階段 DDL 時不產尚未啟用的 nullable FK 欄位約束，待啟用即用 migration 補齊約束與驗證全部現有列。公開 schema required 欄位仍完整，從未啟用能力輸出明定 null/空/unknown；不能改成任意省略。

首發實際集合＝T0 全部 40 表＋下列 T1 子組 17 表，共 57 表；不是只有文字查卡集合就宣稱建牌可用。`identity_change` 在**第一次公開 `int_id` 之前**啟用，沒有修復事件時可以空，但 schema/匯入/驗證不能延後。逐地區版次的 Decklog 可用性欄位屬 printing（T0）；暫定身分不擋建牌。未查證按官方卡表收錄預設，首發不假造 Decklog 查證。

| 首發 T1 子組       | 必須啟用的建置表                                                                                                                   |
| ------------------ | ---------------------------------------------------------------------------------------------------------------------------------- |
| 圖像               | `image_asset`、`printing_image`、`image_variant`、`image_size`                                                                     |
| 勘誤與已知來源更正 | errata、`errata_version`、`errata_change`、`errata_printing`、`source_correction`、`correction_evidence`、`correction_application` |
| Q&A                | qa、`qa_version`、`qa_card`                                                                                                        |
| 關聯               | `card_related`                                                                                                                     |
| 構築引用的 CR      | `cr_version`、`cr_clause`                                                                                                          |

T0 的構築子組為 `rules_profile`、`rules_profile_revision`、restriction、`restriction_member`、`restriction_coverage`、`deck_role_override`、`identity_change`。`rules_name/face_rules_name` 原已在 T0。首發有已知規則/禁限須填入，覆蓋不足仍明示 unknown，不能捏造合法；`construction_rules_ref` 與非空 FK 的依賴一併提供。首發 `card_related` 支援已確認相關卡與追加區建議；未知 relation 不猜；有已核對的換皮資料時，`same_rules_reskin` 依 build-db §5 的採納與失效規則投影。EN/非官方 SNC/翻譯/裁定等若納入該次資料，再啟用對應條件組及完整非空引用閉包；57 是明列最低集合，不是略過實際資料依賴的上限。

若首發納入現有 EN 登錄，另啟用 `region_mapping_review`、`art`、`region_text_review`、`region_divergence`，最低集合成為 61 表；61 是含 EN 條件組的結果，不是所有發布無條件必建的數量，其他非空引用仍須補齊閉包。

首發不啟用 `face_semantics`／`revision_semantics`／`semantic_reference`；身分分組不視為規則等義證明。`astra/1` 原型不符合 [正式候選入口](authored-layout.md#7-dsl-與拒絕輸入) 與 DSL 1.0 Schema，不構成正式 DSL 候選；沒有正式候選的卡依 [build-db.md §10](build-db.md#10-dsl驗證與未實作卡片頁) 輸出 `missing_dsl`，`dsl_id/dsl_version` 為 null、`automatic=false`；未指定引擎時引擎目標為 null；公開 automatic 仍由支援狀態推導，不新增傳輸欄位。預覽快照通過其已啟用能力的驗證，不代表正式首發 57 表已驗收。

| `tier` | 用途                               | 啟用/未啟用行為                                                                                                                                                                          | 表數 |
| ------ | ---------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---- |
| T0     | 查卡與構築共同必要集合             | 必做；缺 DSL 仍輸出 `missing_dsl`。建置資料庫的 `card_engine_support` 未啟用引擎時 dsl/load/engine 欄為 null；`printing_face.art_id=null`、加工未知、printed unknown，頁面仍能顯示原文。 | 40   |
| T1     | 來源/策展資料到齊才啟用            | 有該類來源即在該批發布前完成；SNC 需要 references、非官方 metadata；EN 需要 mapping review/語義核對；已知更正/勘誤不能因 tier 而丟掉。                                                   | 30   |
| T2     | 翻譯、等義歸納、數位、裁定有資料時 | 首個已採納等義再錄就先實作 semantics 子組，首個翻譯就做 context/template 引用閉包；數位/語音/裁定可獨立啟用，未上線則 unknown/無提示，不假造翻譯。                                       | 34   |
| T3     | 引擎/機制能力啟用前                | 首個 manual mechanism 可先做 keyword/action/coverage/projection 及其依賴；DSL/巨集/實跑子組有資料才做。沒實跑永不輸出 `engine_passed`，未檢查機制仍 unknown。                            | 17   |

## 逐表分配（每張恰一次）

| 表                             | tier |
| ------------------------------ | ---- |
| `source_record`                | T0   |
| `decision`                     | T0   |
| `decision_source`              | T0   |
| `language`                     | T0   |
| `vocabulary`                   | T0   |
| `card`                         | T0   |
| `face`                         | T0   |
| `identity_change`              | T0   |
| `card_int_id`                  | T0   |
| `rules_name`                   | T0   |
| `face_rules_name`              | T0   |
| `region_mapping_review`        | T1   |
| `product_family`               | T0   |
| `product`                      | T0   |
| `printing`                     | T0   |
| `printing_product`             | T0   |
| `printing_reference`           | T1   |
| `region_availability_override` | T1   |
| `artist`                       | T1   |
| `art`                          | T1   |
| `face_art_baseline`            | T1   |
| `art_artist`                   | T1   |
| `printing_face`                | T0   |
| `stamp`                        | T1   |
| `printing_stamp`               | T1   |
| `text_unit`                    | T0   |
| `face_revision`                | T0   |
| `face_text_section`            | T0   |
| `printing_text_section`        | T0   |
| `face_trait`                   | T0   |
| `face_title`                   | T0   |
| `face_special_kind`            | T0   |
| `face_current`                 | T0   |
| `printing_face_observation`    | T0   |
| `face_semantics`               | T2   |
| `revision_semantics`           | T2   |
| `semantic_reference`           | T2   |
| `source_coverage`              | T0   |
| `errata`                       | T1   |
| `errata_version`               | T1   |
| `errata_change`                | T1   |
| `errata_printing`              | T1   |
| `card_related`                 | T1   |
| `region_divergence`            | T1   |
| `region_text_review`           | T1   |
| `qa`                           | T1   |
| `qa_version`                   | T1   |
| `qa_card`                      | T1   |
| `cr_version`                   | T1   |
| `cr_clause`                    | T1   |
| `ruling`                       | T2   |
| `ruling_revision`              | T2   |
| `ruling_evidence`              | T2   |
| `ruling_card`                  | T2   |
| `ruling_template`              | T2   |
| `ruling_hint`                  | T2   |
| `ruling_supersession`          | T2   |
| `ruling_review`                | T2   |
| `rules_profile`                | T0   |
| `rules_profile_revision`       | T0   |
| `restriction`                  | T0   |
| `restriction_member`           | T0   |
| `restriction_event`            | T1   |
| `restriction_coverage`         | T0   |
| `deck_role_override`           | T0   |
| `digital_card`                 | T2   |
| `digital_face`                 | T2   |
| `digital_text`                 | T2   |
| `digital_art`                  | T2   |
| `digital_art_source`           | T2   |
| `digital_link`                 | T2   |
| `digital_art_link`             | T2   |
| `digital_link_coverage`        | T2   |
| `digital_endpoint`             | T2   |
| `voice`                        | T2   |
| `card_voice`                   | T2   |
| `glossary_term`                | T2   |
| `glossary_translation`         | T2   |
| `sentence_template`            | T2   |
| `template_translation`         | T2   |
| `template_component`           | T2   |
| `text_template_binding`        | T2   |
| `translation`                  | T2   |
| `translation_binding`          | T2   |
| `translation_term`             | T2   |
| `translation_selection`        | T2   |
| `translation_context`          | T2   |
| `translation_use`              | T2   |
| `dsl_document`                 | T3   |
| `dsl_source`                   | T3   |
| `dsl_review`                   | T3   |
| `dsl_exam`                     | T3   |
| `dsl_load`                     | T3   |
| `card_engine_support`          | T0   |
| `dsl_ruling`                   | T3   |
| `dsl_qa`                       | T3   |
| `author_macro`                 | T3   |
| `macro_review`                 | T3   |
| `dsl_macro_use`                | T3   |
| `keyword`                      | T3   |
| `mechanic_action`              | T3   |
| `card_mechanic`                | T3   |
| `mechanic_coverage`            | T3   |
| `mechanic_override`            | T3   |
| `mechanic_projection`          | T3   |
| `card_mechanic_coverage`       | T3   |
| `image_asset`                  | T1   |
| `printing_image`               | T1   |
| `image_variant`                | T1   |
| `image_size`                   | T1   |
| `shop_link_template`           | T1   |
| `build_issue`                  | T0   |
| `source_correction`            | T1   |
| `correction_evidence`          | T1   |
| `correction_application`       | T1   |
| `text_symbol`                  | T0   |
| `card_route`                   | T0   |
| `card_route_alias`             | T0   |
| `route_override`               | T0   |
| `default_printing_override`    | T0   |
| `search_alias`                 | T0   |

未建表的後續範圍：`event/printing_event`、`signature/printing_signature`、`artist_link/art_post`、`query_alias`、Decklog adapter。已有建置表但沒有資料的 conditional 組不需先造空的公開資料集內容；公開列集合可以空、相關 coverage 必須 unknown/partial。本表與 build-db 的表集合必須完全一致（每表恰一次），由驗證工具核對，不靠手寫表數維持一致。
