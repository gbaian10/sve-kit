# authored 維護方式

`authored/` 的檔案配置與匯入規則。下列路徑是提案；ID、決定和配號皆為結構示例，不能當人工審核證據。規則以 [build-db.md](build-db.md) 為準。

## 1. 人寫例外與永久登錄分開

| 類別              | 路徑提案                                                                                                         | 維護方式                                                                                              |
| ----------------- | ---------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| 永久身分/面/版次  | registry/identities/BP01.yaml、registry/identities/PR/001.yaml                                                   | 工具產候選，採納後固定 ID；不是每次重新分組                                                           |
| printing UInt32   | ids/index.yaml（include）、ids/BP01.yaml                                                                         | 工具全域唯一配號，只能新增；不能靠排序重建                                                            |
| 模板/詞彙永久 key | templates/BP01/001.yaml、keywords.yaml、`vocabulary/_shared/001.yaml`                                            | 內容 ID/穩定 code；工具檢碰撞，舊 ID 不覆寫                                                           |
| 身分修復/特殊構築 | overrides/identities/BP01.yaml、overrides/deck-roles/BP01.yaml                                                   | merge/split/reassign 與例外角色需人工                                                                 |
| JP/EN 對應        | registry/identities/BP01.yaml 的 EN printing 歸屬                                                                | 每筆人工確認；不另寫 `region_mapping` 真值                                                            |
| 新包策展          | curation/BP01/001.yaml                                                                                           | 一包封套包含 art/stamps/digital/serials/related；按 kind 分段，避免到處開空檔                         |
| 來源錯誤/語義差異 | corrections/BP01.yaml、divergences/BP01.yaml                                                                     | confirmed 例外，含來源版本與原因                                                                      |
| 翻譯              | translations/zh-Hant/BP01.yaml、translation-templates/zh-Hant/BP01/001.yaml、`glossary/zh-Hant/_shared/001.yaml` | 全句/子句模板，繁中跟 JP；EN 只掛適用選用                                                             |
| DSL/巨集          | effects/BP01.yaml、effects/index.yaml、macros/BP01/001.yaml                                                      | 工具填 meta；EN 只有 divergence/EN-only 才加 override                                                 |
| 裁定/機制/禁限    | rulings/R-0001.yaml（每裁定一檔，ADR-0011）、mechanics/BP01.yaml、`rules/_shared/001.yaml`                       | 人工證據/覆寫，推導 projection 不寫 authored；裁定匯入時 Q 號轉 `qa_version_id`、`zh-TW` 轉 `zh-Hant` |
| 路由/預設例外     | overrides/routes.yaml、overrides/defaults/BP01.yaml                                                              | 正常 route/default 不人寫，改號 alias/多 variant 入口才登錄                                           |
| 搜尋/記號/設定    | aliases/zh-Hant/BP01.yaml、`symbols/_shared/001.yaml`、`config/*.yaml`                                           | 一張 `search_alias`；圖示 SVG 屬 app shell                                                            |

authored 是人工判斷或不能重建的永久狀態。官方原文/QA/圖像、可重建 `face_current/route/projection` 不整份複製入 git。檔案按首次 `owner/home_set` 固定；共用模板/裁定放首次定義包或 `_shared`；PR 大檔依固定序號 bucket 切。每檔 <1,048,576 bytes，建議 512 KiB 分片；include 亦可分層。安全 YAML 解析、禁止重複鍵/tag/跨檔 anchor，日期與 ID 引號明示。

## 2. 批次封套與確認

```yaml
authored_format: 1
kind: curation_batch
owner: BP01
default_decision_id: example-batch
records:
  - record_key: art-group-example
    kind: art_group
    art_id: "a:example"
    card_id: "c:example"
    face_id: "f:example:front"
    classification: alternate
    uses:
      - printing_id: "p:jp:example"
    decision_id: null
decisions:
  - id: example-batch
    category: art
    state: proposed
    scope: batch
    policy_id: art-review-v1
    membership_hash: "sha256:<工具計算精確成員與內容>"
    sample_ids: []
    authored_by: example-author
    authored_at: "2026-09-27T00:00:00Z"
    reviewed_by: null
    reviewed_at: null
    note: "尚未抽查；此例不能發布為採納資料"
```

`decision_id=null` 表示匯入時沿封套；SQLite 必須展開為實際 FK。`membership_hash` 由記錄 canonical 語義內容（排除 decision 指針）與 key 排序計算。新增/修改記錄不沿舊 sampled；工具產下一批 decision，舊決定不可覆寫。sampled 必含實際抽查的 record keys、審核者/時間、抽樣政策和失敗例外；confidence `high/model_reviewed` 均不足以代替人。

art/stamp/digital/serial/翻譯可 sampled；UI 顯示抽查覆蓋。跨區身分仍 confirmed＋每筆 checked，不因採用同封套就抽查取代全筆人工。

## 3. 身分、跨區與配號

