# 公開邏輯投影

`project(db, regions=..., as_of=..., settings=..., decisions=...)` 讀已驗證的建置 DB，
回傳 `Projection.tables/config/metadata`。`tables` 恰有 40 個文字集合與 3 個影像集合，
即使未啟用也保留空陣列。這是 join 後的公開邏輯物件，不是分片、tuple 快照或發布器；
輸入批次／採納／freshness 的 domain 驗證仍由建置器負責。

```python
from sve_carddb.snapshot.project import Decisions, Settings, project

result = project(
    database,
    regions=("jp",),
    as_of="2026-10-01",
    settings=Settings(
        catalog_feedback_url="https://example.invalid/feedback",
        grammar_version="card-query-v1",
        normalizer_version="card-normalizer-v1",
    ),
    decisions=Decisions(),
)
```

## 白名單與 null 邊界

`records.SCALARS` 逐集合列出同名直投的每個 scalar 欄；`source.Source` 僅經
`Database.select` 讀明列欄位。未列建置欄位不會帶出，包括作者、raw locator、來源 ID、
逐列 hash、載入報告與 template dependencies。所有公開物件及內嵌欄位再經固定的
Schema descriptor 驗完整欄序、nullable、enum 與額外鍵，不以 SQL 表名當出貨宣告。
引用閉包與 metadata 完成後再共用 A 的跨列語意檢查（詞彙、摘要、nested、符號／提示參數）；
分片 owner 的驗證留給匯出階段。

| 公開欄位 | 來源／投影規則 | 空、未知與未啟用能力 |
| --- | --- | --- |
| `card.faces`、`face.current` | `face` 的永久 ordinal、`face_current`；current 僅含此次地區的公開 revision | 退役墓碑可缺現行面；缺 current 不偽造 revision |
| `card.regions` | 同卡版次、`region_availability_override`、`region_mapping_review`、日期、預設版次與角色 | 兩區各一項；無發行證據為 unknown；mapping/release 獨立；nullable 日期／預設／角色不補值 |
| `printing.review_level/reference_urls/int_id/decklog_source_url` | registry 版次皆為人工確認身分，固定 `confirmed`；`printing_reference→source_record.url`、永久配號、Decklog 的 source URL | 官方 references=[]；unlisted 仍須 Schema 的非空公開 URL；所有版次必有配號；來源／查核日依 verified/unverified 約束 |
| `printing.faces` | `printing_face` 加 section、stamp、觀測、翻譯、更正；按永久 face ordinal | 未知印刷原文維持 null/unknown；觀測不填回 printed 欄 |
| `printing_product.available_on/date_precision/date_raw` | 三個 `first_available_*` 覆寫欄 | null precision 沿 product；unknown 明示未知；month/year 不補一日 |
| `printing_product.first_inclusion_state`、debut | 同卡同區全部 inclusion 的有效日期 | 任一可能更早日期不明就 unknown，不把空 inclusion 宣稱 first |
| 預設版次 | 直接使用 `routes.defaults.select_defaults` 的版次 ID／method；分類證據承接 `GeneralEvidence` | 依路由採納契約驗 override、home、一般版加工、競爭版次與日期；本投影不另寫選取算法 |
| `card.regions.deck_role` | confirmed override，否則依同卡同區 current 的已識別 type／special kind 推導 | 未確認 override 不採用；未知代碼、沒有 current 或多面角色衝突為 null |
| `art.review_level/regions/artists`、`artist` | registry 插畫固定 `confirmed`；此次地區的現行 `printing_face` 使用關係、`art_artist` | 無現行用途的舊 art 與只被舊 art 引用的 artist 不出貨；被排除 art 的 nullable 引用留 null |
| `traits/titles/special_kinds/sections` | `face_trait/title/special_kind/text_section`、`printing_text_section` | ID/code 集合穩定排序；段落保留 ordinal；未有資料為 [] |
| `translation.source_unit_id/text_unit_id`、各 owner 的 translations | `translation_use/context/selection` 的精確 owner/field/ordinal，chosen translation 的原文與譯文 | 未選、未 reviewed 不出；同 source 的不同 context 不合併；缺譯留原文 |
| `qa.current_version_id`、`qa_version.cards` | 同 QA 最高 revision、`qa_card` | 無版本 null；原版本歷史保留；QA/errata 稀疏摘要空不代表 absent |
| `errata.versions` | `errata_version/change/printing` | 可未知公告／生效日；before/after 保持受限 JSON |
| `ruling_revision` 的 scopes/evidence/cards/hints | 建置器提供的有效 scope、supersession、精確 QA/CR 引文／source URL、ruling_card/hint | 不推測文字切段；沒有明確有效 scope 或 undecided 不提供提示；無 active scope 接點直接拒絕 |
| `rules_profile.revisions/restriction.members` | profile revision、restriction member | 未有資料 []；不從沒有 restriction 宣稱牌組合法 |
| `card_related.applicable_regions` | 已驗證的 reskin 地區投影 | reskin 缺 eligibility 不出列；其他 relation=null，不推 DSL 或同名張數 |
| `digital_art.digital_card_id/phase`、link/voice 的 phase | `digital_face` 的 parent/phase | nullable phase 保留 null；數位資訊只留被實體關聯、圖、語音引用的閉包 |
| `keyword.name_unit_id/actions` | 同概念 `glossary_term.source_ja` 與 mechanic_action | 明列 glossary 概念；符號不推測機制；無能力集合為 [] |
| `mechanic_projection/card_mechanic_coverage` | 建置端新鮮投影直投，驗最短 include/exclude 編碼 | 缺 coverage 為未知；partial 不證明 absent；EN block 不計 full |
| `image_asset.format`、影像 variants | source mime 的 format、已核可且 available 的公開衍生檔 | null mime→null format；pending/withdrawn 無 variants/path，保留來源及撤下原因；不輸出原 PNG blob |
| config | language、digital_endpoint、shop_link_template、固定五檔 image sizes、明示 search/feedback 設定 | 缺 optional 能力陣列為 []；固定政策與大小仍完整；normalizer 不一致拒絕 |
| metadata | source_coverage 的公開 windows、restriction_coverage、QA/errata ID 集合、mechanic universe/coverage | 不造 complete；window 只准 region:* 或此次公開 product；target 三欄全 null；未實作 review/translation coverage 為 [] |
| `card_engine_support` | R1 每 card 一列 missing_dsl，包括墓碑；再附缺來源、未核對、divergence、wording 等區域 block | 無 DSL／program；`effective_support` 先 override、後 block；automatic=false；passed 被 block 降 reviewed |

