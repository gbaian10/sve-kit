# authored 維護方式

引用與授權：範例中沿用的官方卡名、商品名、詞彙及卡文片段不在本專案授權內；
專案欄位、合成值、中文說明與資料規則依文件授權。來源及適用範圍見[文件引用說明](../quotations.md)。

身分登錄格式 **v1，2026-09-28 定案**。本文件定案的範圍為永久 card／face／printing、printing 整數編號（`int_id`，依地區分段配號）、日英對應、無對應審核、英文原創插畫、換皮卡，以及本批來源更正。商品人工輸入格式另定為 **product-authored-v1**（§10），不擴充既有身分登錄格式；current／wording 採納輸入 **wording-adoption-v1，使用者 2026-10-01 核可具體格式**（§9.1–§9.7）；§9.5、§9.6、§9.8 分別記載 2026-10-01 已核可的處理政策；身分修復與決定續版 **identity-transition-v1，使用者 2026-10-01 核可**（§12）；翻譯／模板格式與推導邊界見 §6（2026-10-01，技術契約）；詞彙與顯示覆寫封套見 §8（技術契約，稀有度白名單與繁中缺譯順序於 2026-10-01 核可）；來源綁定的插畫裁切覆寫見[覆寫契約](image-crop-overrides.md)（技術契約，不表示 loader 或資料已完成）；人工限量序號版次另定待審技術契約 [manual-printings-v1](manual-printings.md)，收錄政策依維護者 2026-10-03 最新更正；構築專用入口見 [construction-adoption-v1](construction-adoption.md)，首批政策為維護者 2026-10-03 決定的 JP／EN Standard，首發先上可查禁限資料、CR 引用與固定 ref 等 #48 並明示尚未完成，不承諾整副牌合法性；其餘類別仍是提案。建置資料庫語意以 [build-db.md](build-db.md) 為準；已核可的表記未定顯示擴充另見 [snapshot-format §2.3](snapshot-format.md#23-表記未定的公開呈現)。

## 1. 路徑與共同格式