```yaml
authored_format: 1
kind: identity_registry
records:
  - card_id: "c:example"
    home_set_id: BP02
    identity_state: provisional
    faces:
      - { id: "f:example:front", ordinal: 0, side: front }
    printings:
      - id: "p:jp:example"
        region: jp
        card_no: "EXAMPLE-JP"
        variant_key: standard
        source_face_map:
          - { source_index: 0, face_id: "f:example:front" }
      - id: "p:en:example"
        region: en
        card_no: "EXAMPLE-EN"
        variant_key: standard
        cross_region_review:
          state: proposed
          checked: false
          decision_id: example-region-review
        source_face_map:
          - { source_index: 0, face_id: "f:example:front" }
```

日英身分只存這裡，EN `source_face_map` 也是同一確認的一部分；`cross_region_review` 是 authored 輸入，匯入後 decision/source 與 `printing.card_id` 表達，不出貨第二張 `region_mapping`。

JP 初始化工具以全部面完整特徵產候選，對既有 registry 優先保持 ID；新勘誤造成效果不同不能自動拆卡，依同卡通則歸併 EP/SEP、CP03-125/126、ルゥ 與經核對等義表記，真歧義才隔離。普通單面 `source_face_map` 可由工具生成；雙面依來源順序先配候選但覆核完整面後才採納，不拿 ordinal 當跨區推斷。

```yaml
authored_format: 1
kind: card_int_registry_shard
records:
  - int_id: 1
    printing_id: "p:jp:example"
    region: jp
    card_no: "EXAMPLE-JP"
    variant_key: standard
    allocated_at: "2026-09-27"
```

沒有 `decision_id`。配號工具鎖全域 next-id，append 後驗重複；與前次公開 registry 比較，不能更改/刪除/重用。主入口 ids/index.yaml 只 include shards。現行 `authored/README.md` 的 `card-ids.yaml`（卡片 ID、各區卡號與跨區對應）落地時改由 registry/identities 與 ids/ 承接，並同步更新該 README。未發表草稿號不當正式分配。

`identity_change` 另寫 `old/new/kind/printing?/data_version/decision/reason`。split 多目的與受影響 printing 清單完整；永久 int→printing 不變，父 card 修復要可見，不靜默改玩家牌組。

## 4. 新卡包的人工作業量

確定性項目全自動：官方來源欄位、`face_current` 無衝突預設、`rules_name`、`deck_role`、route、default printing、int 配號、模板套用、projection/coverage 報告。

人處理：新句型/語義衝突、JP 身分歧義、數位/異畫/標誌批次抽查、新卡名/譯文抽查、EN 身分逐筆確認、必要裁定與手動 override。正常 JP 包不要求為每張卡寫 current/default/route/decision 四份檔；封套＋工具結果可一次審閱。

度量不是把人工語義壓到固定數量：每次報 `generated_rows`、`explicit_overrides`、`sampled_rows`、`individually_checked_rows`、新句型數、人工作業時間。200 個版次的普通 JP 包，以「0 筆手寫 route/current/default/int decision、策展按包批次、只有例外覆寫」為驗收。可把 `explicit_overrides`≤新卡數當觀察目標，超出要找自動化缺口，但不可因此略掉有必要的確認。EN 對應的全筆檢查單獨計，不能用 JP 指標減掉。

## 5. 插畫/數位/標誌匯入順序

來源頁`→identity/face→art_group` 採納`→printing_face.art_id→digital_art_link`。svwb-art 的 printing+face 候選原樣保留 staging；art 沒採納前不 materialize 正式 link，也不配假 art。批次原樣高 confidence 不自動 sampled。

每個 face 的 base art 明示；alternate 需有 base 和不同圖證據。signed 留面級 Bool?；序號在 `printing.serial_total`；stamp 留原字與顯示年，不生造 event、日期或名次。`credit_raw` 由版次頁保存於建置資料庫的 `printing_face`，不併到 `art_artist`。

## 6. 模板、翻譯與語義例外

`sentence_template` 一個 ID 就是一份不可變內容；既有 prefix+10hex ID 保留、碰撞檢查必做。完整內容 hash 包含 `normalizer_version/parameter_schema/semantic_variant`；變更新增 ID＋supersedes，不設 `template_revision/current` 指標。模板翻譯自身仍可有不可變 revision，不是禁止翻譯修字。

EN 身分確認且文字對照完成，無 divergence 時自動選官方英文、共用 JP 繁中與 DSL。例外格式：

```yaml
authored_format: 1
kind: region_divergence
records:
  - card_id: "c:example"
    region: en
    field_scope: rules
    reason: "英文仍是舊語義，待核對官方更正"
    effect: manual
    override_dsl_id: null
    resolved: false
    decision_id: example-divergence-review
    evidence: []
```

這是例子而非可發布 confirmed 事實。翻譯 origin/authority 分開，效果永遠 unofficial；繁中來源跟 JP，適用 EN 的判斷由建置輸出 FieldTranslation，不在瀏覽器猜。

## 7. DSL 與拒絕輸入

meta 保留 DSL 版本、rule-bundle-v2 source hash（原觀測 face-bundle-v1 另留追溯，遷移需重驗）、`written_by/reviews/status`、`verified_by_exam`、ruling IDs、QA IDs、macro 用途。審卡程式自動填，不要人工複製逐卡 hash。shared 是預設，EN exception 才 `scope=en_override`；DSL body 僅依 `dsl/` 真正 schema，不在此造示意 op。

