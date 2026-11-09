# 人工版次（官方資料庫未收錄）：manual-printings-v1

本契約是 [#130](https://github.com/gbaian10/sve-kit/issues/130) 的 docs 單位，細化 [authored-layout §9](authored-layout.md#9-批次表記上下文與非官方條目)。
範圍與欄位依維護者 2026-11-09 決定（[#897](https://github.com/gbaian10/sve-kit/issues/897)）：
官方日英卡表只收有卡號的 EP／SEP，比賽獎品、會場活動、店家合作、大使簽名會發放的 EP／SEP，以及規則頁附的基本 EP／SEP
實體存在但官網沒有，這個入口把它們收進卡表，前端標「官方資料庫未收錄」。之後的限量序號卡（SNC、數位版活動卡、序號補充）走同一個入口；
那部分尚未實作，§8 保留原始設計。官方卡文、第三方頁面與原圖不進此入口。

讀取、驗證與建置由 `sve_carddb.domains.manual_printings` 實作，卡圖由 `sve_carddb.images.manual` 轉檔，
登錄工具是 `sve-carddb manual-printings append／check`。

## 1. 收錄原則

- 每筆記錄是一個**實體版次**：同一張卡面不同活動發的 EP 各自一筆（例如三款「ガルミーユ×うどん」）。
- 沒有官方卡文可比對，所以每筆都建**獨立的 provisional card／face**（`identity_state=provisional`、`layout=single`），不與官方 card 共用，
  也不在版次之間共用。之後若官方卡表收錄，走 [identity-repair](identity-repair.md) 的明示身分修復，不改 printing ID 與 `int_id`。
- **卡號可以沒有**：這批全部沒有官方卡號，`card_no_state=none`、`card_no=null`（建置資料庫與快照寫成空字串 `""`）。**不編假卡號**。
  有卡面號碼證據的未收錄版次（之後的序號卡）仍可 `official`／`provisional`，規則沿 §8。官方卡表版次永遠必須有卡號。
- **卡面左下印字**（例如「2024千葉-A」「2024福岡」）另存 `printed_code`，可顯示、可搜尋，**不是卡號、不推網址**。
- **卡名照卡面登錄**成人工名稱 `name`（來源是卡面），**不寫能力文字**（EP 規則都一樣），比照官方 `BP12-EP` 只有卡名的顯示。
  人工名稱不是官方 `printed_name`、不產生 `face_revision`／`face_current`、不進 `rules_name`、翻譯與 DSL。
- 地區只有 jp／en，`name.lang` 等於該區原語（ja／en）。

## 2. 入口與分片

| 路徑（相對 authored） | 內容 |
| --- | --- |
| `manual-printings/printings/<home_set_id>/<NNN>.yaml` | `format: 1, kind: manual_printings_shard, records: [...]`，每片至少一筆 |
| `ids/index.yaml` | 官方版次的配號 index；本入口不修改 |

分片沿 [authored-layout §2](authored-layout.md#2-分片與來源) 的嚴格 YAML 1.2、單檔 <1 MiB、路徑安全規則，拒絕未知欄位、符號連結與重複記錄。
目錄名就是 `home_set_id`（owner），記錄的 `owner` 必須相同。分片**只增不改**：新批次寫到該 owner 的下一個序號檔，舊檔不重排、不改寫；
需要撤回或更正時另走明示的修復流程。沒有這個目錄時入口為空、仍通過驗證。

record 恰為 `{kind: manual_printing, owner, data}`，`record_key = manual_printing:<printing id>`。

## 3. 值欄位

`data` 恰有以下欄位，可空欄位仍須明示 null：

| 欄位 | 型別與規則 |
| --- | --- |
| `slug` | `^[a-z0-9][a-z0-9-]{0,63}$`，維護者取的穩定英文識別字，全域唯一；是永久 ID 的錨點，之後不改（不叫 `key`，避免被 secret 掃描誤判） |
| `id`、`card_id`、`face_id` | 永久 ID，沿 [authored-layout §3.1](authored-layout.md#31-id-與身分) 的 UUIDv5 namespace：`p`／`c` 的 anchor 為 `<region>:manual:<slug>`（卡號不會含 `manual:`，不可能撞官方 anchor），face 為 `f` 加 `<card_id>:0`。驗證重算必須相等 |
| `int_id` | 落在該區未收錄號段（§4），全域唯一，由登錄者人工指定，可留空號 |
| `region` | `jp`／`en` |
| `home_set_id` | 已 confirmed 的 `product_family.id`，等於 owner；EP／SEP 活動卡歸 `EP` 家族 |
| `card_type` | `ep`／`sep`；核對卡圖方向（EP 直式、SEP 橫式），並投影成版次的 `manual_type_code`；不產生官方卡文 |
| `card_no`、`card_no_state` | `card_no_state=none` ⇔ `card_no=null`；`official`／`provisional` 時 `card_no` 為非空原樣字串（§8 的序號卡） |
| `printed_code` | 卡面印字的原樣字串或 null；不能有首尾空白 |
| `name` | `{lang, text}`，lang 等於該區原語，text 非空、無首尾空白 |
| `available_on`、`date_precision` | 活動開始／基本卡首次發售日期；`day` 為真實曆日 `YYYY-MM-DD`，`month` 僅 `YYYY-MM`，`unknown` 必為 null；禁止替月份補日 |
| `release_kind` | `event`（比賽／會場／店家／大使）或 `basic`（規則頁附的基本 EP／SEP） |
| `references` | 排序去重的公開 URL 陣列，每個都要通過 §5 白名單；`event` 至少一個，`basic` 必為空 |
| `image` | null 或 `{sha256, width, height, origin, source_url, approved}`，見 §6 |

## 4. 配號政策 v3：`region-ranges-2026-11-09-v3`

程式唯一定義在 `sve_carddb.domains.registry.allocation`。官方版次仍從原有 `next_int_id` 游標往後配，號段、既有配號與分片不變。

| 區 | 官方卡表版次（`REGION_RANGES`） | 官方未收錄（`UNLISTED_RANGES`） |
| --- | --- | --- |
| jp | 20001–54999 | 55000–59999 |
| en | 60001–94999 | 95000–99999 |

1–20000 仍保留、60000 仍未分配。`ids/index.yaml`（`format: 2`）僅保存官方 `next_int_id`；現行 index 不接受 `next_unlisted_int_id`。
歷史 v1／v2 index 仍可讀以重播身分修復的 basis；v2 保留原有兩組游標，不將它們轉成 v3。
地區仍讀 `printing.region`，不從號碼反推。

人工版次的每筆記錄與 `manual-printings append --spec` 輸入都必須明定 `int_id`。工具只配發永久字串 ID，
驗證整批與既有記錄的整數 ID 全域唯一、落在該區未收錄號段；可跳號、可在之後填補空號，不要求連續或最大號＋1。
重複或出界時整批失敗，人工工具不寫官方 index。區塊與保留號是人工登錄約定，驗證器不限制只能使用列出的區塊或空號。

EP／SEP 依類別分區塊、每類從整百開始；同區塊內依下表與既有記錄配號，後續新增避免佔用保留號。

| 區 | 起點 | 類別 |
| --- | --- | --- |
| jp | 55000 | 基本卡（EP 55000、SEP 55001） |
| jp | 55100 | 官方大使 |
| jp | 55200 | 店家合作 |
| jp | 55300 | ご当地 EP（依發放日期） |
| en | 95000 | 基本卡（SEP 95001） |
| en | 95100 | 官方大使 |
| en | 95200 | 活動與賽事獎品 |

| 保留號（暫不登錄） | 用途 |
| --- | --- |
| 55100–55102 | JP 舊設計大使：渡辺雄也、ゆいっこ、Riowh（缺圖，[#1895](https://github.com/gbaian10/sve-kit/issues/1895)） |
| 95000 | EN 基本 EP（遊戲開賣日 2023-06-30；官方圖含 SAMPLE 浮水印，待補） |
| 95100–95101 | EN 舊設計大使 DifferentFight、Ignideus（第一代僅這兩位） |
| 95202–95203 | EN BCS 24/25 Mono 閃卡（可能兩張，對應 Mono 02/03，缺圖） |

維護者於 2026-11-09 明示授權在正式上線前，將既有 26 筆人工 EP／SEP 的 `int_id` 一次性改為上述區塊，並新增 EN 基本 SEP。
這是分片只增不改的明示例外，printing／card／face 永久 ID 不變；官方版次的 `int_id`、游標與分片均不變。
前端在無卡號（`card_no_state=none`）的版次之間依 `int_id` 比較，保留有卡號版次的原排序規則。

## 5. 來源連結

unlisted 版次的 `references` 投影成公開 `reference_urls`，**只放官網、官方 X 或官方影片**（維護者決定），由 `reference_class` 以精確規則核驗：

| 類別 | 規則 |
| --- | --- |
| `official_site` | https、無帳密／埠／fragment，host 精確等於 `shadowverse-evolve.com`、`en.shadowverse-evolve.com`、`sve-gp.com`、`bushiroad.com`、`en.bushiroad.com` |
| `official_x` | `https://x.com/<帳號>/status/<數字>`，帳號（不分大小寫）為 `shadowverse_ev`、`shadowverse_jp`、`svevolve_en`、`ShadowverseRAGE`、`bo_tcg_event` |
| `official_video` | `https://www.youtube.com/watch?v=<id>` 或 `https://youtu.be/<id>`；頻道是否官方由維護者逐筆核對 |

子網域萬用、同根網域推定、第三方店家頁一律拒絕；新增 host 或帳號要改程式並審核。

**例外**：`release_kind=basic` 的基本 EP、基本 SEP **不放任何來源連結**（`references` 必為空、`reference_urls=[]`，`image.source_url` 也必為 null）；
其他 unlisted 版次至少一個。這是維護者 2026-11-09 定的規則，原 v1「unlisted 至少一個 URL」的 Schema 限制已移除，改由本入口驗證。

## 6. 卡圖

卡圖來源是維護者自己拍的實卡照片，或從官方宣傳圖／影片裁出的單張卡；母檔是 PNG，**不進 git、不進 R2**，只記錄：

| 欄位 | 規則 |
| --- | --- |
| `sha256` | 母檔 bytes 的 `sha256:<hex>` |
| `width`、`height` | 母檔像素；EP 須 `height > width`，SEP 相反 |
| `origin` | `maintainer_photo`（無 `source_url`）、`official_image`（必有官方 `source_url`；`release_kind=basic` 例外，見下）、`official_crop`（`source_url` 可空，有則須通過 §5） |
| `source_url` | 有值時須通過 §5；`release_kind=basic` 一律為 null（維護者 2026-11-09 決定基本卡完全不留來源 URL），即使 `origin=official_image` |
| `approved` | 固定 `true`：逐張人工核可的內部紀錄，前端不顯示 |

`images build-index --manual-image-dir <目錄>` 遞迴掃該目錄下的 PNG，以 sha256 對應母檔（檔名無意義），用與官方圖相同的
[variants 規則](../images/image-variants.md)產生 card_s／card_l／art_s，並核對解碼尺寸等於記錄值。有任何記錄帶 `image` 而沒給目錄即失敗。
公開 `image_asset`：id 為 `img:manual:` 加 `H({printing_id, sha256})`，`source_src_raw` 為母檔 sha256 字串，`source_url` 為 `image.source_url`（可 null，因此
`image_asset.source_url` 改為可空）。`image: null` 的記錄仍出一列 `printing_image`（`pending`／`missing`、無 variants），`source_src_raw` 為 `manual:<slug>`。
文字匯出 `attach` 以上述規則核對每個人工面，不讀官方觀測。範圍建置（`--scope`）可用 `home_set_id` 或 printing ID 選到人工版次；人工母檔每次重編碼，
不借用上一版。

## 7. 建置與匯出

建置資料庫（[build-db](../build/build-db.md)）每筆寫入：

- `source_record`：kind `authored`、`parser_version=manual-printings-v1`、分片 canonical hash 與 authored revision
- `card`（provisional、single）、`face`（ordinal 0、front）、`card_int_id`
- `printing`：`catalog_state=unlisted`、`card_no_state`、`card_no`（none 時 `""`）、`printed_code`、`manual_name_unit_id`（人工名稱的 text_unit）、`manual_type_code`（ep／sep）、`manual_available_on`／`manual_date_precision`（發放日期與精度）、
  `listing_confidence=high`（維護者逐張看過實卡）、`rarity_raw="-"`（卡面無稀有度，沿官方 EP 的「-」）、`decklog_available=false`、`unverified`
- `printing_face`：`printed_text_state=unknown`、三個 printed 欄位 null、無 observation
- `printing_reference`：每個 `references` URL 一列

DDL 約束：`card_no_state=none` ⇔ `card_no=''`；只有 `catalog_state=unlisted` 可為 none、填 `printed_code`、`manual_name_unit_id` 或 `manual_type_code`。
不產生 `face_revision`、`face_current`、`rules_name`、翻譯或 DSL；`card_engine_support` 該區 block 為 `identity_unconfirmed`＋`missing_region_source`。
路由走既有 `/cards/_provisional/{int_id}`（[build-db §15](../build/build-db.md#15-網址搜尋預設版次與記號)），不佔官方 `card_no` 路由；預設版次 method 為 `fallback`。

卡表快照（[snapshot-format](../export/snapshot-format.md)）printing 末尾新增 `printed_code?`、`manual_name_unit_id?`、`manual_type_code?`、`manual_available_on?`、`manual_date_precision?`，`card_no_state` 新增 `none`；
`reference_urls` 來自 `printing_reference.url`。沒有任何觀測的 unlisted 面不要求 current 或「表記未定」項目（`face.current=[]`、`face.wording=[]`），
這是 [snapshot-format §2.3](../export/snapshot-format.md#23-卡文版本與依據版本) 的明示例外，官方面規則不變。
前端缺少官方種類時，僅 unlisted 版次使用 `manual_type_code` 作種類標示、種類計數與牌組位置篩選；官方種類永遠優先，官方版次不使用人工值。建置配置釘住每片的路徑與 hash（`manual_printings`）。

人工日期由維護者逐筆採納，以活動開始日為準，不以公告日替代。`available_on` 在日／月精度時必填，unknown 必為 null；
本批 3 筆未知發放日追蹤於 [#1902](https://github.com/gbaian10/sve-kit/issues/1902)，不以推測日期補齊。基本卡依維護者指定的首次發售日。
快照的日期欄只限 unlisted；official 兩欄必為 null，既有商品／收錄日期不變。
前端單卡版次資訊共用「發行日」一行與 ISO 日期格式，月精度只顯示 `YYYY-MM`，unknown 不顯示。
列表的卡包／發行日排序對人工版次使用本欄（unknown 置後），官方仍用既有地區卡包發售日；月精度直接比較 `YYYY-MM`，不補成月初曆日。

## 8. 尚未實作：限量序號卡與序號補充

以下是 v1 原始設計中這次沒有用到的部分，保留作為序號卡批次的起點，**實作前不得宣稱已支援**：

- 限量序號卡（周年 SNC 含 `BP20-SNC01`、數位版活動實體卡 WB、既有官方 PR 補序號）走同一個 `manual_printing` 記錄：有卡面號碼證據時
  `card_no_state=official`，只有店家佐證或推定時 `provisional`；兩軸與 `catalog_state` 獨立。候選不等於 confirmed；信心 high 不代表確認。
- 歸屬：SNC 編號周年卡歸 `SNC`，數位版活動卡歸 `WB`，官方 PR 補序號留 `PR`；`ANV` 是稀有度標記不是家族。稀有度原樣存 `rarity_raw`，code 只在已採納 exact binding 才填。
- 需要新增的欄位：`counterpart`（確認對應的一般版 card／face，之後改走 identity-transition）、`serial_total`／`serial_note`（卡面分母，不記個人序號）、
  `inclusions`／`distribution`（商品收錄或無商品的獎品配送，日期精度沿 product 規則）。
- `serial_supplement`：為已存在的官方 printing 補 `serial_total`、註記與證據，不新增版次、不配 `int_id`、不改官方卡文與收錄狀態。
- 第三方店家 URL（`third_party_url`）與官方封存證據（`official_archived` 的 source_ref 閉包）：本批只收官方公開 URL，兩者都未實作。

## 9. 下游驗收

程式以合成資料驗：官方卡不准空卡號或人工欄位；unlisted 配號人工指定、落在未收錄號段、全域唯一且允許空號；官方游標不受影響；基本 EP／SEP 無連結、活動卡至少一個；
非官方 host／非官方 X 帳號拒絕；人工卡名只進 `manual_name_unit_id`，不進 `face_revision`／`rules_name`；無觀測的 unlisted 面匯出通過且無 wording；
母檔以 sha256 對應、尺寸不符或缺檔失敗、無母檔的記錄出 pending 列；索引的人工 asset 與記錄不符拒絕；分片未知欄位、ID 不符 recipe、owner 不等於 home_set 均拒絕。

公開卡圖及快照保留依 [snapshot-format §4.1](../export/snapshot-format.md#41-發布窗口圖片新鮮度與回收)；卡圖 URL 以 `int_id`＋永久 `face.ordinal` 為 key。
