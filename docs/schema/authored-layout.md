# authored 維護方式

身分登錄格式 **v1，2026-09-28 定案**。本文件定案的範圍為永久 card／face／printing、printing 整數編號（`int_id`，依地區分段配號）、日英對應、無對應審核、英文原創插畫、換皮卡，以及本批來源更正。其餘類別仍是提案。建置資料庫語意以 [build-db.md](build-db.md) 為準；本格式不變更出貨契約。

## 1. 路徑與共同格式

| 狀態 | 類別 | 實際路徑或提案 |
| ---- | ---- | -------------- |
| 定案 | 永久卡、面、版次 | `registry/card/<owner>/001.yaml`、`registry/face/<owner>/001.yaml`、`registry/printing/<owner>/001.yaml` |
| 定案 | 整數編號配號／全域入口 | `ids/<owner>/001.yaml`、`ids/index.yaml` |
| 定案 | 英文獨有查核 | `registry/region_mapping_review/<owner>/001.yaml` |
| 定案 | 英文原創插畫 | `registry/art/<owner>/001.yaml` |
| 定案 | 換皮卡 | `registry/card_related/<owner>/001.yaml` |
| 定案 | 本批來源更正 | `registry/source_correction/active/<owner>/001.yaml`、`registry/source_correction/needs_review/<owner>/001.yaml` |
| 已定案（ADR-0011） | 裁定 | `rulings/R-0001.yaml`，維持原格式 |
| 提案 | 身分修復、特殊構築 | `overrides/identities/BP01.yaml`、`overrides/deck-roles/BP01.yaml` |
| 提案 | 其他策展、數位、標誌 | `curation/BP01/001.yaml` |
| 提案 | 模板、詞彙、翻譯 | `templates/BP01/001.yaml`、`keywords.yaml`、`translations/zh-Hant/BP01.yaml` 等 |
| 提案 | 語義差異、DSL、路由、設定 | `divergences/`、`effects/`、`macros/`、`overrides/routes.yaml`、`config/` 等，見後續各節 |

`owner` 是首次歸檔代號，保留大小寫（例如 BP01、DSD01a、PR），不是商品收錄證據。card 採首次配發代表版次的 owner；printing 與配號按自身 owner，跨包外鍵允許。檔名為只增序號，不因新增較早排序的卡而重新分片。每檔 **小於 1,048,576 bytes**，以 512 KiB（524,288 bytes）為目標：以**寫出後的完整分片 YAML**（含封套、decision 的 members／sample_ids）量測，依序裝入不超過目標的最多筆數；單筆就使分片達 1 MiB 時直接報錯。PR 同樣切序號檔，不造單一大檔。

分片內記錄依對應 printing 的 `(region, card_no, variant_key, printing id)` 排序（`card_no` 為原樣字串的 code-point 字典序）：printing、配號、來源更正用自身或所指 printing；art 用第一個 use 的 printing；card、face、英文獨有查核、換皮卡用該 card 所有 printing 中最小的鍵（face 再加 ordinal）；最後一律以 `record_key` 收尾。排序只作用於**同一次寫入的新記錄**：正常追加只排序本次新增、寫到該 `(area, owner)` 的下一個序號檔，舊分片不動，所以同一 owner 的多個分片合起來不保證是全域卡號序。2026-09-28 首次公開前曾一次性全量重新分片（見 §3.2）；此後不再重排。