effects/index.yaml 提供 `card_id`＋scope＋file/record key；即使候選 YAML 解析失敗仍可定位卡。candidate hash 取 exact authored bytes，`load_report` 在建置層追加 accepted/rejected＋目標版本。valid AST 才進 `dsl_document`；無效候選不進公開附件，support 仍能列 `rejected_yaml`。

題本/載入結果與 program/source/engine build/policy/測試輸入版本匹配，最新 fail 不能沿用舊 pass。引擎未指定時不冒充 `engine_passed`；公開只投影 support 和 `program_ref`，private raw 報告不出貨。巨集作者/兩模型審查/機械三檢查留建置資料庫，出貨 AST 已展開。

## 8. 更正、路由與設定

`source_correction` 原值/改值/字段/來源 hash/evidence/核對者日期/是否回報完整保存。套用前先比是否已上游修正；不符合 expected/hash 則停用重審。卡表快照只出引用者上的 `corrected_from`/標記/公開理由，不能在去重 text 上全域標更正。

route/default 純推導；authored 只寫 alias、variant `route_override`、`default_printing_override`。canonical 編碼 exact 原卡號，folded 輸入只在唯一時轉址。UI `fallback_order` 只用介面詞彙，卡文保留所選區原文。

所有 config/template URL 限 HTTPS＋具名參數白名單。`language_map` 可擴充，不新增 SVE region。固定圖示 code 指 app shell 自製 SVG；卡片影像清單沒 `card_back` role。

## 9. 批次表記、上下文與非官方條目

wording diff 封套可一次簽 confirmed，但 `sample_ids` 必須列全部 checked 成員；不能用抽樣認定整批全都等義。scope=record 的三個 batch 欄位皆 null。初始 exact 原文/無差異採機械路徑，省略提醒或共用語義才檢正規化政策。保留 `printing_face_observation` 與 `revision_semantics`，最新表記改顯示、等義 bundle 保持；真規則或 token 依賴改動才重驗 DSL。對同一頁不同時間的更新也先分觀測，不一律當互斥衝突。

`translation_context` 預設 `semantic_variant=default`；只有採納的同字異義例外才能另配 variant。`translation_use` 釘具體 owner/field/ordinal，`translation_selection` 依 `context/target_lang` 選同模板同參數唯一翻法。不是每張卡任意自由翻；模板/術語更新仍沿 binding 反查。建置資料庫的上下文關係不出貨，卡表快照的 FieldTranslation 指已選 translation.id。

SNC 另用 `manual-printings/SNC/001.yaml` 路徑提案，仍受單檔 <1 MiB；匯入 snc-list 只產候選，不把 high 當 confirmed。最小封套欄位為 `printing_id/card_id/region/card_no/card_no_state/catalog_state/listing_confidence/serial_total`、references（url/role/locator）、inclusions（`product_id/inclusion_kind/date_precision/date_raw/note`）、decision。`normal_counterparts` 全筆確認後才連同 card；無對應可登 `region_mapping_review` 的 `confirmed_none`＋查核範圍/`as_of`。

例如 BP20-SNC01（ANV，4 周年，初版限定 n/10）的 10 可寫 `serial_total` 候選；卡號是否真的印於卡面仍依來源核對，不能因本文件提到就改 official。PR-350/PR-442 上限 150、PR-544 上限 500 為已知的維護需求，仍留下原證據/欄位來源。月年日期原樣保存，不補完整日期；QR 兌換與初版限定用 `inclusion_kind` 區分。unlisted 公開頁有「非官方整理，可能不完整」、來源/信心/回報入口。

暫定 `card_no` 不占官方網址；`int_id` 所有出貨 printing 都追加分配。補正 `card_no` 後留下 provisional→official 永久 alias；`int_id` 不變。卡號推算與 card 身分是不同軸，同卡通則不會讓所有 SNC 或 EN 候選自動 confirmed。authored/config 已固定 `third_party_image_policy=mirror_reviewed`、`deck_eligibility_policy=regional_decklog`。每張第三方圖以 `review_decision_id` 連到 confirmed 的來源/圖片確認，保存 `source_url`、內容 hash、確認者 `reviewed_by` 與時間 `reviewed_at`；換圖/換來源須重新確認，抽樣不代替逐圖確認。建牌資格依該地區/版次的 `decklog_available`；人工查證記來源與日期，未查證依官方卡表收錄狀態預設（詳 [build-db.md](build-db.md) §17.2）。暫定號/身分不阻擋建牌；不可用版次禁止新加入、新分享碼與匯出。舊碼/既有牌組仍開啟保留條目，警告並提示可用同名版次，不靜默刪除。

發布程序另外追加永久版本索引及內容閉包；所有舊 text 鍵集合用來做固定 16 hex＋lang 的碰撞檢查，無碰撞才可追加，不能重配歷史鍵。這個可重建鍵索引不進人工 registry，也不刪 R2 歷史來省索引工作。