所有非 null ID，包括 nested wording、譯文、裁定與印刷歷史，均須在同一公開投影可達。
無用文字與翻譯不出貨；text ID 按 exact UTF-8 字串生成並驗碰撞，不重配舊鍵。
新建的 glossary／譯文字串使用相同配號規則；碰撞檢查只涵蓋同一份快照，不保存跨版本的發布鍵索引。

## 建置接點

`Decisions` 只承接其他建置能力已驗證的結果，不重新執行其採納邏輯：

- `wording`：face ID → pending WordingView 陣列；由 #145 計算 display、候選與未知日期。
  `observed_texts`：精確 `(printing_id,face_id)` → 完整公開觀測陣列，包含 revision、state、source URL。
  `Decisions.with_text_views(wording_views(db, plan), printing_observed_texts(db, plan))`
  直接接收 #145 的 Pydantic 模型，經 JSON 型別邊界保留全部公開欄位；不重算表記或觀測狀態。
  來源不可用時即使 DB 沒有觀測列，仍保留原生輸出的 `missing_effect`／null revision／來源 URL。
  `observation_states` 為既有 DB 觀測列的精確 `(printing_id,face_id,source_id)` 狀態接點；
  提供完整 `observed_texts` 的 parent 以完整模型為準，不走此推導。
  projector 驗 shape、唯一 region、同面同區 revision、候選能回 observations、display/current 與 ID 閉包。
  有觀測且沒有 current 時不得缺 wording；未定觀測不改 printed、route 或 Decklog。
- `display_bindings` 與 `aligned_regions`：建置器已核對且 fresh 的來源 owner／跨區顯示選用；
  來源 use 一律驗原始 owner/context；跨區以 destination 的結構鍵選定顯示 owner。
  官方 counterpart 的 display owner 就是原始 source owner，必須提供 direct translation ID，
  只取代該 owner 的同欄同語言共用選譯；shared_jp 使用 JP 共用選譯，顯示於同面 EN owner。
  未核對、divergence 或只有 pending display 的跨區選用不出。建置器仍須驗 exact source bundle／名稱來源。
  #29 的 shared_jp_unchecked 尚待 Schema／建置能力接入，本模組不自行產生未核對選用。
- `related_regions`：reskin 在各輸出地區的已驗證 eligibility。
- `active_scopes`：ruling revision 的已驗證現行有效文字單元；projector 不猜部分取代的切段。
- `general_evidence`：printing ID → 路由建置器的 `GeneralEvidence`，原樣傳給唯一的預設版次選取器；
  不由卡號、字串猜加工，不把未知分類補成已確認一般版。

目前 baseline 機器契約尚未含 `face.wording`／PrintingFace.observations；依 #143 的既定分工，
這兩項使用本模組的 exact shape／連結檢查，其他欄位仍完整通過 baseline Schema。
待 #145 同步候選 Schema 後，既有 validator 自動驗其 descriptor；本模組不改 Schema／golden／reader。
這份邏輯結果在那之前不能被宣稱為已通過 A 的完整 wire 快照。

## 驗證

```bash
uv --directory carddb run pytest tests/test_snapshot_project.py
```

合成 DB 正例讓 43 集合都有資料。`tests/fixtures/snapshot-project/expected-ancillary.json`
是獨立手寫的完整附屬列 oracle；測試另驗核心、config、support、nullable 欄與缺能力反例。
區域反例分別覆蓋混合日期、未確認 mapping／release／role、角色衝突、未知代碼、divergence 範圍與 QA 現行版本。
整體反例驗離線 DB 版號與 join 後的引用閉包；未核可圖帶 variant 則先由建置完整性約束拒絕。
正式卡文不進 fixture 或回報；真實封存來源與合成資料的數量須分開記錄。