| 狀態 | 類別 | 實際路徑或提案 |
| ---- | ---- | -------------- |
| 定案 | 永久卡、面、版次 | `registry/card/<owner>/001.yaml`、`registry/face/<owner>/001.yaml`、`registry/printing/<owner>/001.yaml` |
| 定案 | 整數編號配號／全域入口 | `ids/<owner>/001.yaml`、`ids/index.yaml` |
| 定案 | 英文獨有查核 | `registry/region_mapping_review/<owner>/001.yaml` |
| 定案 | 英文原創插畫 | `registry/art/<owner>/001.yaml` |
| 定案 | 換皮卡 | `registry/card_related/<owner>/001.yaml` |
| 定案 | 本批來源更正 | `registry/source_correction/active/<owner>/001.yaml`、`registry/source_correction/needs_review/<owner>/001.yaml` |
| 定案（格式） | 歸檔類別、人工商品與收錄 | `products/index.yaml`、`products/{family,product,inclusion}/<filing_key>/001.yaml`，見 §10；不表示已有採納資料或匯入器 |
| 定案（格式） | 官方商品身分對照 | `product-identities/index.yaml`、`product-identities/<region>/001.yaml`，見 §11；獨立於商品內容採納 |
| 定案（格式） | current／wording 表記採納 | `wording-adoptions/index.yaml`、`wording-adoptions/<region>/001.yaml`，見 §9.1–§9.7；使用者 2026-10-01 核可格式與處理政策，不表示已採納任何群組 |
| 待審技術契約 | 人工序號版次／官方序號補充 | `manual-printings/index.yaml`、`manual-printings/{printings,serials}/<filing_key>/<sequence>.yaml`，見 manual-printings-v1；不表示已有採納資料 |
| 已定案（ADR-0011） | 裁定 | `rulings/R-0001.yaml`，維持原格式 |
| 定案（格式） | 身分修復與決定續版 | `identity-transitions/index.yaml`、`identity-transitions/<sequence>.yaml`，見 §12 |
| 待審技術契約 | Standard 構築／禁限／角色與必要 CR 引用 | `construction-adoptions/index.yaml`、`construction-adoptions/<area>/<region>/standard/<sequence>.yaml`，roles／cr 為 `<area>/<region>/<sequence>.yaml` 整區共用；見[構築採納契約](construction-adoption.md)，尚無正式採納資料 |
| 提案 | 其他策展、標誌 | `curation/BP01/001.yaml` |
| 待審（技術契約） | 數位對應與查核覆蓋採納 | `digital-links/index.yaml`、`digital-links/{links,coverage}/<filing_key>/<sequence>.yaml`；見[數位對應採納契約](digital-link-adoption.md)，真人link入口已實作、尚無逐卡正式遷入；coverage未實作 |
| 定案（技術契約） | 模板、詞彙、翻譯採納 | `translations/index.yaml`、`translations/{templates,glossary,overrides,region-reviews}/<filing_key>/<sequence>.yaml`、`translations/template-sources/<sequence>.yaml`；推導結果不進 authored，見 §6 |
| 定案（技術契約） | 詞彙、記號、搜尋別名、特殊構築名稱、語言 | `catalog-adoptions/index.yaml` 與分片，見[採納契約 §2](catalog-route-adoption.md#2-入口分片與封套) |
| 定案（技術契約） | 同號路由與預設版次覆寫 | `display-overrides/index.yaml` 與分片，見[覆寫契約 §5](catalog-route-adoption.md#5-同號路由與預設版次覆寫)；永久路由修復仍走 identity-transitions |
| 定案（新格式） | 數位名字／同名瀏覽政策 | `digital-name-policies/index.yaml`；名字用可修改的 current 規則，同名瀏覽維持獨立非翻譯入口，見[名字契約](digital-name-policy.md) |
| legacy，僅供轉換 | 模板採納政策／核可收據 | `translation-policies/index.yaml`、`translation-policies/<policy_id>.policy.yaml`／`.approval.yaml`／`.review-queue.yaml`；無文字摘要進索引，首輪實際抽查、不可變索引與五欄 pin 依 [模板採納政策契約](translation-policy.md)，未支援完整 loader 前不得套用 |
| 定案（新格式） | 模板參數辨識規則 | `template-parameter-rules/current.yaml`，format 2；規則與必要反例隨程式 PR 修改，不需 approval |
| 定案（技術契約） | 來源綁定的插畫裁切覆寫 | `image-crops/<filing_key>/<sequence>.yaml`；無 index、全體查重，列不存 image_id，依[覆寫契約](image-crop-overrides.md)；不表示 loader 或資料已完成 |
| 提案 | DSL、設定 | `effects/`、`macros/`、`config/` 等，見後續各節；跨區語義差異採納改走 translations/region-reviews |

`owner` 是首次歸檔代號，保留大小寫（例如 BP01、DSD01a、PR），不是商品收錄證據。card 採首次配發代表版次的 owner；printing 與配號按自身 owner，跨包外鍵允許。檔名為只增序號，不因新增較早排序的卡而重新分片。每檔 **小於 1,048,576 bytes**，以 512 KiB（524,288 bytes）為目標：以**寫出後的完整分片 YAML**（含封套、decision 的 members／sample_ids）量測，依序裝入不超過目標的最多筆數；單筆就使分片達 1 MiB 時直接報錯。PR 同樣切序號檔，不造單一大檔。

分片內記錄依對應 printing 的 `(region, card_no, variant_key, printing id)` 排序（`card_no` 為原樣字串的 code-point 字典序）：printing、配號、來源更正用自身或所指 printing；art 用第一個 use 的 printing；card、face、英文獨有查核、換皮卡用該 card 所有 printing 中最小的鍵（face 再加 ordinal）；最後一律以 `record_key` 收尾。排序只作用於**同一次寫入的新記錄**：正常追加只排序本次新增、寫到該 `(area, owner)` 的下一個序號檔，舊分片不動，所以同一 owner 的多個分片合起來不保證是全域卡號序。2026-09-28 首次公開前曾一次性全量重新分片（見 §3.2）；此後不再重排。

YAML 為單一文件、UTF-8；省略版本指示或明示 `%YAML 1.2` 可讀，其他版本指示拒絕。`carddb` 使用 `yamlrocks==0.6.1` 的純量與詞法語意，不宣稱完全等同 YAML 1.2 core resolver，也不保留舊數字拼法相容層：例如 plain `0777`、`0b101`、`1_000` 是字串；正常十進位、`0o17`、`0xFF` 與指數形式依套件解讀。日期仍是字串，不啟用 timestamp；作者的日期字串加引號慣例不變。所有鍵必須是字串，禁止重複鍵、非有限浮點；套件先拒絕重複鍵與複合鍵，載入後仍做 strict JSON、canonical 雜湊（`allow_nan=False`）、結構與引用檢查。大小限制與分片規則不變，另見 [DSL 1.0 §11](../dsl/author-syntax-1.0.md#11-載入與錯誤)。

Anchor／alias／merge 與顯式 tag **讀取允許，寫入不產生**；普通 alias 與 merge 由套件展開，循環 alias 報錯，quoted `"<<"` 保留為一般字串鍵。標準 tag 依套件解讀；自訂 tag 保留為物件，再由 strict JSON 拒絕，不註冊 tag callback，也不啟用 include／env／secret／Python 物件執行。合法位置的 tab、NEL／LS／PS、interior BOM 與未知指示由套件處理，不另設 libyaml 字元禁令或 lint；縮排等語法錯誤仍拒絕。Parser 錯誤只回報安全類別，不附原始文字。寫出維持 `ruamel.yaml` 與原有 `storage.encode`，不設 PyYAML fallback；套件升級須重驗 canonical 差分與邊界案例。

人工限量序號版次的獨立入口為 `manual-printings/index.yaml` 與 `manual-printings/<area>/<filing_key>/<sequence>.yaml`；完整欄位、來源類別及續版以 [manual-printings-v1](manual-printings.md#2-入口封套決定與續版) 為準，不加入身分 registry v1。

構築採納另有獨立 `construction-policies/index.yaml` 與 `<policy_id>.policy.yaml`／`.approval.yaml`／`.review-queue.yaml` 政策閉包，見[構築採納 §1.2](construction-adoption.md#12-政策首輪抽查與核可收據載體)；不列採納 includes，不借翻譯政策授權。載體／loader 與真實首輪收據未到位時，不得政策採納。

## 2. 分片、批次決定與來源

本節決定封套用於仍採該格式的身分／商品等入口；翻譯 format 2、詞彙／語言 format 2 與名字當前規則
依各自契約，不使用本節的 membership、sample_ids 或核可收據。共通 YAML、安全路徑、大小與引用檢查仍適用。

每個身分登錄分片有 `authored_format: 1`（分片格式未變；`ids/index.yaml` 為 2，見下）、`kind: registry_shard`、`default_decision_id`、`records`、`decisions`。每筆 record 固定為 `record_key/kind/owner/data`；`data` 是該 kind 的資料。配號以外，匯入時將封套的 decision 展開成具體資料表 FK，不另建立 subject 真值表。配號分片的 decision 為 null，decisions 為空。

```yaml
authored_format: 1
kind: registry_shard
default_decision_id: "d:<64 hex>"
records:
  - record_key: "card:c:<32 hex>"
    kind: card
    owner: BP01
    data:
      id: "c:<32 hex>"
      layout: single
      identity_state: confirmed
      home_set_id: BP01
decisions:
  - id: "d:<64 hex>"
    state: confirmed
    scope: batch
    category: identity_registry
    policy_id: identity-init-2026-09-28-v1
    membership_hash: "sha256:<64 hex>"
    members: [["card:c:<32 hex>", "sha256:<64 hex>"]]
    sample_ids: ["card:c:<32 hex>"]
```

這是格式示意，不是額外審核證據。2026-09-28 的採納依使用者整批確認與後續裁決；`sample_ids` 列**全部 checked record keys**，不是抽樣。決定不保存製作者／核對者姓名、時間或精度。本批新找出的更正候選用 proposed decision 與空 checked 集合，不能冒稱已確認。

hash recipe 固定：JSON 物件鍵排序、UTF-8（不 ASCII escape）、分隔符 `,`／`:`、無額外空白／尾端換行，不正規化 Unicode。先對完整 record（不含封套的 decision 指針）計 semantic hash；將 `(record_key,semantic_hash)` 二元素陣列按 key 排序，再計 membership hash。decision ID 使用完整 membership hash。任何新成員或內容變更都不得沿用舊決定。2026-09-28 的一次性重新分片讓部分 printing 分片合併，這些分片的 decision 依新成員重算 ID／members／membership_hash；政策與確認狀態沿用原成員的決定（原本就是同一次使用者整批確認），不是新的審核事件，也不保留舊 decision ID。

`ids/index.yaml` 的 `includes` 是 authored 根目錄相對路徑 → 分片**解析後 canonical JSON** hash；`authored_format: 2` 的 index 另有 `allocation_policy`（目前 `region-ranges-2026-09-28-v1`）與各地區游標 `next_int_id: {en: …, jp: …}`，每個游標是該區下一個未使用值；鍵必須恰為政策內的地區，值落在 `[start, end+1]`，`end+1` 表示該區已用盡。不認識的政策或格式直接拒絕。解析內容 hash 可容忍格式工具只調整 YAML 排版。入口列出所有登錄分片，不掃描未被納入的檔案作為有效資料；存在未索引的登錄檔時停止，避免中斷後重用配號。

每張 printing 的 `observation` 保存 `region/card_no/recipe/observation_hash/rules_hash`。`registry-observation-v1` 是工具 `Card` typed projection 的 canonical JSON；包括全部 faces 的 card name、職業、種類、數值、特性、原文、sections／speech、來源 img src，不含抓取時間或本機路徑。它是**萃取觀測 hash，不是原 HTML hash**。原始萃取仍留 repo 外；建置匯入需以同 recipe 驗證原始觀測並連到 source_record／decision_source，不能把它偽裝成官方 HTML 的 sha256。僅取得此 registry 不足以重建官方卡文。

規則 hash 包含逐面 `name/text/speech/sections` 的原值。JP／EN 再錄措辭、提醒文字與標點差異可依本批人工政策共用 card，但各觀測分別保留；**不產生 confirmed 的 revision_semantics 或規則等義證明**，不沿用 DSL 驗證。未來來源變動須重新審核，不是忽略括號後自動通過。

建置讀取先驗完整 index 與全區分片，再做地區投影；不得先濾 JP 再改寫 decision 成員、checked 集合或配號游標。讀取保留分片路徑、解析後 canonical 內容及 hash、record 的原 decision 指向與完整封套。格式／kind 等封套欄位必須明示；配號與帶 decision 的 record 不得混在同一分片。decision ID 必須由其完整 membership hash 決定；任何狀態的 sample_ids 都不可重複或含非成員，confirmed 仍須全部 checked。這些檢查驗歷史登錄一致性，不代表已對目前 raw 來源驗證 fresh 採納。

## 3. 永久身分、配號與策展

### 3.1 ID 與身分

ID 使用 `c:`／`f:`／`p:`／`a:`／`r:`／`x:` 加 UUIDv5 的 32 小寫 hex；固定 namespace 為 `e304714a-f18c-5fb6-a987-222988ffbb7a`。UUID 名稱為 kind、NUL、首次 anchor，並檢查全域碰撞。首次 printing anchor 是 exact `region:card_no`（目前 variant 固定 standard）；card anchor 是採納群組首次代表 printing 的 anchor；face anchor 是 card ID 與固定 source ordinal。art anchor 是 printing ID 與 face ordinal。這些配方只用於首次配發，**既有 registry 優先，不能日後重新算 card 分組來換 ID**。

card 的 `data` 如前例。face 的 `data` 為 `id/card_id/ordinal/side`，single 恰一個 front，double_faced 恰 front/back；一般進化前後不同種類，因此不同 card。printing 格式：

```yaml
record_key: "printing:p:<32 hex>"
kind: printing
owner: BP02
data:
  id: "p:<32 hex>"
  card_id: "c:<32 hex>"
  region: en
  card_no: "BP02-070EN"
  variant_key: standard
  home_set_id: BP02
  source_face_map:
    - {source_index: 0, face_id: "f:<32 hex>"}
  observation:
    region: en
    card_no: "BP02-070EN"
    recipe: registry-observation-v1
    observation_hash: "sha256:<64 hex>"
    rules_hash: "sha256:<64 hex>"
  cross_region_review:
    checked: true
    target_jp_card_no: "BP02-071"
    target_observation:
      region: jp
      card_no: "BP02-071"
      recipe: registry-observation-v1
      observation_hash: "sha256:<64 hex>"
      rules_hash: "sha256:<64 hex>"
```

卡號保存官網原樣，含 `Ⓢ`、小寫 `a`。EN 的 target 來自已確認候選及有序覆寫，絕不以去 EN 自動配對；JP printing 不填 cross_region_review。有對應時兩端共用 card／face；target 卡號只是核對證據，不是第二張 region_mapping 真值。EN-only 仍有獨立 card 和 printing，target 及 target_observation 為 null。

JP 初始分組依全部面同名／同職業／同種類／同數值／同特性，加上人工判定的規則差異；不是只依同名自動採納。EP、SEP、CP03-125/126、ルゥ遵循 build-db §3.1。同欄位但實際卡面規則不同時，由初始化決定檔的 `separate_groups` 指定不同群組。雙面的 source_index 是此次全體 checked 的面對應，不是以 ordinal 猜日英面對應。

### 3.2 UInt32

`card_int_id` 的 `data` 僅 `int_id/printing_id`；`record_key` 為 `card_int_id:<printing_id>`。已配發記錄不修改、不刪除、不重用，沒有 decision。型別上限仍是 UInt32（4294967295），可用號段由版本化配號政策 `region-ranges-2026-09-28-v1` 決定，程式唯一定義在 `sve_carddb.registry.allocation`：

| 號段（閉區間） | 用途 |
| -------------- | ---- |
| 1..20000 | 保留；一般配號器不得使用，特殊用途須另定放行程序（尚未定義） |
| 20001..59999 | `region=jp` 的 printing |
| 60000 | 未分配 |
| 60001..99999 | `region=en` 的 printing |
| 100000..4294967295 | 未分配；新地區開新的、不重疊的號段 |

號段依 printing 登錄的 `region` 查詢，**不從號碼反推地區**。新增只從該區游標往後追加，不回填空號、不依最新卡號排序重編。配發前先算出本批各區需求，任一區超出區段即整批失敗，不部分寫入、不溢出到其他號段；擴區規則日後另定。驗證要求每號落在所屬 printing 地區的閉區間、全域唯一且與 printing 一對一，各區游標等於該區最大號＋1（空區為起點），游標不得回退。

2026-09-28（首次公開前）曾一次性重配全部 14,789 筆：各區依 `(region, owner, card_no 原樣字串, variant_key, printing id)` 自區段起點連續配發（JP 20001..27369、EN 60001..67420），並同時全量重新分片。字串 ID、地區、卡號與其他語義資料不變。這是公開前唯一的例外；此後只增不改。

工具持有全域檔案鎖，先驗證全部輸入、既有索引／hash／外鍵、計畫與大小，再寫新分片，最後原子替換 index。重跑無變更時不寫檔。中斷留下未索引分片時停止；恢復者須從 git／備份核對完整批次，不能刪檔後猜 next-id。來源更新拒絕覆寫舊證據，另走來源版本與決定重審流程，不因文字更新產生 identity_change。父 card 改動或合併既有 card 才需 confirmed identity_change；此工具不實作身分修復或同號 variant 猜測。

### 3.3 英文獨有、原創插畫與換皮卡

`region_mapping_review` 的 `data` 為 `card_id/target_region/state/as_of/coverage_scope/coverage_hash/observations`。本批 target_region=jp、state=confirmed_none；新增記錄的 as_of 取 CLI 的 `--as-of`（JP 輸入的日期），coverage_hash 是當次完整 JP 萃取輸入檔的 SHA-256，observations 釘受審 EN 版次。匯入 source_id 指這份 immutable authored 查核紀錄；不是推論永遠不會出日版。

`art` 的 `data` 為 `id/card_id/face_id/classification/uses/observation`，uses 是 printing_id＋face_id 陣列。本批原創插畫只掛已確認的 EN printing；未確認基準及跨版次同圖組，故 classification=unclassified，不造假 base／alternate 或 JP art。`art_id=null` 的其他 printing 不代表沒有插畫。

`card_related` 的 `data` 為 `id/from_card_id/to_card_id/relation/source_kind/target_printing_id/suggested_count/dsl_id/evidence`。relation=same_rules_reskin、source_kind=authored，三個選用欄為 null。evidence 逐筆記 `role: from|to` 及全部兩端已採納版次的觀測；匯入 decision_source 釘兩端來源。相同換皮卡的普通／特殊 printing 只產一條關係，禁止自指、多目標與反向重複。只能在兩端都有版次且來源驗證仍匹配的地區投影；任一端目前來源改變就停止該地區投影，依 build-db §5 重審。不能繼承原卡 DSL 或構築張數。

confirmed_none 與 same_rules_reskin 的決定不可因追加版次自動擴張。工具與獨立 validate 都逐筆比對 printing 的完整 observation（換皮關係另含 from／to），拒絕缺漏、重複、過期或多餘證據。新版次需要新的 review／relation 決定並釘住新觀測；v1 尚未啟用決定續版格式（[§12 的獨立封套](#12-身分修復與決定續版)已核可、待實作），因此現有工具追加既有 EN-only card 或換皮關係任一端的版次會直接失敗並提示重審，不改寫舊決定。

登錄內的 printing.observation 必須與 printing 的地區及原樣卡號一致；有 EN 對應時 target_observation 必須與已登錄 JP 目標的完整 observation 相同，無目標時為 null。art 的 observation 必須對應其已登錄 use，uses 不可重複；換皮關係不可反向成對。這些是既有證據的引用一致性要求；實際來源版本的觀測比對、跨區採納新鮮度及發布投影仍由匯入器另驗。

匯入器先驗完整登錄，再明示輸出地區；完整歷史決定與「凍結來源仍匹配」是兩個維度。來源缺失或變動時保留原封套及決定狀態，逐筆回報 record_key、決定與來源比對結果，不能把歷史 confirmed 當成重新確認。來源面數須與 source_face_map 完整覆蓋一致，每面欄位依原 source_index 取值；同一 printing＋face 不得同時採納兩個 art。未投影與暫緩的登錄亦保留於完整輸入，不得由區域子集重算 members／sample_ids／配號游標。

### 3.4 來源更正

`source_correction` 的 `data` 保存 `id/printing_id/face_id/field/expected_raw_value/corrected_value/expected_source_hash/source_hash_recipe/reason/state/reported_to_official/reported_on/report_url/evidence`。本格式 field 白名單為 effect、card_type，分別映射效果文字與種類；公開 field／值型別依 [傳輸契約 §3.3](snapshot-transport.md#33-公開更正值)，不擴張此 authored 格式的兩欄白名單。expected_source_hash 採前述觀測 recipe，匯入仍須先匹配來源版本，不可直接替換 HTML 原文。

evidence 元素為 `kind: card_image`、`sha256`、官網原樣 `image_src`、region、locator（卡面文字框／種類標記）。匯入以檔案 hash 與 URL 連到 image source_record，再寫 correction_evidence；不把圖片或本機路徑存進 git。BP07-P06 由協調者確認為 active；使用者於 2026-09-28 追加確認 JP PR-114、BP20-P42、BP20-P57、BP20-P67 與 EN BP15-P32EN、CP03-127EN、PR-388EN、PR-442EN，這八筆亦為 active，decision 為 confirmed，不保存確認者與流程日期。卡圖 hash 與原觀測不變。一般尚未確認的候選仍用 needs_review＋proposed decision，不得套用至 current／規則解讀。來源原始觀測永遠保留。卡圖已支持的同卡判定與來源欄位是否正式套用更正是不同採納事項。

本批八筆候選升為 active 是使用者明示授權的來源更正採納，非一般追加：移至 active 分片、建立涵蓋精確內容的新 confirmed decision、同步更新 index。先前 proposed 分片與收據保留在 Git 歷史及舊批次資料中；其他永久登錄與決定不改動。一般產生工具仍拒絕修改既有記錄，不以自動升級取代人工確認。

建置的 `registry.corrections.project_corrections` 對 active 更正比對原值與完整觀測 hash，匹配才產生更正值與欄位標記；原值已等於改值時回報 already_fixed，不重複套用，後續需警告並退役來源更正；其他差異為 conflict，不套用並阻擋 CLI 完成。needs_review 不產生投影。結果依 printing／face／field 定位，`corrections` 元素使用 snapshot-format 的 `{field, corrected_from, is_corrected: true, reason, source_url?}`；沒有可用官方頁 URL 時 source_url 仍存在、值為 null，不把卡圖 URL 冒充官方頁。此標記不附到共享文字上。

CLI 每次建置均驗證這個投影，可用 `--corrections-output <absolute-derived-json>` 保存更正後的欄位 value、applied／already_fixed 狀態與顯示標記；加 --check 時只比對已存的投影，不寫檔。這個 CLI 輸出仍是欄位投影，不是完整 face_revision 或公開卡表快照。正式建置 staging 由 `source_corrections` 接入文字觀測匯入器：核對封存圖片後匯入三表，保留原始觀測並建立更正後 revision，再提供受影響引用者的公開 Correction 值；完整快照序列化與發布驗證另由匯出器處理。API 與 F1 使用閉包見 [來源更正實作說明](../../carddb/src/sve_carddb/source_corrections/README.md)。

### 3.5 重跑與新卡包

工具入口為 `uv --directory <absolute-carddb> run python -m sve_carddb.registry`；參數 `--jp/--en/--candidates/--confirmations/--original-art/--decisions/--images/--authored` 全為明示路徑，`--as-of` 為 JP 輸入的 ISO 日期，`--check` 要求現有輸出完全相同。全程只讀本機來源，不讀 manifest、不抓網路。

decisions 是本機的初始化決定檔（`registry.review.InitDecisions`），JSON 欄位為 policy、corrections、reskins、separate_groups、art_groups，只記人工配對決定。policy 目前固定 identity-init-2026-09-28-v1。工具不從 confidence 產生配對決定；產生的登錄照一般 PR diff 審查。新包先產候選、核對所有面與差異、核圖實質增刪，再更新決定檔。任何未核對的職業、種類、數值或英文同卡名稱衝突都失敗。

corrections 元素包含 region、card_no、face_index、field、expected_raw_value、corrected_value、image_sha256、locator、state、reason。needs_review 的 card_type 候選另須明示 `adoption_scope: identity_check_only` 與 `source_correction_status: pending_user_confirmation`，否則拒絕用於身分核對；這兩欄不會提升來源更正狀態，也不改寫原觀測或正式分片。reskins 是 EN 卡號 → JP 原卡號；separate_groups 是 `region:exact_card_no` → 明示分組鍵；art_groups 是已核對同幅插畫的 EN 卡號陣列集合，組間不得重疊、不能跨 card。未列入者各自登錄；本批 CP02-072EN／CP02-P57EN 依同圖不同簽名加工規則共用 art。決定檔不進 git，採納後的永久登錄與其精確成員決定才是維護狀態。新增 package 使用包含原觀測的完整輸入集合，不把變動的舊來源塞進追加工具；舊來源更新另走 source／identity 修復流程。

### 3.6 輸入保存

每批在 repo 外的 `SVE_DATA_DIR/derived/registry/<batch-id>/` 建立新的永久目錄；不可覆寫舊批次。保存以下 exact bytes，不重新序列化或排序既有輸入：

| 檔名 | 取得方式與內容 |
| ---- | -------------- |
| jp.jsonl | 取得已核對的 JP 萃取快照；每行 Card 的 number 與完整 faces。一般 JP 萃取由 extract/jsonl.py 產生；本工具不讀 manifest、不執行萃取 |
| en.jsonl | 取得審閱批次的完整 EN 萃取快照；每行同樣符合 registry.inputs.Card。目前沒有正式 EN 萃取 CLI，不可假定重新解析 HTML 能還原舊批次 exact bytes |
| candidates.jsonl | 取得本批已審候選；每行 en_no、category（A/B/C）、jp_candidates（含 jp_no）。新增批次須完整列出 EN 版次，人工確認 A/B 第一候選或 C 無對應 |
| confirmations.tsv | 保存依序追加的人工裁決，欄位 en_no、jp_no、verdict、confirmed_on；後列覆蓋前列，不能重排 |
| original_art.jsonl | 保存人工卡圖比對結果，每行含 en_no、verdict；en_original_art 是插畫確認證據 |
| decisions.json | 本批使用的初始化決定檔，欄位見 §3.5 |

舊批次的取得方式是從保存目錄或其備份複製上述檔案，使用 SHA256SUMS 驗證；不依賴 session 暫存檔、個人草稿或重新生成候選。舊批次搭配當時的 Git commit 重現。inventory.json 記各檔 hash、大小與來源。SHA256SUMS 與決定檔都需納入批次備份；git 不存官方原文。

新批次依序執行：

1. 取得完整日英萃取與所有候選／人工確認／插畫證據，依 §3.5 核對各面、規則差異及必要卡圖。保留原始欄位，不將候選更正寫回輸入。沒有 EN 萃取器時須先提供可審閱的完整 Card JSONL，不能省略原文或沿用 confidence 當決定。
2. 建立全新 batch-id 目錄，複製五份輸入；以 SHA-256 比對來源與副本 exact bytes。原有卡片觀測若更新，走來源版本重審流程，不能覆寫舊批次。
3. 人工完成核對後，建立符合 `registry.review.InitDecisions` 的 decisions.json：corrections／reskins／separate_groups／art_groups 依 §3.5 填寫；無資料則空集合，不能自行沿用上一批的決定。
4. 寫入 inventory.json 與涵蓋本批全部檔案的 SHA256SUMS，記錄輸入取得方式。
5. 將 §3.5 CLI 的五個輸入與 --decisions 全部指向此持久目錄，--as-of 填 JP 輸入的日期，--images 指本機官方卡圖、--authored 指登錄目錄。先驗證輸入／計畫，完成後用相同命令加 --check 驗證零改寫。既有 EN-only／換皮卡追加需等待續版決定格式，不能透過修改決定檔繞過拒絕。

## 4. 新卡包的人工作業量

確定性項目全自動：官方來源欄位、`face_current` 無衝突預設、`rules_name`、`deck_role`、route、default printing、int 配號、模板套用、projection/coverage 報告。

人處理：新句型/語義衝突、JP 身分歧義、非規則數位/異畫/標誌批次抽查；獨立核可同名規則按完整條件產卡層same_name、不造真人樣本、新卡名/譯文抽查、EN 身分逐筆確認、必要裁定與手動 override。正常 JP 包不要求為每張卡寫 current/default/route/decision 四份檔；封套＋工具結果可一次審閱。

度量不是把人工語義壓到固定數量：每次報 `generated_rows`、`explicit_overrides`、`sampled_rows`、`individually_checked_rows`、新句型數、人工作業時間。200 個版次的普通 JP 包，以「0 筆手寫 route/current/default/int decision、策展按包批次、只有例外覆寫」為驗收。可把 `explicit_overrides`≤新卡數當觀察目標，超出要找自動化缺口，但不可因此略掉有必要的確認。EN 對應的全筆檢查單獨計，不能用 JP 指標減掉。

## 5. 插畫/數位/標誌匯入順序

來源頁`→identity/face→art_group` 採納`→printing_face.art_id→digital_art_link`。svwb-art 的 printing+face 候選原樣保留 staging；art 沒採納前不 materialize 正式 link，也不配假 art。批次原樣高 confidence 不自動 sampled。

每個 face 的 base art 明示；alternate 需有 base 和不同圖證據。signed 留面級 Bool?；序號在 `printing.serial_total`；stamp 留原字與顯示年，不生造 event、日期或名次。`credit_raw` 由版次頁保存於建置資料庫的 `printing_face`，不併到 `art_artist`。

## 6. 模板、翻譯與語義例外

翻譯的 index／分片使用 `translation_authored_format:2`，清冊使用 `template_source_format:3`，
完整欄位依[翻譯契約](translation-contract.md)與[清冊重產契約](template-source-replay.md)。
舊決定封套、membership、核可收據、採納鏈、歷史 producer／expected 只供轉換，不是新 reader 的必要輸入。
資料可直接改，退回用 git revert；只記來源類別 official/project/machine、低信心及必要資料理由。

模板功能全部保留：固定字、參數、句型比對、術語、卡名、風味、加粗、新卡自動套用及未匹配清單。
[辨識規則](template-parameter-policy.md)由現行程式與當前設定提供；風味依[整段模板契約](flavor-translation.md)。
純譯文／note 改字不換模板或術語 ID；真正固定字／參數語義改變才是另一模板。
一般讀取驗結構與引用，CI／建置用本次固定來源重產清冊比檔案，不逐次回放 Git 祖先或舊環境。
context/use/binding、渲染全文與 selection 由工具推導，不存另一份逐卡翻譯真值。

context_assignment／card_name_concept 只對自己的 owner 與 exact 原文有效，建置驗目前身分與原文；
不再釘核可時 identity_basis 或要求手動 base-main 檢查。它們不取代跨區身分或數位同卡關係。
同名歧義與撤回仍有明示資料，不能因移除收據就按字串猜配對。
術語與加粗的當前值見[術語契約](glossary-adoption.md)，class/type 仍引用 vocabulary。

繁中以 JP 原文為主；EN 已確認同卡而文字未核對時沿 shared_jp_unchecked 顯示提示，
已核對為 shared_jp，已知 divergence 不共用受影響欄位；不放行未核對的 DSL／機制或官方 counterpart。
origin 與 authority 分開，本站效果翻譯仍 unofficial；機器譯文人看過仍 machine。
低信心但自動檢查通過的譯文直接顯示待校對，可切原文；壞結構／錯來源不渲染。

平常一個新包只需一個資料 PR 與一張低信心／新句型／缺項清單，
不要求首輪抽查、逐筆雙模型、approved_policy、額外核可頁或手動合併前驗證。
官方 CI 輸入沿現有私有 testdata repo 及鎖定檔，個人機器路徑與事件不得進 committed 資料。

## 7. DSL 與拒絕輸入

meta 保留 DSL 版本、rule-bundle-v2 source hash（原觀測 face-bundle-v1 另留追溯，遷移需重驗）、`written_by/reviews/status`、`verified_by_exam`、ruling IDs、QA IDs、macro 用途。審卡程式自動填，不要人工複製逐卡 hash。shared 是預設，EN exception 才 `scope=en_override`；DSL body 僅依 `dsl/` 真正 schema，不在此造示意 op。

effects/index.yaml 提供 `card_id`＋scope＋file/record key；即使候選 YAML 解析失敗仍可定位卡。candidate hash 取 exact authored bytes，`load_report` 在建置層追加 accepted/rejected＋目標版本。valid AST 才進 `dsl_document`；無效候選不進公開附件，support 仍能列 `rejected_yaml`。

題本/載入結果與 program/source/engine build/policy/測試輸入版本匹配，最新 fail 不能沿用舊 pass。引擎未指定時不冒充 `engine_passed`；公開只投影 support 和 `program_ref`，private raw 報告不出貨；附件准入與 AST 出貨依 [傳輸契約 §3.4](snapshot-transport.md#34-dsl-程式包與版本准入)。巨集作者/兩模型審查/機械三檢查留建置資料庫。

## 8. 更正、路由與設定

`source_correction` 原值/改值/字段/來源 hash/evidence/核對者日期/是否回報完整保存。套用前先比是否已上游修正；不符合 expected/hash 則停用重審。卡表快照只出引用者上的 `corrected_from`/標記/公開理由，不能在去重 text 上全域標更正。

route/default 純推導；authored 只寫 alias、variant `route_override`、`default_printing_override`。canonical 編碼 exact 原卡號，folded 輸入只在唯一時轉址。UI `fallback_order` 只用介面詞彙，卡文保留所選區原文。

詞彙／語言採[format 2 當前入口](catalog-route-adoption.md#0-詞彙與語言-format-2)，保留 code 與映射檢查、移除收據。
搜尋別名、記號定義、特殊構築名稱及路由覆寫仍用原入口，永久 alias／改號走 §12。
一般版稀有度白名單與繁中缺譯先日文再英文不變；翻譯標籤及記號文案走 §6。

公開的 text_symbol／ruling hints 使用 [傳輸契約 §3.2 的 ParameterSchema](snapshot-transport.md#32-公開參數宣告)，不把建置模板的參數或引用直接投影為公開物件。

所有 config/template URL 限 HTTPS＋具名參數白名單。`language_map` 可擴充，不新增 SVE region。固定圖示 code 指 app shell 自製 SVG；卡片影像清單沒 `card_back` role。

## 9. 批次表記、上下文與非官方條目

wording diff 封套可一次簽 confirmed，但 `sample_ids` 必須列全部 checked 成員；不能用抽樣認定整批全都等義。使用者 2026-10-01 核可的規則集可按 §9.5 完整檢查涵蓋差異，須區分政策檢查與逐組人工核對。scope=record 的三個 batch 欄位皆 null。初始 exact 原文/無差異採機械路徑，省略提醒或共用語義才檢正規化政策。保留 `printing_face_observation` 與 `revision_semantics`，最新表記改顯示、等義 bundle 保持；真規則或 token 依賴改動才重驗 DSL。對同一頁不同時間的更新也先分觀測，不一律當互斥衝突。

`translation_context` 每次建置推導，預設 `semantic_variant=default`；只有採納的同字異義例外才能另配 variant。`translation_use` 釘具體 owner/field/ordinal，`translation_selection` 依 `context/target_lang` 選同模板同參數唯一翻法。不是每張卡任意自由翻；模板/術語更新仍沿 binding 反查。建置資料庫的上下文關係不出貨，卡表快照的 FieldTranslation 指已選 translation.id。

人工限量序號版次使用獨立 [manual-printings-v1](manual-printings.md) 封套：new unlisted printing 與官方版次的 serial_supplement 分開；不造官方 observation／source_face_map。候選不自動 confirmed，信心不代替逐筆人審。官方來源封存釘版，第三方店家只留 URL；後者來源 FK 指完整 authored 封套，不捏造第三方內容 hash。未收錄且未確認一般版對應者只顯示卡號、人工名稱、來源，沒有卡文；既有官方 PR（含 PR-442）照常顯示官方卡文。

維護者 2026-10-03 最新更正：SNC 周年（日英、含 BP20-SNC01）歸 SNC，WB 三張歸 WB，PR-350／PR-442／PR-544 留 PR 且只補序號資料。ANV 等稀有度照卡面原樣存，不作歸檔家族。serial_total 為卡面分母；EN 一周年填 10、註記實際每種一張。無實體商品不造 product，只留 distribution 參考與註記；月年精度保留原樣，不補完整日期。圖片與額外張數／EN 查證各屬 #210／#211。

暫定 `card_no` 不占官方網址；`int_id` 所有出貨 printing 都依其地區號段追加分配（§3.2）。補正 `card_no` 後留下 provisional→official 永久 alias；`int_id` 不變。卡號推算與 card 身分是不同軸，同卡通則不會讓所有 SNC 或 EN 候選自動 confirmed。authored/config 已固定 `third_party_image_policy=mirror_reviewed`、`deck_eligibility_policy=regional_decklog`。每張第三方圖以 `review_decision_id` 連到 confirmed 的來源/圖片確認，保存 `source_url`、內容 hash 與確認決定；換圖/換來源須重新確認，抽樣不代替逐圖確認。建牌資格依該地區/版次的 `decklog_available`；人工查證記來源與日期，未查證依官方卡表收錄狀態預設（詳 [build-db.md](build-db.md) §17.2）。暫定號/身分不阻擋建牌；不可用版次禁止新加入、新分享碼與匯出。舊碼/既有牌組仍開啟保留條目，警告並提示可用同名版次，不靜默刪除。

發布程序維護精簡且耐久的 `(lang,short_id,full_digest)` 文字鍵索引，以全部已發布／已保留鍵檢查固定 16 hex＋lang 碰撞，不能只驗 current＋previous 或重配歷史鍵。此索引及發布收據須備份驗回，不進人工 registry，不含完整歷史卡文；資料來源不再是永久 R2 快照聯集。公開快照只保 current＋previous，詳 [snapshot-format §4.1](snapshot-format.md#41-發布窗口圖片新鮮度與回收)。

### 9.1 current／wording 採納入口 wording-adoption-v1

**wording-adoption-v1 的具體格式（§9.1–§9.7）經使用者 2026-10-01 核可**，含 §9.6 的暫顯選取規則。既有核可政策為：可證 absent 用空字串、表記未定仍顯示、規則清單核可一次後自動分類處理、剩餘群組以 Artifact 逐組／同型態批次確認。格式核可不表示任何群組已採納；§9.5 的具體規則清單與核可收據儲存格式仍由 #145 提交核可，#146 的實際回答與 §9.6 的 semantics 能力例外仍待決。

本節補齊 §9 與 build-db §4 的持久採納輸入，適用同一 face／region 的等義表記；不修改身分 registry v1 或商品封套。公開表記未定的顯示另依 §9.6 與 snapshot-format，不能把它當已採納 current。**格式可驗、觀測全體核對、current 可選、語義能力已實作是四個分開的條件**。本格式不提供真規則變更、來源更正、未核對的顯示偏好或真規則 override 的捷徑，也不把既有 identity_registry 決定當表記決定。

| 路徑（相對 authored 根目錄） | 完整頂層欄位 |
| --- | --- |
| `wording-adoptions/index.yaml` | `wording_adoption_format: 1, kind: wording_adoption_index, includes` |
| `wording-adoptions/<region>/<sequence>.yaml` | `wording_adoption_format: 1, kind: wording_adoption_shard, default_decision_id, records, decisions` |

region 恰為 jp/en；sequence 是只增的三位以上十進位序號。沿 §1、§10.2 的 YAML 限制、單檔大小、路徑安全與新分片排序；includes 只含本表分片，值是完整分片解析後的 §2 canonical JSON Hash。先驗完整 index、全區全部分片與決定，再作地區投影。缺檔、未索引分片、未知欄位／格式、重複鍵、hash 不符均拒絕；新分片寫好才原子換 index，不覆寫歷史分片。正式空集合使用存在且 `includes: {}` 的 index；啟用本入口卻缺 index 不等於尚無採納。

每筆 record 恰有 `record_key, kind, filing_key, data, evidence`；kind 固定 `wording_adoption`，filing_key 等於路徑及 data.region。record_key 是 `["wording_adoption",face_id,region,adoption_no]` 的 §2 canonical JSON **字串**。`adoption_no` 是同 face／region 從 1 起連續只增的整數，不是 face_revision.revision 或來源日期。全域 record_key 及 `(face_id,region,adoption_no)` 唯一；同一面可追加續次採納，不藉重複主鍵替換舊決定。

採獨立入口是為了沿用商品封套的檢查與來源追溯，同時避免把內容選擇混入永久身分或商品內容。只存 hash／識別碼及不含官方卡文的理由；重建文字仍讀凍結來源，不在 authored 複製第二份卡文。

### 9.2 record data 與精確觀測

data 恰含下表欄位；可空欄仍必須明示 null。Hash 一律為完整 `sha256:<64 lowercase hex>`；陣列除另定順序者外按完整 canonical JSON bytes 排序唯一。

| 欄位 | 型別與含義 |
| --- | --- |
| `face_id, region, adoption_no` | 已登錄 face、jp/en、上述正整數；須驗全部 printing 的 face／card／region 一致 |
| `review_context` | 下述完整凍結核對範圍與程式／設定 pin；不只存報告 URL |
| `observations` | 非空的精確觀測物件陣列，按 observation_key 排序；包含該範圍內全部觀測，不只差異或選中者 |
| `observations_hash` | 對完整 observations 陣列套 §2 recipe；與 decision.members 的 hash 分開 |
| `checked_observation_keys` | 恰為 observations 的全部 observation_key，排序無重複；不能只列每個表記的代表版次 |
| `previous` | null、機械初始 current，或前次採納引用，見 §9.4 |
| `equivalence` | 固定 `equivalent`；全部觀測的投影內容及非空 previous 已經人工或核可規則完整核對等義，不由 hash 推論 |
| `review` | 核對方式與規則 pin，見 §9.5；人工與政策檢查不能混稱 |
| `wording_order` | 由舊至新的非空層級陣列；每層為非空 observation_key 陣列，見 §9.3 |
| `order_evidence` | 相鄰層的排序證據陣列，見 §9.3；單層時為空 |
| `selected_observation_key` | 精確指 observations 成員，且位於 wording_order 最後一層 |
| `previous_order` | previous=null 時為 null；否則明示前次選中內容至本次選中內容的先後證據，見 §9.4 |

`review_context` 恰有 `context, source_batches`。context 沿 [F1 §2.2.1](source-archive.md#221-建置輸入紀錄與完整使用閉包) 的完整 `{program_revision,dependencies,configuration}`；source_batches 是排序唯一的 `{batch_id}` 陣列。configuration 的 canonical JSON 內容明列本次 registry、商品內容、商品身分、更正／勘誤等已讀 authored 入口各自的 `{authored_revision,index_path,index_hash}`、解析與投影 recipe、地區範圍及勘誤查核截止日。dependencies 保存所有實際讀取的 index／分片、parser／投影程式與 lock 的 exact bytes hash。不能以 authored revision 代替程式 revision；所有檔案須能從對應 immutable revision 重取。尚未支援的能力明示未啟用，不假造空的已驗覆蓋。

此 context 是**產生待核對觀測時**的輸入，不引用尚未寫出的本次採納分片，避免 hash 自我引用。採納後的建置另釘住 wording-adoptions 入口，見 §9.7。批次包括核對來源與排序證據來源；範圍由釘住 registry 的該 face／region 全部 printing，加上 source_batches 中能匹配這些 printing 的全部卡片來源版本決定。不得挑一頁、刪除缺句版本或先濾不合意內容後宣稱全體；缺來源、解析失敗、身分不匹配均列明並阻止該群組 confirmed。

每個 observation 恰有 `observation_key, printing_id, source_index, source_version_id, raw_hash, parser_version, raw_face_hash, effect_presence, corrections, content_hash`。source_index 為來源面的零起算 UInt，必須吻合 registry.source_face_map；source_version_id 為來源歸檔的 `src:v1:…`，不能用官網 URL、卡號或建置 DB 的流水號代替。raw_hash 須等於 descriptor 的原始 bytes hash；parser_version 為實際 extractor pin，須由 review_context 的程式與依賴重現。

`raw_face_hash` 使用 `wording-face-v1`：對物件 `{name,class,type,cost,attack,defense,traits,title,text,sections}` 套 §2 canonical hash。這些是正式 extractor 的原始面欄位，attack／defense 對應 power／hp；數值保留來源字串，title／text 可為 null，traits 與 sections 保留原順序、每項原字串。其餘純文字欄位保留原值；不 trim、不移除括號、不做 Unicode／空白正規化。萃取器的別名轉接須在 context 釘住，不從 card_no 推欄位。此 hash 不含插畫、稀有度、語音等不參與 current 規則選取的欄位，亦不能取代包含全部面的 registry observation_hash 或 rules_hash。

`corrections` 恰列該觀測所有適用 active 更正，每項為 `{correction_id,record_hash,decision_id,status}`；record_hash 為來源更正完整 record 的 §2 hash，status 僅 `applied/already_fixed`。順序按 correction_id；不得重複或選擇性漏掉適用更正，同欄位互相矛盾即失敗。`content_hash` 對同一 wording-face-v1 物件先依 §9.8 的 effect_presence 判別投影 absent，再套已驗更正後計算；沒有 absent 轉換且無更正時必等於 raw_face_hash。不把更正值改寫為 raw；未知段落類型仍是 unknown，不藉此 hash 自動分類 reminder／token_definition。

observation_key 是 `["wording-observation-v1",printing_id,face_id,region,source_index,source_version_id,raw_hash,parser_version,raw_face_hash,effect_presence,corrections,content_hash]` 的 §2 canonical JSON **字串**。同一 printing／face 的不同來源版本都保留；同來源換 parser 或更正採納也不會誤命中舊收據。僅 raw hash 相同不足以合併來源；觀測物件、checked 全集合與排序均包含在完整 record hash 內。

record.evidence 沿 §10.3 的 `{batch_id,source_version_id,locator,role}`，非空、無重複，逐項驗 sealed batch／descriptor／first receipt／raw 閉包。effect_presence 的完整 result／hash 沿 §9.8，參與精確觀測鍵與重建；尚未核可或實作的 parser 不得用固定 absent 填值。每個 observation 至少有 role=`wording_observation` 的對應證據，locator 是 `{printing_id,face_id,source_index}` 的 canonical JSON 字串。來源可共用，角色與定位不丟失；舊 current、更正與排序的依賴須同樣能追回完整來源閉包，不能只留下 hash。所有 evidence 的批次均須列於相應 review_context。

### 9.3 明示順序與選定觀測

wording_order 恰好分割 observations 全集合，各 observation_key 出現一次。**同層所有 content_hash 必須相同，且重建內容須 exact 相同**；層內按 observation_key 排序只供穩定呈現，不表示時間。最後一層的 selected_observation_key 明示選中的 printing／來源版本；同內容的不同來源選一筆作代表不等於刪除其他 printing_face_observation。

不同內容必須分層；每組相鄰層恰有一筆 order_evidence，欄位為 `{before_level,after_level,basis,evidence_indexes,review_receipt}`。level 是零起算 UInt、after=before+1。basis 為 `printing_availability/source_update/reviewed_order`：前兩者的 evidence_indexes 為本 record.evidence 的非空唯一排序索引，分別釘正式 product／printing_product 的可信發售／收錄日期，或可驗的官方更新證據，review_receipt=null；證據必須支持該相鄰層先後，不只指任一相關頁。

**使用者 2026-10-01 核可**：按可信發售日排序；未知日期的 PR 等版次另列詢問使用者。同日、month/year/unknown 或多重收錄不足以判斷時，不以 date_raw 字典序、卡號、hash、fetched_at 或分片順序補年代。對未知／並列部分，reviewed_order 保存使用者明示的**採納順序**，不聲稱官方先後：evidence_indexes=[]，review_receipt 恰有 `{before_observation_keys,after_observation_keys,note}`；兩組 keys 恰等於該相鄰層，精確內容已在 record hash 中。收據保存兩組觀測及順序說明，不保存回答者／時間，不能從等義回答自動生成採納順序。沒有回答仍報 unresolved_wording_order，但按 §9.6 顯示候選，不排除卡片。

只依可信發售／更新證據選取時，basis=latest_adopted_wording；wording_order 或 previous_order 含任一 reviewed_order 時，face_current.basis=reviewed_override，decision 指本次精確封套。這個例外只核可同面同區、已證等義表記的採納順序，不授權真規則 override。順序不填 effective_from／effective_until，不改未知日期；正式勘誤仍依生效區間處理，不能以等義收據接受真規則變更。§9.6 未採納觀測的暫顯也不是此 reviewed_override。

### 9.4 前次 current、續次採納與重建

previous 是封閉的三選一，不讀當前 dist DB 猜前次值：

| 形式 | 完整欄位與驗證 |
| --- | --- |
| null | 僅 adoption_no=1 且沒有可用舊 current；不得藉此切斷已存在的採納鏈 |
| `kind: mechanical` | `{kind,review_context,observations,observations_hash,selected_observation_key}`；僅 adoption_no=1，以上欄位沿 §9.2；從釘住的舊輸入重跑 build-db §4 機械初始 current，全部候選內容 exact 相同、來源完整且無未解勘誤／更正，selected 必須匹配重建結果；沒有人工 decision |
| `kind: adoption` | `{kind,record_key,record_hash,decision_id}`；指同 face／region 的 adoption_no−1，完整 record 與 confirmed 封套必須存在於已釘住 includes，hash 及 decision.members 都匹配 |

重建先按 adoption_no 逐步驗前件，再從每次釘住的 raw／parser／correction 重現 observations 與選中內容；DB revision ID 只作重建產物，不是 previous 的採納證據。機械根的完整觀測集合可自舊來源重建，不能只存一個 text hash。前件選中內容即本次核對基準；若不在本次來源集合，確認頁仍須加列前件原文與差異，不能宣稱本次 observations 自動涵蓋它。其原始來源與程式 pins 留在前件 context，不要求用新 parser 冒充舊結果。

previous_order 恰有 `{basis,evidence_indexes,review_receipt}`，basis 為 `same_content/printing_availability/source_update/reviewed_order`，另必填可空 review_receipt。same_content 要求兩次選中內容 exact 相同，evidence_indexes=[]、review_receipt=null；發售／更新兩種 basis 沿 §9.3 的非空證據索引機制，證明本次選中表記晚於前次，不能只驗本次候選內部順序便回退 current。前件不在本次來源集合時亦須保留這項比較；reviewed_order 的收據沿 §9.3，但 before_observation_keys 恰含前件選中鍵、after 恰含本次選中鍵，明示使用者選擇的採納順序；採用它時 current.basis 亦為 reviewed_override。沒有證據或明示回答就暫不追加採納，仍可公開候選。這不是由 adoption_no 推論表記年代。

同面同區是一條連續鏈：缺號、跳號、分叉、循環、前件跨面／跨區或同號不同內容均為輸入錯誤，整筆交易失敗。全體 checked 與 equivalence 同時涵蓋這次 observations 及前件選中內容；任何未核對新增觀測、更正或選取變更，另寫下一次採納、新 record hash 與新 decision，不原地修改舊分片。此續次格式只適用表記採納，不擴張 §10／§11 或 identity_registry 的修復規則。已採納判斷事後發現錯誤時隔離受影響結果，走來源更正／勘誤／身分修復，不能以等義續次記錄掩蓋。

新的建置先重建既有採納，再比對本次全部觀測。新增版本不繼承 checked；即使 raw 內容相同，新來源鍵仍是未核對候選。原收據保留為歷史 confirmed；沒有新採納時保留仍有效的舊 current 並報候選。前件依賴缺失／hash 不符是不可重建錯誤，不能退回任取現存觀測；已知更正／勘誤或身分衝突使舊 current 不再有效時也不能繼續當確定事實。

### 9.5 核可規則、confirmed 封套與人工確認

本節的 wording 等義授權不替代[模板參數辨識政策](template-parameter-policy.md)；兩者重用 canonical／不可變 pin 做法，但 kind、收據與授權範圍各自獨立。

**使用者 2026-10-01 核可**：空白、標點、提醒文、句型等差異由程式分類，具體規則清單核可一次後，僅對規則完整涵蓋的差異套用；其餘群組在 Artifact 確認頁逐組確認，也可對同一變更型態明示批次確認。下表定義分類邊界，**不是已核可的替換／等義規則清單**；具體 matcher、反例與適用範圍由 #145 提交使用者核可。

| 分類 code | 可分類的差異 | 不可由類別名稱推得的結論 |
| --- | --- | --- |
| `whitespace` | 空白、換行、排版差異 | 不能刪除會改變詞界、數字或條件的空白 |
| `punctuation` | 標點、括號、字形差異 | 括號可能是規則條件，不能一律略過 |
| `reminder` | 已識別的提醒文增刪／改寫 | 未識別段落或 token 定義不能冒充 reminder |
| `section_layout` | 主文與 sections 分段、段落順序差異 | 分段相似不表示條件／適用對象未變 |
| `terminology` | 同一地區的術語替換／改名 | 不跨區推翻譯等價，也不自動改卡名或數值 |
| `sentence_pattern` | 有限、具名句型的改寫 | 不能用自由改寫模型或相似度涵蓋新句型 |
| `uncovered` | 其餘改字、缺句、數值／種類／特性／token 依賴差異，或規則互相衝突 | 留人工確認／勘誤判讀，不自行等義 |

每條待核可規則必須列唯一 rule_id、類別、適用 region／欄位／段落、精確匹配條件及排除條件、有限變換、正反例，並明示 action=`classify_only/equivalent`。只有使用者核可 action=equivalent 的規則才能讓命中差異免逐組再問；僅分類的規則仍留人工佇列。規則集以 `{policy_id,authored_revision,path,hash,approval_receipt_hash}` 釘住不可變內容與核可收據，path 為 repo 相對路徑，hash 為 §2 canonical 內容 hash，收據 hash 同 recipe；實際檔案 bytes 另進 F1 dependencies。具體儲存格式隨 #145 一併審核，未取得收據以前不啟用自動採納。

**wording-rule-policy-v1 儲存格式**：格式由協調者依使用者授權採用；使用者 2026-10-02 核可的 action 只有 `wp:eol-v1`（僅 CRLF／LF）為 equivalent，其餘六條為 classify_only。這是規則政策核可，不是任何逐卡等義或採納順序回答。

沿 authored 的 YAML 慣例，政策與收據配對存於 `authored/wording-rules/<version>.policy.yaml`、`authored/wording-rules/<version>.approval.yaml`；首版為 `145-v1`。`review.rule_set.path` 指政策檔的 repo 相對路徑，收據路徑以同一版本將 `.policy.yaml` 換成 `.approval.yaml` 推得，不提供可任意改指的第二個路徑。

| 封套 | 必填欄位 |
| --- | --- |
| policy | `rule_policy_format: 1`、`kind: wording_rule_policy`、`policy_id`、`common_boundary`、`rules` |
| common_boundary | `comparison`、`protected_fields`、`protected_condition`、`missing_text`、`sections`、`output`、`equivalence`；保存全體規則的共同比較、保護欄位、null、段落與採納邊界 |
| rules 每項 | `rule_id`、`category`、`regions`、`fields`、`matcher_version`、`parameters`、`action`、`match_condition`、`exclusions`、`finite_transform`、`examples` |
| examples | `positive`、`negative` 非空陣列；每項恰含 `case_id`、`region`、`before`、`after`、`parameters`；before／after 是完整合成 wording-face-v1 物件，case_id 在該規則內唯一 |
| approval | `rule_approval_format: 1`、`kind: wording_rule_approval`、`policy_id`、`rule_set_hash`、`rules`、`note` |
| approval.rules 每項 | `rule_id`、`action`；按 rule_id 排序唯一，恰與 policy.rules 相同 |

封套欄位封閉；policy.rules 依 rule_id 排序唯一，regions／fields 保留已核可範圍。examples.parameters 是固定測試輸入，不能擴張正式 rules.parameters（首版 reminder ordinals 與 term pairs 皆空）；危險正例仍可分類，action=classify_only 不因正例而升為 equivalent。例子只可用合成文字、hash 或位置，不保存官方卡文。

對**完整解析後的 YAML 值**（包含共同邊界、匹配／排除、有限變換、正反例、parameters 與 action）套 §2 canonical JSON 與 SHA-256，得到政策 hash；approval.rule_set_hash 必須等於它。收據對完整解析值套同一 recipe，得到 approval_receipt_hash。YAML 排版不是此 canonical hash 的一部分，兩個檔案的 exact bytes hash 仍各自列入 F1 dependencies，並從 rule_set.authored_revision 的 immutable Git 內容重取。policy_id 與 action 清單逐項一致；不能只釘住名稱、README 或 matcher_version。day 精度沿 §2 保存 UTC 當日零時。

每個 matcher_version 與政策中的同一組固定正反例綁定，程式测试逐例核對 region、完整欄位、有限參數及預期是否命中；另驗共同排除與僅分類不能採納。更改 matcher 行為、条件、排除、參數、案例或 action 必須建立新 version／policy_id／檔名及新的實際核可收據；不能沿用版本名或舊核可。消費端拒絕未知 matcher／未支援的等義 recipe，不以類別名或相似度補判。已合併政策與收據不可原地覆寫；更正或擴張採只增續版，保留舊內容與 Git revision，使歷史採納仍能重建。

record.data 新增必填 `review`，恰有 `{mode,rule_set,rule_matches}`；mode=`human/approved_rules`。human 的 rule_set=null、rule_matches=[]；approved_rules 的 rule_set 是上述完整 pin，rule_matches 為排序唯一的 `{from_observation_key,to_observation_key,rule_id,field,before_range,after_range}`。range 是兩個 UInt 的 `[start,end]`、start≤end，表示原始 Unicode code point 的零起算半開區間，sections 的 field 用 `sections/<ordinal>` 定位；前件比較以 `previous` 這個保留字指前件選中觀測。每個不同內容與選中觀測、以及非空前件的完整 diff 都須重算，所有差異區段恰被核可 equivalent 規則涵蓋；重疊、未覆蓋、跨欄位未知差異、parser／規則 pin 不符即回人工，不任取某條規則通過。原文／sections 原樣保存，規則只提供採納依據，不在 raw 或公開顯示中刪字。

每檔 records 非空，共用一個 default_decision_id，decisions 恰含該決定。決定欄位與 hash 計算沿 §10.4；scope=`batch`、category=`wording_adoption`、policy_id=`wording-adoption-v1`、state 固定 `confirmed`。members 恰為本檔全部 `[record_key,完整 record 的 semantic_hash]`；membership_hash 對排序 members 計算，id=`d:<完整 membership hash hex>`。semantic_hash 是歷史欄位名，不表示工具已證明語義相同。

sample_ids 恰為全體 checked record_key，各 record 的 checked_observation_keys 恰為全部精確觀測；human 表示人逐組／逐型態核對，approved_rules 表示依核可政策逐觀測完整機械檢查，**不能把後者宣稱為逐卡人工審閱**。兩種 mode 分檔；同檔 approved_rules 使用同一 rule_set。human／approved_rules 的依據由 mode 與政策內容區分，決定不保存姓名／時間；approved_rules 的 note 明示「政策核可」。不捏造使用者此次逐組點擊，也不以舊身分／商品決定代簽。報告分開計 `human_rows`（mode=human）、`approved_rules_rows`（mode=approved_rules）與仍待人工的群組。

只對等義且可表示、排序已解的群組追加採納；未回答、勘誤、不確定、未涵蓋或待排序者留 authored 外的精確收據／診斷，不先寫 proposed 再覆寫。null 主文未能依 §9.8 證明 absent 者仍 deferred，不能以等義回答補造缺文。

**確認頁方向（#146 實作時再細化，不屬於本格式驗收條件）**：每列為 face／region 群組，顯示全部 printing 原樣卡號、精確來源、前件與各候選的逐欄差異、段落邊界、更正狀態、日期及精度、分類與規則命中／未涵蓋區段。Artifact 從受控凍結來源顯示原文；官方卡文不進 git 或 PR。依可信發售日排審，日期未知的 PR 等版次獨立列出詢問順序，不以 fetched_at、卡號或 hash 補年代。

選項為「意思相同／勘誤／不確定」，預設未回答。「勘誤」是待判讀意見，不直接建立官方 errata、來源更正或虛構生效日。同一變更型態可批次確認，但提交前須明示精確成員、完整變換與例外；批次回覆只涵蓋這些成員，不自動成為可套未來卡片的規則。逐組與批次收據都由工具展開到全體 checked；使用者不用填 hash、parser、YAML 或日期格式。未涵蓋的語義差異及未知排序分別提問，不把同意語義等價當成同意某個年代。

### 9.6 未採納表記的顯示與來源更正

**使用者 2026-10-01 核可本節暫顯規則**：沒有舊 current、但表記有差異的卡照樣顯示，不因待採納排除。正式／preview 採相同邊界：`face_current` 只表示已採納 current；暫時顯示的觀測另由公開 `face.wording` 投影，不能寫成 latest_adopted_wording 或虛構 decision。完整公開形狀、候選引用與分片規則見 [snapshot-format §2.3](snapshot-format.md#23-表記未定的公開呈現)。

| 情況 | 行為 |
| --- | --- |
| 初始全體 exact 相同、主文可表示且通過既有完整性、勘誤／更正檢查 | 沿 build-db §4 機械 current；不要求逐卡 authored 記錄 |
| 未核對差異或順序未解，但舊 current 可重建且仍有效 | 保留舊 current；公開標「表記未定」，各版次仍顯示自己的觀測，另列未採納候選 |
| 有差異、無可用舊 current | 各 printing 顯示該版次可表示的觀測；face／card 摘要按下述發售日規則暫顯並標「表記未定」，不能決定時列候選；手動使用，不整卡排除 |
| 已知更正／勘誤衝突使舊 current 無效 | 不再當有效 current；觀測保留且標明衝突，已知有誤的內容不當無警告摘要或規則自動執行依據；按既有更正／勘誤閘門處理 |
| 正式輸入缺檔、hash／成員不符或前件不能重建 | 輸入驗證失敗，回滾；不得當普通待採納而略過 |

暫顯優先使用仍有效的 current。沒有 current 時，由正式 product／printing_product 的可信發售／收錄日選**已知完整日精度日期中的最新版次**；多重收錄須能證明該 printing 的首次取得日；若其他未知日期收錄使首次日無法判定，該 printing 也列日期未定，不用後來再錄商品日期讓同一 printing 變新。最大日期的觀測均可表示且無更正衝突，並只對應一個 exact 內容，顯示該內容並標「依已知發售日暫顯、表記未定」。同內容的代表 revision ID 只為去重定位，不宣稱某來源年代更晚。最新版次本身有多種尚未定序內容、同日最新版次內容不同或最新日仍有主文待確認／更正衝突、或沒有完整日精度日期時，display 為空、列出全部候選，不任選。日期未知／月年精度的版次另列，不因較晚抓到而排在最後；即使另有已知日期可暫顯，也明示這些版本尚未參與排序，交 #146 詢問使用者。

printing 的觀測不是印刷原文：保留自己的 revision／sections 與來源，不拿 face 暫顯或 current 覆蓋，也不提升 printed_text_state。只有主文 null 且無法證明 absent 的觀測仍在 report／F1，公開只顯示文字待確認狀態，不造 revision 或借別版主文；同卡其他可讀觀測照常可見。雙面卡保留兩面及各自狀態，不因一面表記未定刪掉另一面。

沒有已採納 current 的區域顯示觀測只供查閱／手動，card_engine_support 的 region_blocks 加 `wording_pending`，不得共用尚未確認的 DSL／翻譯或放行自動操作。有舊 current 的情況仍依既有來源 freshness／衝突閘門判 DSL，不把新候選當已通過。引用仍須在同份快照完整閉合；候選為 null 的位置明示可空，不指向未建的 revision。#144 的診斷排除閉包不構成發布閘門，不能據此排除表記未定的 card／printing。

來源更正先於候選可用性判斷：#30 的 active 決定仍獨立驗原值／觀測 hash／圖片證據，成功 application 後再計 content_hash、diff 與顯示／採納候選。原始 printing_face_observation、raw_face_hash 與更正後 revision／application 分開。已採納更正不重問；needs_review 不套用，conflict 不當可信摘要，already_fixed 不重複替換並警告退役。更正清單／內容／狀態變更須重驗核可規則或新人工收據，不能沿用舊 checked；全體 exact 相同且通過其餘閘門可走機械 current。更正不自動產表記順序，未套更正的 census 不是最終待問數。

本格式保留 [實作分期](implementation-tiers.md) 的等義能力閘門：消費等義採納、建立 semantic_id 或重用 DSL 仍須 semantics 子組及完整依賴。上述暫顯不消費等義採納，無須先啟用 semantics；report-only 的診斷也不能決定公開範圍。是否另准許正式採納 current 而延後 semantics 仍需另行決定，不以本次「可讀觀測」的核可代替。

### 9.7 建置追溯與驗收邊界

建置 configuration 的 `wording_adoption` 保存 `{authored_revision,index_path,index_hash}`；dependencies 保存完整 index／所有歷史分片的 exact bytes hash，實際檔案須符合 immutable revision。每分片建立 authored source_record，parser_version=`wording-adoption-v1`，決定以 decision_source 指回封套與全部原始證據。共用 raw 的 source_record.parser_version 仍為 null；官方觀測／revision 的 source_id 仍指 raw，採納的 face_current.decision_id 與相關 wording revision 的 decision_id 指 confirmed 封套。更正 revision 仍保留來源更正決定，不以 wording 決定覆蓋其 provenance。

F1 使用紀錄分別保存 `wording_observation`（實際 extractor pin、精確 printing／face／source_index locator）、`wording_order_evidence`（實際日期／更新解析 pin 與定位）與 `wording_evidence_closure`（僅閉包驗證，parser=`archive-closure-v1`）。更正／身分／商品的實際用途另外保存，不因共用 raw 吞掉；歷史前件 context／用途亦須驗回並納入本次實際使用。decision_source 同 source／role 多定位時，locator 保存排序唯一的 canonical JSON 定位陣列，避免其複合 PK 丟失定位；quote=null，不複製官方卡文。

輸出前從釘住 registry、批次、採納鏈與證據獨立列 expected uses，再驗 F1 完整閉包、metadata 及四檔 bundle；報告保留本次選用 record_key／record_hash／decision、selected_observation_key、前件與未採納候選。重跑須由相同 pins 得到相同採納／暫顯選取與待確認狀態，不依工作目錄、檔案迭代順序或舊 DB 狀態。

匯入器至少驗：漏一觀測／checked 成員、同來源不同版本、錯 face／region／source_index、同字不同來源、同日／月年／未知日期無證據或明示順序回答、選到非末層、同層內容不同、前件缺失／分叉／機械根不能重現、新觀測不能繼承舊核對、active／already_fixed／conflict、absent 證據／unknown 主文、雙面候選顯示與公開引用閉包、核可規則未涵蓋差異必回人工、F1 缺用途／錯 pin，以及等義能力尚未啟用。格式文件與 hash-only 盤點都不是上述實作驗收，也不是任何真實群組的人工採納。

### 9.8 無主文的證據 recipe（採用 B）

**使用者 2026-10-01 核可 B**：能證明「確定沒有效果主文」的面使用 exact 空字串，不能證明者維持 null、留 report／F1。這沿用 build-db 已有的「空字串＝已確定無文字」語意；新增的是判別與追溯 recipe，不放寬任何 NOT NULL 欄位，也不新增公開 effect_state。absent 只表示該來源版本的主文欄位沒有文字，不表示卡沒有能力；sections、種類／特性／標記仍保留，不因 absent 放行自動規則。

`effect-presence-v1` 的結果是建置期證據，不是新的 DB 表或公開欄位。每筆恰有 `{recipe,source_version_id,source_index,parser_version,template_id,container_locator,state,reason_code}`，recipe 固定 `effect-presence-v1`；source_index 是該來源面的零起算 UInt，parser_version 與 template_id 必須由 F1 程式／依賴 pin 重現，container_locator 是非空、可重現的來源區塊定位（缺容器時定位應在的父區塊）。state／reason_code 配對固定如下：

| state | reason_code | 判別要求與結果 |
| --- | --- | --- |
| `present` | `nonempty_container` | 完整已識別頁面、正確 face 的主文容器有內容，原字串保留；空白本身不能被本 recipe 刪成空字串 |
| `absent` | `empty_container` | 完整已識別版型中主文容器存在、確實無文字或承載規則的子節點；允許的純排版節點由該 template_id 的 parser recipe 明列，不能略過未識別圖片／icon |
| `absent` | `template_omits_empty_effect` | 完整已識別版型依已驗結構契約在無主文時省略容器；所有必需區塊／face 均完整，能排除截斷與選錯面，不能僅因查不到 selector 成功 |
| `unknown` | `unrecognized_template` | 版型或 face 文字區辨識失敗；template_id=null，保留可定位父區塊 |
| `unknown` | `incomplete_source` | 頁面／面或必需區塊不完整，不能推 absent |
| `unknown` | `ambiguous_container` | 多個可能容器、未識別子節點或不符合已知省略規則，不能任取／略過 |

除 unknown 的 template_id 可明示 null 外，所有識別／定位欄位非空；每種 parser 的版型簽章、容器定位與省略條件須釘住、附獨立完整／截斷／錯面／未知節點反例，#145 實作時驗收。text=null、sections=[]、卡種是 follower 或另一版次沒有文字，均不能代替上述來源證據。若需核圖／補來源才能判斷就維持 unknown，不要求使用者對全部 null 觀測猜答案。

對完整結果物件套 §2 canonical hash；觀測的 `effect_presence` 恰有 `{result,result_hash}`，result_hash 須重算匹配。raw_face_hash 仍含 extractor 原值（包括原始 null），不改寫凍結 raw；只有 absent 的可重現證據才在建置投影將主文表為 `""`，再套已驗來源更正並計 content_hash。present 保留 exact 主文；unknown 保留 null，不建立空文字單元或引用其他版次。原先的「無更正 content_hash=raw_face_hash」只適用沒有 absent 投影轉換者。effect_presence 與其 hash 必須包含在 observation_key／record hash 及採納 freshness 中，不能按 card_no 沿用判斷。

F1 新增 usage=`effect_presence`，parser_version 為實際判別 parser pin；locator 為 `{printing_id,face_id,source_index,recipe,template_id,container_locator,state,reason_code,result_hash}` 的 canonical JSON。原始來源 metadata 與 archive pin 依 F1 保存，raw source_record.parser_version 仍為 null。所有 present／absent／unknown 的已讀來源均列此 use，未知／deferred 也不能漏掉；報告保存完整 result，輸出前從凍結來源獨立重跑判別並核對 result_hash／用途閉包。

建置維持 `face_revision.effect_unit_id`、`printing_face_observation.revision_id`、`text_unit.text` NOT NULL。absent 可引用依既有語言＋exact 空字串 recipe 產生的 text_unit；來源證據留引用端／F1，不掛在共享空文字上。unknown 暫不建其 revision／觀測表列，report 明列 total／materialized／deferred 與理由。公開 effect_unit_id 仍非空；已證 absent 顯示無主文並照常呈現 sections。公開候選無 revision 時使用 §9.6 的明示待確認位置，不把 null 傳進 face_revision.effect_unit_id。

**A 未採用**：新增三態 DB 欄與 nullable effect FK 可讓 unknown 進 revision，但目前保存在 report／F1 已足夠，B 可沿用現有儲存與公開文字語意。A 若在候選 format 1.0.0 期間實作，依 [snapshot-contract](snapshot-contract.md) 可直接同步 Schema、descriptor、golden 與 reader，**不必因此升格式版本**；正式凍結後才另判相容性。B 本身不要求更動 DDL／公開 Schema；§9.6「表記未定」的公開欄位擴充則是另一項已核可顯示政策的實作工作，不能混算為 B 的成本。本 PR 只改文件，沒有修改上述機器契約或宣稱 absence 判別器已完成。

## 10. 商品人工輸入 product-authored-v1

本節定義歸檔類別、人工商品與人工收錄的持久輸入；格式定案不等於候選資料已確認，也不表示匯入器已完成。官方來源直接萃取的商品觀測仍屬凍結來源，不需要把所有觀測抄成 authored。

### 10.1 資料規則與使用者決定

**使用者決定（2026-09-30，規格變更）**：依 [build-db §3.2](build-db.md#32-商品與發行) 與 [§15](build-db.md#15-網址搜尋預設版次與記號)，卡包瀏覽與搜尋改以已登錄的歸檔代號 home_set_id 為主，限定全站目前選定的 JP 或 EN 版本，兩區結果不混。`/sets/{code}` 依歸檔代號與地區列卡；商品名稱、發售日、合併包與收錄作單卡頁補充資訊及連結，不驅動卡包（`set=`）瀏覽與搜尋；初收錄 facet 另依 §15 的協調者決定。以下明列本次變更與保留的資料限制：

- **本次變更**：`product_family` 保留為歸檔類別，人工確認 code、public_code、kind 與 name。**日英家族串連為選填**，不同區商品仍各自成列，`product.family_id` 可留空，不必為 PCS01 撞名或 EN Combined Set 強制配家族。家族關係沒有填寫不阻擋瀏覽，也不要求先完成這項人工確認。
- **保留限制**：`home_set_id` 是固定歸檔 owner，不能當實際商品收錄證據，再錄不搬 owner；允許單純依已登錄的歸檔代號篩選，不由卡號前綴推商品收錄。
- 歸檔 owner 代號可能跨區撞名，例如本批 `home_set_id=PCS01` 同時歸檔 51 筆 JP 公主連結版次與 3 筆 EN Summer Edition 版次；此時該 family 只代表歸檔類別，名稱與 kind 由人工確認，各區 `product.family_id` 依實際商品另行判斷（可以不同或為 null），不因共用 owner 就視為同一商品家族。
- `product` 是真實商品，`printing_product` 才表示實際收錄；不由卡號前綴、owner、兩區同名或去除 EN 後綴建立商品／收錄／跨區關係。日英家族串連為選填，真實商品的 `family_id=null` 是可接受的輸入，不要求強制補家族，也不能造家族 placeholder。
- PR 可以只是一個歸檔集合，不能假造整批 PR 的商品或發售日。只有 `day` 填完整日期；`month/year` 保留原字串、完整日期為 null，不能補一號。`unknown` 不猜日期。
- 收錄與 printing 的 region 必須一致；收錄的 `first_available_precision=null` 表示沿用商品日期，`unknown` 表示明示未知的覆寫，兩者不可混用。未知商品日期不阻擋已知卡文的展示。

商品與收錄可依凍結來源中可驗證的線索匯入，供單卡頁顯示；來源明示的欄位與版次收錄不必先寫成人工商品封套，或先取得日英家族串連的決定。下列 product-authored-v1 封套仍用於人工維護的商品／收錄與歸檔類別；只有這些人工輸入需要相應採納決定，不能把官方萃取觀測冒充 confirmed 人工決定。缺少日期或配布方式證據時沿用既有未知／待核對規則，不按 owner 補值。

官方萃取商品的永久 ID 另由 [§11 商品身分對照](#11-官方商品身分對照-product-identity-v1) 取得；確認 ID 對照不等於採納本節的人工商品內容。

### 10.2 新定的路徑、索引與封套

以下是 **product-authored-v1 新定的格式選擇**；不修改 `ids/index.yaml`、`registry_shard`、永久配號或身分決定的內容。

| 路徑（相對 authored 根目錄） | 內容 |
| --- | --- |
| `products/index.yaml` | 獨立商品入口：`product_authored_format: 1, kind: product_index, includes` |
| `products/family/<filing_key>/001.yaml` | `product_family` 記錄 |
| `products/product/<filing_key>/001.yaml` | 人工 `product` 記錄與選填的家族關係 |
| `products/inclusion/<filing_key>/001.yaml` | 人工 `printing_product` 記錄 |

`filing_key` 僅分檔，使用 `[A-Za-z0-9_-]+`，可以沿用既有 owner；不產生任何家族或收錄關係。無家族商品可用 `unassigned` 分檔，不能據此建立同名家族。檔名採只增的三位以上十進位序號；依 §1 的 512 KiB 目標及單檔嚴格小於 1 MiB 切檔。新分片按 `record_key` 字典序排列，不重排既有分片。

`includes` 是 authored 相對路徑到解析後 canonical JSON Hash 的映射，hash recipe 沿 §2；只允許上述三類分片。不接受絕對路徑、`..`、symlink、重複索引或未索引的商品 YAML。讀取先驗全部商品 index／分片／決定，再作區域投影；缺檔、hash 不符、重複 record_key／資料主鍵都失敗。不將商品分片加進身分 registry 的 includes；商品匯入也不重配 printing 整數。

每個分片恰有 `product_authored_format: 1, kind: product_shard, default_decision_id, records, decisions`；records 非空，同檔記錄只有一種 kind，共用一個決定，`decisions` 恰含該決定。每筆 record 恰有 `record_key, kind, filing_key, data, evidence`，不接受未知欄位。所有可空欄位也須明示 null；省略不是另一種未知狀態。YAML 解析限制沿 §1。

`record_key` 是主鍵陣列的 canonical JSON **字串**：family 為 `["product_family",id]`，product 為 `["product",id]`，inclusion 為 `["printing_product",printing_id,product_id]`。例如 YAML 的 `'["product_family","EXAMPLE"]'`；不用可能相撞的字串分隔符拼複合鍵。`filing_key` 必須等於路徑的該段，但不須等於 family_id。

### 10.3 新定的記錄欄位與證據

下表是輸入欄位；資料語意、enum 及 FK 沿 build-db，不直接儲存建置產生的 text_unit ID。`name` 與可空 `note` 使用 `{lang: Lang, text: Text}`，語言必須在建置的 language 登錄；名稱不可空。原文 exact bytes 保留，不以名稱分配永久 ID；其他語言名稱仍由翻譯流程處理。

| kind | data 的完整欄位 |
| --- | --- |
| `product_family` | `id, code, public_code, kind, name`；kind 七選一：`booster/promo/deck/collaboration/special_pack/special/other`；code 為穩定小寫搜尋碼 `[a-z][a-z0-9_-]*`，public_code 保留人工確認的公開代號大小寫 |
| `product` | `id, region, family_id?, product_code?, name, product_type, released_on?, date_precision, date_raw?` |
| `printing_product` | `printing_id, product_id, first_available_on?, first_available_precision?, first_available_raw?, inclusion_kind, note?` |

family 的 id、code、public_code 各自唯一；已被 home_set_id 引用的家族須保留該 ID。product.id 是人工首次採納時指定的永久非空 ID，全域唯一，不由當下排序／名稱／URL 重算；product_code 不是 ID，也不假定兩區相同。`product_type` 沿 build-db 的 Code，不把家族的七種 kind 偷換成商品型別 enum。 **使用者核可 2026-10-01**：本節人工 product 的 `product_type` 仍必填 Code、不接受 null；官方萃取商品使用 confirmed 家族 `public_code` 精確對應的 kind，無對應則在 DB 留 null 並診斷，見 [build-db §3.2](build-db.md#32-商品與發行)。兩種輸入不共用 nullable 的人工採納規則。`inclusion_kind` 僅 `pack/box/first_edition_campaign/qr_redemption/event_prize/other`，無證據時不得由家族 kind 猜配布方式。

`product.id` 的新定字元限制為 ASCII `[a-z][a-z0-9_-]*`：採小寫字母起首，只允許小寫字母、數字、底線與連字號，避免空白、路徑分隔符及 Unicode／大小寫正規化的歧義；不要求語意前綴，避免把地區或商品代號編成 ID 的解讀規則。格式驗證須對原始字串作完整比對，拒絕非字串或不匹配的值，不 trim、轉小寫或正規化後再接受；`printing_product.product_id` 與 record_key 中的 product 主鍵亦須符合此格式，並與被引用的 `product.id` 原樣相等。格式通過仍須檢查全域唯一與引用存在，不代表已採納商品身分。

`date_precision` 限 `day/month/year/unknown`。day 的 released_on 必須是有效完整 ISO 日期；month/year 必須有非空 date_raw 且 released_on=null；unknown 的 released_on=null，date_raw 可以保留來源的未知描述或為 null。日精度若來源有原字串仍保留 date_raw，不把轉換後 ISO 日期冒充原字串。inclusion 的日期三欄同理；precision=null 時另兩欄皆 null，以免把無覆寫與部分覆寫混在一起。不得從抓取／封存時間補發售日期。

`evidence` 是去重陣列，每項恰有 `{batch_id, source_version_id, locator, role}`。前兩個識別欄釘住 [source-archive](source-archive.md) 的已封存批次與其中來源版本；store 名稱及根目錄由執行端設定；locator 是非空人工定位字串（例如商品區塊序號），role 是非空證據用途。匯入須驗 batch／descriptor／receipt／raw 閉包並由 descriptor 取得原 URL／raw hash，不接受以 URL 或 hash 字串代替實際可驗來源，也不在 authored 存本機絕對路徑。locator 不是可執行查詢語言。

商品與收錄的 evidence 不可空，且須支持該地區、商品及具體版次收錄的主張；卡片頁 products 區的共現只產候選，不能自動宣稱某家族、配布方式或完整收錄全集。純人工的歸檔家族可 evidence=[]，由下節精確成員的人工決定支持；若主張與真實商品對應，仍須該商品的來源證據及人工核對。商品頁 URL 可以是卡片頁記載的線索，但未封存該商品頁就不能宣稱已驗其內容。

### 10.4 新定的決定形式與匯入投影

每個商品分片使用 batch 決定，必須有 `id, state, scope, category, policy_id, membership_hash, members, sample_ids`，可附 `note`。scope 固定 `batch`、category 固定 `product_catalog`、policy_id 固定 `product-authored-v1`；state 限 `proposed/confirmed`。members 為排序唯一的 `[record_key,semantic_hash]` 二元素陣列，恰好包含本檔全部記錄；semantic_hash 對完整 record（含 evidence）套 §2 canonical recipe，membership_hash 對 members 套同 recipe，id 為 `d:` 加完整 membership hash 的 64 hex。default_decision_id 必須指向此 id。

confirmed 的 sample_ids 恰為全體 members 的 record_key 集合（排序、無重複），不是抽查；proposed 的 sample_ids=[]。決定不保存姓名、時間或精度；note 可省略，讀取時視為空字串，若提供則為 Text（可空字串，不是 null）。不能沿用 identity_registry 的決定、把來源頁重複次數當人工確認，或用 confidence 提升採納狀態。

這些欄位是本格式的明示人工採納收據，不新增 product／printing_product 的 DB decision_id 欄。匯入每個分片時以完整 authored revision、分片路徑及 canonical hash 建立 authored source_record；決定以 decision_source 指回完整封套及全部 evidence 的 raw source_record。product_family.decision_id 指該決定；人工 product／printing_product.source_id 指上述 authored source_record，沿 decision_source 可追回核對及原始證據。文字以既有 text_unit 邊界建立後填 name_unit_id／note_unit_id，不另建第二套文字真值。

只有 confirmed 的資料可作已採納人工輸入；proposed 僅列候選診斷，不能補 FK 父列。驗證必須逐項檢查 family／product／printing 的引用、printing 與 product 的地區一致性、既有 owner 不變，以及來源與本次主張相符；同主鍵的官方觀測與人工內容衝突需報告，不任取最後一筆。這些檢查不因 SQL FK 通過而省略。

本版定義初次採納與新增記錄；既有採納記錄的續版／替代選用須另定契約，不能覆寫舊決定、悄悄替換既有分片或追加重複主鍵來繞過它。新增資料另建分片及其精確決定，最後更新商品 index。機器產生的候選表不屬此採納輸入，沒有人工確認時不寫成 confirmed。格式文件、候選產生、實際採納與匯入器驗收是各自獨立的完成狀態。

## 11. 官方商品身分對照 product-identity-v1

**使用者核可（2026-10-01，商品 ID 方案 A）**：官方商品首次分配永久 ID，與可驗來源中的商品識別線索一起持久保存於 authored，建置釘住 revision／hash。此對照只確認「這個 ID 對到這個地區的這個官方商品」；名稱、日期、商品型別與收錄仍由正式 extractor 讀凍結 raw，不宣稱已被人工 confirmed。格式定案不代表個別對照已採納。

### 11.1 入口、封套與雜湊

使用獨立入口，避免擴充 §10 的 `product-authored-v1` kind 白名單或把 ID-only 輸入當成人工商品內容；沿用其封套、canonical hash 與全筆 checked 決定機制。

| 路徑（相對 authored 根目錄） | 完整頂層欄位 |
| --- | --- |
| `product-identities/index.yaml` | `product_identity_format: 1, kind: product_identity_index, includes` |
| `product-identities/<region>/<sequence>.yaml` | `product_identity_format: 1, kind: product_identity_shard, default_decision_id, records, decisions` |

region 恰為 jp/en，sequence 為只增的三位以上十進位序號。單檔大小、YAML 限制、路徑安全、缺檔／未索引檔拒絕及新分片排序沿 §1、§10.2；不改舊分片。includes 只允許本表分片，值是解析後完整分片的 §2 canonical JSON Hash；不納入 `products/index.yaml` 或 `ids/index.yaml`。未知格式／欄位、重複鍵、hash 錯誤均拒絕。先驗完整 index、全區分片、全部決定與證據，再作建置地區投影。

每筆 record 恰有 `record_key, kind, filing_key, data, evidence`；kind 固定 `product_identity`，filing_key 等於路徑與 data.region。data 恰有 `product_id, region, match`；不含 name/date/family_id/product_type 或收錄。record_key 是 `["product_identity",region,match]` 的 §2 canonical JSON **字串**，match 為下節完整物件；不含 product_id，讓同一識別線索不能另配 ID 繞過重複鍵檢查。

同一 `(region,match)` 全域只允許一筆記錄，即使目標 ID 相同也不能重複；同一 product_id 可以有多筆不同 match，但 region 必須一致。product_id 與 §10 人工 product 共用全域身分命名空間：同 ID 必須指同區同商品，不能另作配號池。對照記錄不是 product 父列；沒有正式商品內容與來源仍不得填 FK。

每檔 records 非空，共用一個 default_decision_id，decisions 恰含該決定。決定欄位及 hash 計算沿 §10.4；本格式的 state 固定 `confirmed`，category 固定 `product_identity`、policy_id 固定 `product-identity-v1`。semantic_hash 包含完整 record 與 evidence；members 精確涵蓋本檔全部記錄，membership_hash 與 `d:<64hex>` 可重算。confirmed 表示使用者實際確認，sample_ids 恰為全部 checked record_key；決定不保存姓名／時間，工具不得自行宣稱已核對。既有 family／product_catalog／identity_registry 決定不能代簽商品身分。

**協調者決定（2026-10-01）**：商品身分對照只接受 confirmed 記錄；proposed 或其他 state 一律視為輸入驗證失敗。因 `(region,match)` 全域唯一且分片只增，不能先納入 proposed 再原地升級或追加同鍵的 confirmed。候選草稿一律留在 authored 外，經使用者逐筆確認後才首次寫入正式分片；本限制只適用於 §11，不改 §10 的既有格式。

### 11.2 永久 ID 與可驗識別線索

product_id 完整比對 §10.3 的 `[a-z][a-z0-9_-]*`。首次**建議**用 region 加官方商品區明示代號的小寫，例如 `jp-bp01`、`jp-csd03a`；這只是方便人工核對的命名，不是 extractor 每次重算的 recipe。地區永遠讀 region，不解析 ID。沒有適合代號時由人選未使用的合法 ID，不用名稱／日期／URL hash、候選 `hint-*` 或當下排序配永久 ID。採納後名稱、日期、URL 或官方代號改動皆不重配 ID；也不重用已採納的 ID。

match 是下表三選一的封閉物件；每種只接受列出的欄位，`?` 欄仍必須存在，可填 null。來源區塊是正式 parser 辨識的單一商品區塊，不能跨區塊拼接線索。

| match.kind | 完整欄位與限制 |
| --- | --- |
| `product_link` | `kind, product_url, expansion_code?`；product_url 是區塊中該 region 正式官網商品連結解析後的 URL，expansion_code 是同區塊搜尋連結的官方 expansion 原值；區塊確無該代號才為 null |
| `expansion_link` | `kind, search_url, expansion_code`；僅供區塊沒有正式站商品連結時使用。search_url 為該 region 官網搜尋連結解析後的完整 URL，expansion_code 為其中非空的官方 expansion 原值 |
| `source_block` | `kind, source_version_id, product_block_ordinal`；來源版本沿 source-archive，ordinal 為依 HTML 文件順序列出的商品區塊零起算 UInt。僅匹配此版本此區塊，不跨 raw 版本泛化 |

連結先解 HTML attribute entity，再以 descriptor.url 為 base 解析相對 href，保留原 href 作萃取追溯；解析後的字串 exact 比對。不移除 query、不重排 query、不改尾斜線、不將 dev host 改成正式 host；expansion 依 URL query 解碼一次，保留大小寫與完整複合代號，不切 `BP12-BP13`，不由卡號或 owner 補值。parser 必須驗連結用途與地區：JP 正式 host 為 `shadowverse-evolve.com`，EN 為 `en.shadowverse-evolve.com`；正式商品路徑為 `/products/` 下，搜尋路徑分別為 `/cardlist/cardsearch` 與 `/cards/searchresults`。非 HTTPS、其他 host 或用途不符只保留診斷線索，不當正式商品／搜尋 URL。新增來源版型需更新並釘住 parser，不放寬為任意 query 中有 expansion 就算商品證據。

JP、EN 均讀搜尋 URL query 中名稱精確為 `expansion` 的參數，參數名區分大小寫，不接受 `Expansion` 等別名。依上述 query 解碼一次後，恰好一個非空值才可作 expansion_code；保留空值及重複參數供檢查，不能由 parser 預設丟掉。缺少 `expansion` 參數才算缺代號；出現空值或多個值（即使值相同）均列為歧義，不任取第一／最後值，也不能退回 null 的 product_link。依下述區塊歧義規則處理，必要時使用已確認的 source_block。此解析規則由正式 parser pin 鎖定。

`product_link` 的 URL 與代號兩者都必須相等，null 只匹配缺代號，**不是 wildcard**。因此共用商品頁的 CSD03A／CSD03B 分別有兩筆對照；不得只按 URL 合併。只有 URL 且區塊不能區分多商品時，不採納 URL-only 對照，可用各自 `source_block` 確認該版本的確切區塊，或留待補證據。`expansion_link` 不表示搜尋分類本身就是真實商品；泛用 PR 分類不能建立整批 PR 商品，活動分類／合併名稱的商品邊界不明時仍留診斷。

同一區塊若有多個不同商品 URL／expansion 值，parser 不任選或作笛卡兒積：列出歧義，僅容許經使用者核對的 exact `source_block` 對照辨識該區塊代表的商品；若該區塊其實含數個商品，須先有能分出個別商品的來源及 parser。`source_block` 不用名稱／日期作匹配鍵；raw 改版後即使只是更正名稱，也須為新來源補對照，沿用原 product_id，不能套用舊 ordinal 猜。

evidence 沿 §10.3 的 `{batch_id,source_version_id,locator,role}`，非空、無重複；每項都須驗 sealed batch／descriptor／first receipt／raw 閉包。至少一項證據的 role 為 `product_identity_match`，locator 為 §2 canonical JSON 字串 `{"product_block_ordinal":0}`（數字依實際區塊），正式 parser 須從該來源重現完整 match 與 region；source_block 還須與 evidence 的版本／ordinal 相同。其他 role 可保留人工判斷所需的來源及非空定位。卡片頁的商品區塊可以證明該頁記載的識別線索；只有商品連結不表示已讀／驗該商品頁內容。

### 11.3 首次採納、重建匹配與改址

首次建立時，工具只產候選 ID、match、完整來源與疑難清單；使用者逐筆確認商品邊界與對應後，才另建 confirmed 分片、更新 index。草稿不直接變成正式輸入，通過格式檢查或來源閉包驗證也不是人工採納。

對每個已驗凍結來源中的商品區塊，正式 extractor 產生可用的 product_link；沒有正式商品 URL 時才產生 expansion_link，另可產生該版本的 source_block。以 region 加完整 match 查找所有已驗證的 confirmed 對照，不作 fuzzy 比較；proposed 等非法狀態已在輸入驗證時拒絕：

| 結果 | 建置行為 |
| --- | --- |
| 命中一個 product_id（多種 match 同指此 ID 亦可） | 沿用永久 ID；官方內容及收錄仍由 raw 萃取、另驗來源與地區／printing 身分閘門 |
| 零命中 | 診斷列精確來源、區塊、線索及缺映射原因；不臨時配號，不產該商品與依賴它的收錄列，不影響有獨立有效證據的卡文；報告明列排除，不宣稱商品完整 |
| 命中多個不同 product_id | 商品身分衝突，整筆匯入交易失敗；不得任選第一筆、較新一筆或較短 ID |
| 同 match 多記錄、同 ID 跨 region、證據無法重現 match、缺 raw／hash 錯 | 輸入驗證失敗，整筆交易回滾；不能降成「缺映射」後忽略 |

相同 ID 的多次官方觀測可以去重，但名稱、日期、商品型別等內容不一致時依既有商品衝突規則處理，不以身分 confirmed 當作選內容的授權。同名不等於同商品，同代號跨 region 亦不合併；owner 只作歸檔，不能參與商品識別或補收錄。

URL／識別碼改動、相對連結解析結果變更或原本無 URL 後來補 URL 時，舊 match 保留。新線索零匹配即列待確認；使用者確認仍為同商品後，追加一筆指向**原 product_id** 的別名對照及新 evidence／decision，不改舊 record 或決定。別名直接指永久 ID，不指其他 match，沒有 alias chain。僅名稱／日期更正而 match 不變時直接沿用 ID，不要求重新確認內容。URL-only 對照若出現可驗的 URL 重用／不同商品證據，即列衝突，不因字串相同而放行。

匯入器須在地區投影前檢查完整對照集合：同一 region 中，同一非空 expansion_code 出現在不同 product_id 的 product_link／expansion_link match 上時，發出 warning，列出代號、各 ID、match 與來源定位，提示可能把同一商品誤配為兩個 ID。例如 dev 連結改成正式商品 URL 後，新增對照應沿用原 ID；不同代號的 CSD02A／B／C 不觸發此警告。同 ID 的多筆別名不警告。此檢查只提示人工核對，不自動合併、重配 ID 或升為交易失敗，也不取代既有 exact match 衝突檢查。

本格式只允許首次身分及不同 match 的追加；不提供重新指派已採納 match、刪除舊對照、合併／拆分永久商品 ID 的捷徑。遇到誤配或同一 exact match 被官方重用且無法區分時停止受影響匯入、交使用者決定修復契約，不能自行定義覆蓋優先序。未採納草稿的修正不算永久身分修復。

### 11.4 建置追溯與既有契約邊界

建置明示讀取兩個獨立商品入口；啟用官方商品匯入時不得把缺少 product-identities/index.yaml 當空對照。依 [source-archive §2.2.1](source-archive.md#221-建置輸入紀錄與完整使用閉包) 的既有 F1 context，configuration 的 `product_identity` 項保存 `{authored_revision,index_path,index_hash}`：完整 40 碼 revision、authored 相對入口路徑與 §2 canonical index hash；dependencies 以 repo 相對路徑釘住 index 與所有分片的 exact bytes hash。實際內容須與釘住 revision 相符，不能用 dirty 檔冒稱該 revision。兩種 hash 分別驗實體輸入及封套內容；所有歷史別名分片仍在 includes，不只留最新 match。

每個分片以完整 authored revision、路徑與 canonical hash 建立 authored source_record，parser_version 使用本封套 recipe `product-identity-v1`；decision_source 連回完整封套及全部 evidence 的 raw source_record。這是新增 authored 封套 recipe，不改 F1 raw 共用列的 parser_version=null 規則。官方 product／printing_product.source_id 仍指萃取內容的 raw 來源，不改指身分對照以假裝內容經人工確認；不新增 DB 表或 decision_id 欄。對照到哪個 product_id 可由釘住封套重建。

僅驗證 evidence 閉包的實際使用以 `product_identity_evidence_closure`／`archive-closure-v1` 登錄；為重現 match 而實際解析的使用另以 `official_product_identity`、正式 parser pin 及精確區塊 locator 登錄。官方內容／收錄的 parser 用途仍各自保存；共用 raw 不吞掉不同用途。零匹配、歧義或被 printing 身分閘門排除的區塊也是已讀輸入，仍納實際 uses。輸出前從釘住輸入獨立宣告並驗完整用途閉包，依 F1 保存 DB／inputs／report／seal；來源衝突與失敗不發布半套產物。

商品身分確認不授權更動 family／owner、日期精度、收錄、EN 身分採納或公開快照白名單。家族關係不明可為 null；機器候選鍵只供本機核對。正式匯入器的驗收須包括同 URL 不同代號、無 URL、名稱／日期修正、改址追加、零／多重匹配、錯 region、proposed 輸入直接拒絕、expansion 參數缺值／空值／多值、同區同 expansion 跨 ID 的 warning、封套／來源 hash 錯及 F1 使用閉包缺漏；不能用本格式文件或候選盤點冒充已完成實作。

## 12. 身分修復與決定續版

完整封套、精確成員 hash、merge／split／reassign 的 face／art 移轉與墓碑、來源更新、
confirmed_none／reskin 續版及有效投影順序見 [身分修復與決定續版](identity-repair.md)。
**使用者 2026-10-01 核可**，含指名撤回修復；新增封套不修改 v1 舊記錄或 hash recipe，
後續實作完成前既有工具的拒絕行為不變。
此獨立入口僅處理卡片 registry，不授權 §10 商品內容、§11 商品身分誤配的續版；
§9 表記採納仍使用其已核可的獨立鏈。

## 13. 數位名字與同名瀏覽政策

[名字當前規則](digital-name-policy.md)用可修改的設定與當次完整來源供名，不再要求 approval 配對。
名字取詞、同名瀏覽與真人 digital-links 的資格分開；格式支援不代表來源已齊或已有正式投影。
不在 authored 維護另一份可機械重產的數位官方卡名表，也不以同名瀏覽授予同卡／效果／語音資格。

## 14. 引擎能力與資源身分設定

`authored/engine-rules/index.yaml` 是 `engine-rules/1` 可執行設定，
語法權威位於 `dsl/engine-rules.schema.json`，完整欄位、legacy-jp 私有轉接與
A／B 兩種名稱語意見 [引擎規則能力與資料身分](../dsl/engine-rules-1.md)。
它引用已採納的 title code、規則名稱身分及釘版來源，不新增另一套卡表或永久 ID。
角色不是整卡 construction_role，也不擴大 Standard 構築採納的 scope。

公開設定不放官方標籤；舊 JP JSONL 透過明示 legacy-jp 的同批私有資料解析。
記憶體載入必須顯式傳入規則與身分背景，不能暗讀工作目錄；未知／錯區、
缺必要模板、歧義或 pin 不符都拒絕，沒有官方名稱預設。
本專案在新路徑的可執行原創資料採 Apache-2.0，來源內容的排除仍依 LICENSING。
格式合法不等於執行器支援或資料已採納，實際載入能力由 engine 契約與測試驗證。