YAML 固定 1.2 core schema、單一文件、UTF-8；所有鍵必須是字串，禁止重複鍵、anchor、alias、merge key、顯式 tag、非有限浮點及 YAML 1.1 指示。日期字串須加引號；隱式日期仍是字串，不啟用 timestamp resolver。遵循 [DSL 1.0 §11](../dsl/author-syntax-1.0.md#11-載入與錯誤) 的解析邊界。`ruamel.yaml` pure safe 載入後仍要做結構與引用檢查。

## 2. 分片、批次決定與來源

每個分片有 `authored_format: 1`（分片格式未變；`ids/index.yaml` 為 2，見下）、`kind: registry_shard`、`default_decision_id`、`records`、`decisions`。每筆 record 固定為 `record_key/kind/owner/data`；`data` 是該 kind 的資料。配號以外，匯入時將封套的 decision 展開成具體資料表 FK，不另建立 subject 真值表。配號分片的 decision 為 null，decisions 為空。

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
    authored_by: registry-tool
    authored_at: "2026-09-28T00:00:00Z"
    reviewed_by: coordinator
    reviewed_at: "2026-09-28T00:00:00Z"
    reviewed_precision: day
```

這是格式示意，不是額外審核證據。2026-09-28 的採納依使用者整批確認與後續裁決；`sample_ids` 列**全部 checked record keys**，不是抽樣。`reviewed_precision=day` 表示原紀錄只有日期；UTC 日界是可重現的日精度編碼，不聲稱核對發生於零時。該精度保留於 authored 證據，匯入 decision 的 Instant 採此編碼。本批新找出的更正候選用 proposed decision、空 checked 集合與 null reviewer/time，不能冒稱 coordinator 已確認。

hash recipe 固定：JSON 物件鍵排序、UTF-8（不 ASCII escape）、分隔符 `,`／`:`、無額外空白／尾端換行，不正規化 Unicode。先對完整 record（不含封套的 decision 指針）計 semantic hash；將 `(record_key,semantic_hash)` 二元素陣列按 key 排序，再計 membership hash。decision ID 使用完整 membership hash。任何新成員或內容變更都不得沿用舊決定。2026-09-28 的一次性重新分片讓部分 printing 分片合併，這些分片的 decision 依新成員重算 ID／members／membership_hash；審核者、日期與政策沿用原成員的決定（原本就是同一次使用者整批確認），不是新的審核事件，也不保留舊 decision ID。

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

JP 初始分組依全部面同名／同職業／同種類／同數值／同特性，加上人工審閱規則差異的收據；不是只依同名自動採納。EP、SEP、CP03-125/126、ルゥ遵循 build-db §3.1。同欄位但實際卡面規則不同時，收據 `separate_groups` 指定不同群組。雙面的 source_index 是此次全體 checked 的面對應，不是以 ordinal 猜日英面對應。

### 3.2 UInt32

`card_int_id` 的 `data` 僅 `int_id/printing_id/allocated_at`；`record_key` 為 `card_int_id:<printing_id>`。已配發記錄不修改、不刪除、不重用，沒有 decision。型別上限仍是 UInt32（4294967295），可用號段由版本化配號政策 `region-ranges-2026-09-28-v1` 決定，程式唯一定義在 `sve_carddb.registry.allocation`：

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

`region_mapping_review` 的 `data` 為 `card_id/target_region/state/as_of/coverage_scope/coverage_hash/observations`。本批 target_region=jp、state=confirmed_none，coverage_hash 釘當次完整 JP 萃取輸入，observations 釘受審 EN 版次。匯入 source_id 指這份 immutable authored 查核紀錄；不是推論永遠不會出日版。

`art` 的 `data` 為 `id/card_id/face_id/classification/uses/observation`，uses 是 printing_id＋face_id 陣列。本批原創插畫只掛已確認的 EN printing；未確認基準及跨版次同圖組，故 classification=unclassified，不造假 base／alternate 或 JP art。`art_id=null` 的其他 printing 不代表沒有插畫。

`card_related` 的 `data` 為 `id/from_card_id/to_card_id/relation/source_kind/target_printing_id/suggested_count/dsl_id/evidence`。relation=same_rules_reskin、source_kind=authored，三個選用欄為 null。evidence 逐筆記 `role: from|to` 及全部兩端已採納版次的觀測；匯入 decision_source 釘兩端來源。相同換皮卡的普通／特殊 printing 只產一條關係，禁止自指、多目標與反向重複。只能在兩端都有版次且來源驗證仍匹配的地區投影；任一端目前來源改變就停止該地區投影，依 build-db §5 重審。不能繼承原卡 DSL 或構築張數。

confirmed_none 與 same_rules_reskin 的決定不可因追加版次自動擴張。工具與獨立 validate 都逐筆比對 printing 的完整 observation（換皮關係另含 from／to），拒絕缺漏、重複、過期或多餘證據。新版次需要新的 review／relation 決定並釘住新觀測；v1 尚未定義決定續版格式，因此追加既有 EN-only card 或換皮關係任一端的版次會直接失敗並提示重審，不改寫舊決定。

登錄內的 printing.observation 必須與 printing 的地區及原樣卡號一致；有 EN 對應時 target_observation 必須與已登錄 JP 目標的完整 observation 相同，無目標時為 null。art 的 observation 必須對應其已登錄 use，uses 不可重複；換皮關係不可反向成對。這些是既有證據的引用一致性要求；實際來源版本的觀測比對、跨區採納新鮮度及發布投影仍由匯入器另驗。

### 3.4 來源更正

`source_correction` 的 `data` 保存 `id/printing_id/face_id/field/expected_raw_value/corrected_value/expected_source_hash/source_hash_recipe/reason/state/reported_to_official/reported_on/report_url/evidence`。本格式 field 白名單為 effect、card_type，分別映射效果文字與種類；公開 field／值型別依 [傳輸契約 §3.3](snapshot-transport.md#33-公開更正值)，不擴張此 authored 格式的兩欄白名單。expected_source_hash 採前述觀測 recipe，匯入仍須先匹配來源版本，不可直接替換 HTML 原文。

evidence 元素為 `kind: card_image`、`sha256`、官網原樣 `image_src`、region、locator（卡面文字框／種類標記）。匯入以檔案 hash 與 URL 連到 image source_record，再寫 correction_evidence；不把圖片或本機路徑存進 git。BP07-P06 由協調者確認為 active；使用者於 2026-09-28 追加確認 JP PR-114、BP20-P42、BP20-P57、BP20-P67 與 EN BP15-P32EN、CP03-127EN、PR-388EN、PR-442EN，這八筆亦為 active，decision 為 confirmed、reviewed_by=user、日期精度 day。卡圖 hash 與原觀測不變。一般尚未確認的候選仍用 needs_review＋proposed decision，不得套用至 current／規則解讀。來源原始觀測永遠保留。卡圖已支持的同卡判定與來源欄位是否正式套用更正是不同採納事項。

本批八筆候選升為 active 是使用者明示授權的來源更正採納，非一般追加：移至 active 分片、建立涵蓋精確內容的新 confirmed decision、同步更新 index。先前 proposed 分片與收據保留在 Git 歷史及舊批次資料中；其他永久登錄與決定不改動。一般產生工具仍拒絕修改既有記錄，不以自動升級取代人工確認。

建置的 `registry.corrections.project_corrections` 對 active 更正比對原值與完整觀測 hash，匹配才產生更正值與欄位標記；原值已等於改值時回報 already_fixed，不重複套用，後續需警告並退役來源更正；其他差異為 conflict，不套用並阻擋 CLI 完成。needs_review 不產生投影。結果依 printing／face／field 定位，`corrections` 元素使用 snapshot-format 的 `{field, corrected_from, is_corrected: true, reason, source_url?}`；沒有可用官方頁 URL 時 source_url 仍存在、值為 null，不把卡圖 URL 冒充官方頁。此標記不附到共享文字上。

CLI 每次建置均驗證這個投影，可用 `--corrections-output <absolute-derived-json>` 保存更正後的欄位 value、applied／already_fixed 狀態與顯示標記；加 --check 時只比對已存的投影，不寫檔。這是供後續建置使用的欄位投影，不是完整 face_revision、SQLite correction_application 或公開卡表快照；那些匯入與輸出尚未實作。

### 3.5 重跑與新卡包

工具入口為 `uv --directory <absolute-carddb> run python -m sve_carddb.registry`；參數 `--jp/--en/--candidates/--confirmations/--original-art/--receipt/--images/--authored` 全為明示路徑，`--check` 要求現有輸出完全相同。全程只讀本機來源，不讀 manifest、不抓網路。

receipt 是本機審閱收據，JSON 欄位為 policy、reviewed_by、reviewed_on、input_hashes（五個完整輸入檔的 exact bytes SHA-256）、corrections、reskins、separate_groups、art_groups。policy 目前固定 identity-init-2026-09-28-v1。工具不從 confidence 產生 approval；操作者須在完成逐筆核對／本批政策審閱後建立收據。新包先產候選、核對所有面與差異、核圖實質增刪，再建立新收據；新增內容不能沿用舊輸入 hash。任何未核對的職業、種類、數值或英文同卡名稱衝突都失敗。

corrections 元素包含 region、card_no、face_index、field、expected_raw_value、corrected_value、image_sha256、locator、state、reason。needs_review 的 card_type 候選另須明示 `adoption_scope: identity_check_only` 與 `source_correction_status: pending_user_confirmation`，否則拒絕用於身分核對；這兩欄不會提升來源更正狀態，也不改寫原觀測或正式分片。reskins 是 EN 卡號 → JP 原卡號；separate_groups 是 `region:exact_card_no` → 明示分組鍵；art_groups 是已核對同幅插畫的 EN 卡號陣列集合，組間不得重疊、不能跨 card。未列入者各自登錄；本批 CP02-072EN／CP02-P57EN 依同圖不同簽名加工規則共用 art。receipt 不進 git，採納後的永久登錄與其精確成員決定才是維護狀態。新增 package 使用包含原觀測的完整輸入集合，不把變動的舊來源塞進追加工具；舊來源更新另走 source／identity 修復流程。

### 3.6 輸入保存與收據建立

每批在 repo 外的 `SVE_DATA_DIR/derived/registry/<batch-id>/` 建立新的永久目錄；不可覆寫舊批次。保存以下 exact bytes，不重新序列化或排序既有輸入：

| 檔名 | 取得方式與內容 |
| ---- | -------------- |
| jp.jsonl | 取得已核對的 JP 萃取快照；每行 Card 的 number 與完整 faces。一般 JP 萃取由 extract/jsonl.py 產生；本工具不讀 manifest、不執行萃取 |
| en.jsonl | 取得審閱批次的完整 EN 萃取快照；每行同樣符合 registry.inputs.Card。目前沒有正式 EN 萃取 CLI，不可假定重新解析 HTML 能還原舊批次 exact bytes |
| candidates.jsonl | 取得本批已審候選；每行 en_no、category（A/B/C）、jp_candidates（含 jp_no）。新增批次須完整列出 EN 版次，人工確認 A/B 第一候選或 C 無對應 |
| confirmations.tsv | 保存依序追加的人工裁決，欄位 en_no、jp_no、verdict、confirmed_on；後列覆蓋前列，不能重排 |
| original_art.jsonl | 保存人工卡圖比對結果，每行含 en_no、verdict；en_original_art 是插畫確認證據 |
| receipt-original.json | 原收據的 exact bytes 副本；供稽核比對，不能就地修訂 |
| receipt.json | 本次使用的收據；若只補採納範圍，明記衍生自哪份原收據及新增欄位，不變更五個輸入 hash |

舊批次的取得方式是從保存目錄或其備份複製上述檔案，使用 SHA256SUMS 驗證；不依賴 session 暫存檔、個人草稿或重新生成候選。本批先保留原收據與補充 scope 的收據；使用者確認後另建批次，五份輸入 exact bytes 相同，新 receipt 記 active 與 user／2026-09-28，移除不再適用的 pending scope。舊批次搭配確認前的 Git commit 重現。inventory.json 記各檔 hash、大小、來源及補充原因。SHA256SUMS 與收據都需納入批次備份；git 不存官方原文。

新批次依序執行：

1. 取得完整日英萃取與所有候選／人工確認／插畫證據，依 §3.5 核對各面、規則差異及必要卡圖。保留原始欄位，不將候選更正寫回輸入。沒有 EN 萃取器時須先提供可審閱的完整 Card JSONL，不能省略原文或沿用 confidence 當決定。
2. 建立全新 batch-id 目錄，複製五份輸入；以 SHA-256 比對來源與副本 exact bytes。原有卡片觀測若更新，走來源版本重審流程，不能覆寫舊批次。
3. 人工完成核對後，建立符合 registry.review.Receipt 的 JSON：填 policy、reviewed_by、reviewed_on，input_hashes 的鍵恰為 jp/en/candidates/confirmations/original_art，值是 `sha256:` 加該副本的 hashlib.sha256(path.read_bytes()).hexdigest()。corrections／reskins／separate_groups／art_groups 依 §3.5 填寫；無資料則空集合，不能自行沿用上一批批准。
4. 寫入 inventory.json 與 SHA256SUMS，記錄輸入取得方式、收據建立人與範圍。修改收據時另存新檔並保留原件，逐項說明差異；收據本身的 hash 也納入 SHA256SUMS。
5. 將 §3.5 CLI 的五個輸入與 --receipt 全部指向此持久目錄，--images 指本機官方卡圖、--authored 指登錄目錄。先驗證輸入／計畫，完成後用相同命令加 --check 驗證零改寫。既有 EN-only／換皮卡追加需等待續版決定格式，不能透過新收據繞過拒絕。

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

題本/載入結果與 program/source/engine build/policy/測試輸入版本匹配，最新 fail 不能沿用舊 pass。引擎未指定時不冒充 `engine_passed`；公開只投影 support 和 `program_ref`，private raw 報告不出貨；附件准入與 AST 出貨依 [傳輸契約 §3.4](snapshot-transport.md#34-dsl-程式包與版本准入)。巨集作者/兩模型審查/機械三檢查留建置資料庫。

## 8. 更正、路由與設定

`source_correction` 原值/改值/字段/來源 hash/evidence/核對者日期/是否回報完整保存。套用前先比是否已上游修正；不符合 expected/hash 則停用重審。卡表快照只出引用者上的 `corrected_from`/標記/公開理由，不能在去重 text 上全域標更正。

route/default 純推導；authored 只寫 alias、variant `route_override`、`default_printing_override`。canonical 編碼 exact 原卡號，folded 輸入只在唯一時轉址。UI `fallback_order` 只用介面詞彙，卡文保留所選區原文。

公開的 text_symbol／ruling hints 使用 [傳輸契約 §3.2 的 ParameterSchema](snapshot-transport.md#32-公開參數宣告)，不把建置模板的參數或引用直接投影為公開物件。

所有 config/template URL 限 HTTPS＋具名參數白名單。`language_map` 可擴充，不新增 SVE region。固定圖示 code 指 app shell 自製 SVG；卡片影像清單沒 `card_back` role。

## 9. 批次表記、上下文與非官方條目

wording diff 封套可一次簽 confirmed，但 `sample_ids` 必須列全部 checked 成員；不能用抽樣認定整批全都等義。scope=record 的三個 batch 欄位皆 null。初始 exact 原文/無差異採機械路徑，省略提醒或共用語義才檢正規化政策。保留 `printing_face_observation` 與 `revision_semantics`，最新表記改顯示、等義 bundle 保持；真規則或 token 依賴改動才重驗 DSL。對同一頁不同時間的更新也先分觀測，不一律當互斥衝突。

`translation_context` 預設 `semantic_variant=default`；只有採納的同字異義例外才能另配 variant。`translation_use` 釘具體 owner/field/ordinal，`translation_selection` 依 `context/target_lang` 選同模板同參數唯一翻法。不是每張卡任意自由翻；模板/術語更新仍沿 binding 反查。建置資料庫的上下文關係不出貨，卡表快照的 FieldTranslation 指已選 translation.id。

SNC 另用 `manual-printings/SNC/001.yaml` 路徑提案，仍受單檔 <1 MiB；匯入 snc-list 只產候選，不把 high 當 confirmed。最小封套欄位為 `printing_id/card_id/region/card_no/card_no_state/catalog_state/listing_confidence/serial_total`、references（url/role/locator）、inclusions（`product_id/inclusion_kind/date_precision/date_raw/note`）、decision。`normal_counterparts` 全筆確認後才連同 card；無對應可登 `region_mapping_review` 的 `confirmed_none`＋查核範圍/`as_of`。

例如 BP20-SNC01（ANV，4 周年，初版限定 n/10）的 10 可寫 `serial_total` 候選；卡號是否真的印於卡面仍依來源核對，不能因本文件提到就改 official。PR-350/PR-442 上限 150、PR-544 上限 500 為已知的維護需求，仍留下原證據/欄位來源。月年日期原樣保存，不補完整日期；QR 兌換與初版限定用 `inclusion_kind` 區分。unlisted 公開頁有「非官方整理，可能不完整」、來源/信心/回報入口。

暫定 `card_no` 不占官方網址；`int_id` 所有出貨 printing 都依其地區號段追加分配（§3.2）。補正 `card_no` 後留下 provisional→official 永久 alias；`int_id` 不變。卡號推算與 card 身分是不同軸，同卡通則不會讓所有 SNC 或 EN 候選自動 confirmed。authored/config 已固定 `third_party_image_policy=mirror_reviewed`、`deck_eligibility_policy=regional_decklog`。每張第三方圖以 `review_decision_id` 連到 confirmed 的來源/圖片確認，保存 `source_url`、內容 hash、確認者 `reviewed_by` 與時間 `reviewed_at`；換圖/換來源須重新確認，抽樣不代替逐圖確認。建牌資格依該地區/版次的 `decklog_available`；人工查證記來源與日期，未查證依官方卡表收錄狀態預設（詳 [build-db.md](build-db.md) §17.2）。暫定號/身分不阻擋建牌；不可用版次禁止新加入、新分享碼與匯出。舊碼/既有牌組仍開啟保留條目，警告並提示可用同名版次，不靜默刪除。

發布程序另外追加永久版本索引及內容閉包；所有舊 text 鍵集合用來做固定 16 hex＋lang 的碰撞檢查，無碰撞才可追加，不能重配歷史鍵。這個可重建鍵索引不進人工 registry，也不刪 R2 歷史來省索引工作。
