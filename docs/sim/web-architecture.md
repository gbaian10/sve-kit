# 查卡網站（`sim/web`）架構

引用與授權：商品例與卡文渲染記號中沿用的官方名稱、原文片段不在本專案授權內；
介面、路由與渲染設計屬專案內容。見[文件引用說明](../quotations.md)。

版本：**v1**，2026-09-28 定案。用語依 [`GLOSSARY.md`](../../GLOSSARY.md)；
資料契約以 [`docs/schema/export/snapshot-format.md`](../schema/export/snapshot-format.md) 為準。
共用尺寸見 §7、§8；色彩值見進版控的 [`tokens.css`](../../sim/web/src/styles/tokens.css)。

`sim/web` 是查卡、建牌、對戰共用的前端（`AGENTS.md`）。本文涵蓋查卡需要的架構；建牌與對戰只預留接點。基本目錄載入、Worker 與手機門檻為設計要求；現有 N0 已接線的 3.0 reader 不代表下列分段載入已完成。

## 1. 目錄與邊界

```text
sim/web/
├── index.html                 # viewport-fit=cover；<head> 同步載入 /theme-boot.js
├── public/theme-boot.js       # 主題預繪（§8）；storage 規則的唯一例外
├── vite.config.ts             # dev／preview 把本機快照目錄掛在 /cdn 與 /cdn-preview（§4.1）
├── scripts/fixture/           # 假資料產生器（Node）
├── fixtures/snapshot/         # 產生器輸出：小、確定性、進 git
├── src/
│   ├── app/                   # 路由表、AppShell、Providers、ErrorBoundary
│   ├── pages/                 # 一個路由一個資料夾
│   ├── components/{ui,card,nav}/
│   ├── domain/                # 純函式：不 fetch、不碰 storage、不 import React
│   ├── data/                  # 唯一可以 fetch 的層；index.ts 是公開 API
│   ├── settings/              # 唯一可以碰 storage 的層；index.ts 是公開 API
│   ├── i18n/
│   ├── styles/                # index.css（@theme）、tokens.css、fonts.css
│   └── workers/               # 分片解析與效果全文掃描
└── tests/                     # lint 規則 fixture、靜態檢查
```

邊界由 ESLint 強制（`sim/web/eslint.config.ts`）：`fetch` 只在 `src/data/`、`localStorage`／`sessionStorage` 只在 `src/settings/`，
其他程式碼只能經這兩層的 `index.ts` 匯入。依賴方向：`components`／`pages` → `data`、`settings`、`domain`；`data` → `domain`；
`domain` 不依賴任何一層。所有能用單元測試釘死的邏輯放 `domain/`。

## 2. 路由與 URL

路由用 `react-router` v7（`createBrowserRouter`）。

| 路徑                         | 頁面 | 備註                                                 |
| ---------------------------- | ---- | ---------------------------------------------------- |
| `/`                          | 主頁 | 搜尋為主                                             |
| `/cards`                     | 查卡 | 條件全部在 URL（§2.1）                               |
| `/cards/:cardNo/:slug?`      | 單卡 | 疊層或整頁，由 `location.state` 能否重建決定（§2.2） |
| `/cards/_provisional/:intId` | 單卡 | 暫定號碼版次（快照格式 §8）                          |
| `/sets`、`/sets/:code`       | 卡包 | 預設 `set=`＋歸檔類別標頭（見 §2.1）                 |
| `/settings`                  | 設定 | 顯示設定；R1 沒有登入                                |
| `/decks`                     | 佔位 | 建牌器的入口，佔位頁                                 |
| `*`                          | 404  | 含搜尋框                                             |

保留、不實作：`/cards/unimplemented`、`/decks/*`、`/play`。

`:slug?` 是原文卡名的可讀片段，可有可無：決定卡片的永遠是卡號；slug 缺少或錯誤時照常開啟，並把網址 `replace` 成 canonical
（`/cards/{卡號}/{原文卡名}`）。`card_route_alias` 永久轉址、`route_override` 與暫定號碼依快照格式 §8；找不到的卡號進 404 頁。

### 卡包分組

**使用者決定（2026-09-30）**：卡包選擇依 `product_family.kind` 分成主要分類與下拉選單兩區，套用全站目前選定的 JP／EN 版本，兩區結果不混。

| 顯示區域 | kind | 顯示分類 |
| --- | --- | --- |
| 主要分類 | `booster` | 補充包 |
| 主要分類 | `collaboration` | 合作包 |
| 主要分類 | `special_pack` | 特殊卡包 |
| 主要分類 | `promo` | PR |
| 下拉選單「預組與特別商品」 | `deck`、`special`、`other` | 預組、特別商品、其他 |

`special_pack` 表示隨機抽取、但不是補充包／合作包／PR 的特殊卡包，例如 SP01「スペシャルパック『シーサイド・メモリーズ』」。BSF2024、BSF2025、NY2024、GFE01 是有自己卡號的非賣品活動獎品，保留各自家族，`kind=promo`，顯示在 PR 分類底下；這項分組不合併歸檔代號或改寫 owner。GFB01a–d 使用 `kind=deck`，因為 Gloryfinder Bundle 是四副固定預組的同捆商品，顯示於「預組與特別商品」。

### 2.1 URL 是列表狀態的唯一來源

`domain/query/model.ts` 的 `QueryState`：

```ts
interface QueryState {
  text: string;
  classes: ClassCode[];
  cost: { min?: number; max?: number }; // 7 代表 ≥7
  types: TypeCode[];
  mechanics: Record<KeywordId, "has" | "not">; // 沒列＝不管
  sets: SetCode[];
  rarities: RarityCode[];
  altArtOnly: boolean;
  unit: "card" | "art" | "printing"; // 每格單位，預設 card（合併印刷）
  sort: "no" | "cost" | "name" | "date" | "atk" | "def"; // 預設 no
  view?: "grid" | "table" | "list"; // 沒帶＝用偏好
}
```

`domain/query/codec.ts` 在 `QueryState` 與 URL 參數（`q class cost type mech set rarity alt unit sort view`）之間互轉：
`set` 值用歸檔類別的 `product_family.code`（`SetCode`）；vocabulary 篩選值用其英文 code。順序固定，篩選、排序、單位的預設值不寫進 URL，所以同一狀態只有一種網址。
`view` 是例外：URL 有帶就照 URL；沒帶就用偏好 `viewMode`；使用者切換檢視時同時寫 URL 與偏好。

依使用者 2026-09-30 的規格變更（build-db §15），`/sets/:code` 的路徑代號是 `product_family.public_code`，解析到同一歸檔類別後，以其 `code` 預設 `set=`，標頭顯示歸檔類別名稱。卡包瀏覽與搜尋依已登錄的 `printing.home_set_id` 篩選，且只納入 `printing.region` 符合目前 `cardEdition`（`jp`／`en`，§3）的版次，兩區結果不混；合併卡片顯示也只使用命中的版次。商品名稱、發售日與收錄供單卡頁補充資訊及連結，不驅動 `set=` 篩選；獨立的初收錄 facet 依 build-db §15 的協調者決定，資料缺少或未知時標示 coverage，不因此隱藏卡片。

進階查詢語法由快照格式的 `config.search.grammar_version` 定義；本文的 URL 參數是結構化篩選，語法到位時只是 codec 的另一種輸入。

### 2.2 單卡的呈現規則、上一張／下一張、返回不跳頂

原則：**疊層只在背景可以從 URL 重建時呈現，否則整頁。** 不依賴記憶體內的結果陣列。

- 結果項目 `ResultItem = { key: ResultKey; printingId: PrintingId }`（`domain/query/apply.ts`）：`key` 依 `unit` 是 card／art／printing 的 ID，
  `printingId` 是這一格實際打開的代表版次。結果順序是純函數：同一份快照、同一個 URL 一定得到同一個序列。
- 從列表或建議清單開卡時 `navigate` 帶 `state = { background, source: "results" | "suggest", pages, resultKey }`：`background` 是列表當時的
  search string（`source=suggest` 時只有 `?q=`，序列＝建議清單的順序）；`pages` 是列表已載入的頁數。
- 單卡路由是查卡路由的兄弟路由。`state.background` 存在 → 在下面渲染查卡頁（帶 `background` 與 `pages`，`inert`）、自己以全螢幕層蓋上、
  底列換成「上一張／加入牌組／下一張」；沒有 → 整頁、頂列有「回查卡」、沒有前後張。
- 上一張／下一張：重算 `sequence`，`findIndex(r => r.key === state.resultKey)`；找不到退而以目前版次所屬的 card／art 再找；仍沒有就隱藏。
  移動用 `replace` 並更新 `resultKey`，只在已載入的 `pages` 內。
- 同分頁重新整理時 `history.state` 還在，疊層照樣重建；複製連結到新分頁沒有 state，是整頁。
- 列表 entry 自己的 state（`pages`、`anchor`）**只透過 router 寫**（`navigate(pathname + search, { replace: true, state })`），不直接呼叫
  `history.replaceState`（會蓋掉 router 存在 `history.state` 裡的欄位）。「載入更多」寫 `pages`；點卡時先寫 `anchor` 再 push 單卡。
  返回時查卡頁依 `pages` 渲染同樣多的結果，`ScrollRestoration`（key＝`pathname + search`）還原捲動，再把 `anchor` 那格亮框。
  若「同一事件先 replace 再 push」在 data router 下不穩定，退路是模組層的 `Map<background, ResultKey>`（重新整理後不亮框），採用時要寫進 README。

## 3. 狀態管理

不加狀態管理套件。

| 狀態                           | 放哪裡                                                                                                                                    |
| ------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------- |
| 查詢、篩選、排序、檢視、目前卡 | URL（§2.1）＋ `location.state`（§2.2）                                                                                                    |
| 偏好                           | `settings/prefs.ts`：一個 `Prefs` 物件、`useSyncExternalStore`、localStorage `sve-kit:prefs`（有版本欄、無效值回預設、讀寫都 try／catch） |
| 最近查看                       | `settings/recent.ts`：最多 8 筆                                                                                                           |
| 快照載入狀態與索引             | `data/status.ts` 的 store，`useSnapshot()`／`useCard()`／`useDetail()` 訂閱                                                               |
| 面板開關、篩選草稿、toast      | React state／context，離開頁面即丟                                                                                                        |

`Prefs` 欄位：`uiLanguage`、`cardEdition`（`jp`／`en`，預設 `jp`）、`nameDisplay`（`translated`／`original`／`both`）、`effectLanguage`（同）、
`symbolLabels`、`theme`（`system`／`light`／`dark`）、`accent`（`amber`／`teal`／`red`／`null`＝依主題預設）、`banRegion`（`jp`／`en`）、
`gridDensity`（2／3）、`viewMode`。

## 4. 資料層（`src/data/`）

### 4.1 位址

- `VITE_CDN_BASE`（建置時）決定 CDN 根目錄；沒設時預設同源 `/cdn`，所以建置與檢查不需要環境變數。
- 本機開發：`vite.config.ts` 把 `SVE_EXPORT_DIR` 掛在 `/cdn`（沒設就掛 `fixtures/snapshot/`，clone 下來就有資料）、`SVE_PREVIEW_DIR` 掛在 `/cdn-preview`。
- 根目錄的版面依快照格式 §4.1 與卡圖的內容定址路徑。介面用的官方職業／卡文圖示與 logo 放在 `src/assets/official/`（附來源與版權說明，
  不在 repo 授權範圍內），不走 CDN。
- **預覽快照**（快照格式 §4.2）：另一個根目錄（`SVE_PREVIEW_DIR`）、`data_version` 帶 `preview-` 前綴、不寫版本索引。reader 明確選擇資料根：
  `data/` 的所有狀態與快取以根目錄 base 為 key（`createSnapshotClient(base)`），切換根目錄＝換一個 client，記憶體內不混用；日後 IndexedDB／
  CacheStorage 的 namespace 同樣帶 base 與 `data_version`。畫面上 `data_version` 帶 `preview-` 前綴時顯示「預覽資料」。預覽的清單入口由快照格式定義；本文不指定入口路徑，由 `data/versions.ts` 實作。

### 4.2 載入順序與驗證鏈

以下流程適用正式快照；預覽入口依快照格式。首頁與外框不等待版本索引或 manifest。

```text
首頁／外框可用 → 背景載入入口、manifest、config
             → 所選區基本目錄下載／驗證／索引 → 整區可搜尋
                                            → 背景補齊印刷資料
                                            → 來源／標註、詳情、另一區按需
每個用途各自 loading／ready／error；失敗可重試
```

- 目前 3.0 reader 的完整版本／minimum／八個 capabilities 依[公開 annotation §1](../schema/export/public-annotation.md#1-版本與驗證邊界)，不只宣告 column-partition-v1；index_format=2 只取 current／previous 或本機已驗相容 active。
- 索引先驗形狀，以 entry.manifest_sha256 驗完整清單，再以 File.sha256 驗解壓後 canonical bytes；不以未驗 manifest 決定可用閉包，不取舊 pages。
- 首頁顯示後背景下載所選 jp／en 整區基本目錄；所有連線同一政策，不設省流量模式。下載中可打字並顯示「資料下載中」，整區驗畢及索引完成後自動搜尋一次，不顯示部分結果。
- 卡名／卡號、基本數值與基本 facet 先可用；稀有度、異圖與完整印刷版本之後背景補齊。相應條件未備妥時提示載入中，不能用部分版次輸出完整搜尋結果。全文與其他進階用途須等其整區閉包驗畢。
- 另一區只在切換或對照時下載；不因 UI 語言切換改 card edition，亦不以字串可讀宣稱雙區來源已驗。單卡深連結可先載當頁完整用途，不宣稱整區搜尋 ready。
- 網路／Service Worker 處理取得、驗 bytes 與持久快取；Worker 逐容器解壓、驗證、解析、建立索引；main 接收必要 view、進度與完整結果。原始 tuple／解碼字串建立緊湊 store 後釋放，詳情用有界 LRU，不把 text_all 一次 JSON.parse 到 main。
- 現有 PWA 前只靠 HTTP 快取；Service Worker／CacheStorage 的上述分工是設計要求，不宣稱已實作。快取不可用時保持明確用途狀態並重新下載，不要求整庫離線預取。
- 持久 bytes 以根目錄及內容定址 URL 重用，衍生索引以 manifest／reader／normalizer 隔離。切快照／版本／查詢取消舊工作，拒絕晚到回應；部分用途 ready 不等於完整離線 active。
- `data/` 公開呼叫採 async，提供用途所需 keys、載入／解析進度與分開的文字／來源／標註狀態；未知分母不造百分比。基本文字就緒、日文依據／標註未備妥、完整檢視缺來源（錯誤）依[公開 annotation §4.1](../schema/export/public-annotation.md#41-三種就緒狀態與部分-reader)。
- main＋Worker 穩態合計 ≤48 MiB、更新峰值 ≤80 MiB，主線程單段 ≤50 ms、所選整區基本目錄完整解析累計 ≤1 秒；量法依[size-budget](../schema/export/size-budget.md#手機記憶體與解析)。各 Worker 不分別享有 48／80 MiB，下載 bytes 不等於 heap；本文件不宣稱效能驗收通過。

### 4.3 容器解碼、型別與拒絕條件

- `data/format-v3/schema.ts` 直接引用 carddb 隨套件發布的唯一 JSON Schema；建置時編譯 Ajv standalone validators，執行期與 Worker 不重新編譯。
  欄序、tuple 長度、nullable 及固定 descriptor 由 Schema 驗；解碼 accessor 使用同一份 `x-columns`／`x-types`。
  卡表管線提供的共用 golden（`tests/fixtures/snapshot-contract/v3/`）與獨立 logical oracle 防止 consumer 與 producer 同時偏離契約。
- `reader.ts`／`semantics.ts` 驗精確 dependencies、hash、`row_index`／`face_ordinal` 合併、完整引用閉包及其他跨列規則。
  `annotations.ts`／`annotation-placement.ts` 驗公開 annotation 的概念、位置、來源 owner 與實際分片歸屬。
  詳情片按需載入時仍須拒絕錯 base、重複用途及不一致來源；不能借另一個 owner 的 ranges 補洞。
- TS 邏輯值與索引存於 `data/store.ts`；文字呈現由 `data/annotated-text.ts` 依精確 owner 取得，
  Unicode scalar ranges 在呈現邊界轉成 UTF-16，不正規化原始文字。必要 FK 與 annotation 閉包驗證遵守公開契約。

### 4.4 索引（`data/store/`）

基本目錄先建所選區全部卡／面、版次輕量定位、正規化卡號 → 版次、current／暫顯名稱與可用翻譯、基本 facet（職業、種類、費用、攻擊、體力、特性、卡包、特殊種類）。
稀有度／異圖、商品收錄、機制三態（§4.6）、Q&A／勘誤各依用途完整載入後補索引，分別保留未備妥狀態。介面 `CardIndex` 固定；實作可由 `Map`／陣列換成 TypedArray posting lists＋唯一字串池（快照格式 §3.1）而不改呼叫端。`size-budget.md` 的記憶體門檻以目標手機實測為準。

### 4.5 詳情分片、文字與定位

- `details.ts`：`loadShard(key)` → 解碼 → 驗拒絕條件 → `row_index` 對回精確 base fragment 的列（`printing.faces` 用 `face_ordinal`）；LRU。
- `text.ts`：`textOf(unitId)` 先查基本目錄，再查已載入的詳情片；沒載入時回「未載入」，與完整查找後不存在分開（[公開 annotation §4.1](../schema/export/public-annotation.md#41-三種就緒狀態與部分-reader)），呼叫端顯示載入中。
- **定位**：快照格式允許同 family 依 ID bucket 拆成多片，所以「一個 owner → 一個檔」不成立。`Locator.shardsFor(kind, ref) → fileKey[]`
  回傳候選檔集合（`kind` 是分片種類，`ref` 是 owner 或 `text_unit` bucket）；要找某一列時依序載入候選片、用片內索引確認；
  「不存在」只在所有候選片都載入且都沒有時才成立。候選集合如何從快照清單算出由快照格式定義；`Locator` 是唯一知道這個規則的模組。

### 4.6 機制三態

`domain/mechanics.ts` 的 `triState(card, keyword, scope)`，依快照格式 §2、§3.1：

1. `mechanic_projection` 有該 (card, keyword, scope) → `present`。
2. 沒有這張卡該 scope 的 `card_mechanic_coverage` 列 → `unknown`（尚未檢查）。
3. 有列：先在 keyword universe U 依 `complete_mode` 解出 complete（include＝列表本身；exclude＝U 減列表；`complete_all=true` 時 complete＝U），
   再在 U∖complete 依 `partial_mode` 解出 partial。
4. keyword ∈ complete → `absent`；∈ partial → `unknown`；其餘 → `unknown`。
5. 卡面地區是 EN 且 `card_engine_support.region_blocks` 有 EN block 時，shared scope 的結果降為 `unknown`。

另一個 keyword 是 present 不能推本項 absent。

### 4.7 搜尋

- 名稱／卡號搜尋只在所選整區基本目錄驗畢並建索引後回完整結果，優先序為正規化卡號完全／寬鬆命中 ＞ 卡號前綴 ＞ 可用卡名／翻譯／`search_alias` 前綴／包含。
  效果全文按需交 Worker 載入並掃描整區該用途閉包（去抖、取消舊查詢、顯示進度），完成後回完整結果，不把已掃片的部分命中當完成。冷啟動與看過幾張卡後的結果須一致。
- 正規化（`domain/normalize.ts`）：NFKC、大小寫折疊、去空白與連字號差異；卡號寬鬆比對（`bp01-51`、`BP01 051`、`BP01-051EN` 都命中）。
  `search_alias.normalized` 的規則要與匯出器（`config.search.normalizer_version`）一致。
- 篩選 `domain/query/apply.ts`：`QueryState` × `CardIndex` → `ResultItem[]`；合併印刷時代表版次優先選符合目前卡片版本與條件者，其次依卡號。

### 4.8 假資料與三層防漂移

`scripts/fixture/` 手寫合成卡（使用自編名稱與卡文），產出完整的版本索引（兩頁）、快照清單、config、啟動包、依 owner 切的詳情分片
（故意把同 owner 同 kind 拆成兩片）、影像清單與程式生成的佔位圖；檔名內容定址、hash 真算、輸出確定性。reader 的測試直接載入它。

| 層                                      | 防什麼                       | 適用資料                     |
| --------------------------------------- | ---------------------------- | ---------------------------- |
| web 常數表＋自家假資料                  | web 內部漂移                 | 假資料                       |
| 卡表管線的 JSON Schema＋golden 契約樣本 | web 與匯出器對格式的解讀不同 | 契約樣本（conformance 測試） |
| 預覽快照 → 正式本機快照                 | 定位、大小、效能             | 真快照                       |

## 5. i18n 與卡面語言

- 介面文字走 i18next；`zh-TW` 是參考形狀，其他語言 `satisfies Messages`，key 齊全由型別檢查；另有靜態檢查：三語同 key 的插值集合一致、
  `zh-TW` 不含維護清單內的簡體專用字。數量與日期用 `Intl`。
- 卡面語言模型 `domain/textLanguage.ts` 的 `resolveFaceText({ edition, uiLanguage, face, revision, prefs })`，實作快照格式 §5 的矩陣：
  卡片版本決定原文與卡圖，介面語言決定譯文列；日／英介面不回退繁中；機器翻譯與本站翻譯都標「非官方」（樣式不同）；缺譯有提示文字，不出現空白；
  切英版但無對應時提示「尚未發行英文版」且卡圖維持日版。
- 卡文渲染 `domain/cardText.ts`：`{記號}`、`{コストN}`、`【關鍵字_N】`、`\n` 切成片段；記號表來自快照 `text_symbol`、關鍵字來自 `keyword`，不寫死。
  記號顯示官方圖示（`aria-label` 用 `localizations`）；關鍵字是可點方塊，點了就地解說並提供「找有此關鍵字的卡」。
- 日文文字節點加 `lang="ja"`。

## 6. 卡圖

`components/card/CardImage.tsx`：由 `data/images.ts` 從 2.0 的卡包 `printing_image` media 取得版本／尺寸，配合 int_id／face.ordinal 組 `srcset`、`width`、`height`；不為首圖載 global `image_variant`；`alt` 三種語境
（`identify`＝卡名＋版次；`redundant`＝旁邊已有同樣文字；`decorative`）；`sizes` 由呼叫端依版面給；`fit: cover | contain`（橫向卡用 contain）；
固定比例、`loading="lazy"`、`decoding="async"`。`/cards` 預設圖卡格；隨 app 打包自行設計的共用佔位圖，尺寸比例與卡片相同，不使用官方卡背。載入中／缺圖共用它，缺圖與待確認另加標示。
列表與建議清單用整張卡圖檔位，放大層用最大檔位；查卡不用插畫裁切檔位。

## 7. RWD 與無障礙

- 斷點依內容寬：`md=600`、`lg=1000`、`xl=1440`。<600 底部列；600–999 底部列＋內容加寬；≥1000 左側欄；橫向手機（高 <480）底部列改左側小列。
- 底部固定元件共用 `BottomBar`，內建 `env(safe-area-inset-bottom)`；鍵盤彈出時底部導覽收起。
- 面板與對話框用原生 `<dialog>`（`showModal()`：焦點圈住、Esc、關閉後焦點回原處、背景 inert），開啟時 push 一筆 history 讓瀏覽器返回也能關；
  選單用 Popover API。
- 焦點環用主文字色，寬 2 px、外距 2 px；skip link；結果數 `aria-live="polite"`；`motion-reduce:` 關動畫。
- 主要觸控目標至少 44×44 CSS px；主要動作按鈕高 48 px、最小寬 48 px（根字級 16 px 時）。較小的可見控制項須補足點擊區域。
  既有按鈕以 rem 實作，隨根字級縮放，見 [`Button.tsx`](../../sim/web/src/components/ui/Button.tsx)。
- 職業快捷列 7 格永遠一排；帶字版放得下才顯示文字（離屏量測）。

## 8. 主題 token

- `src/styles/tokens.css` 是唯一允許顏色常值的檔。基礎色 token 在三個選擇器都有定義：裸 `:root`（淺色）、
  `@media (prefers-color-scheme: dark)` 內的 `:root:not([data-theme="light"])`、`:root[data-theme="dark"]`。強調色 token 另成一組，
  由 `<html data-accent>` 選；沒有 `data-accent` 時依主題用預設色。兩組都有靜態檢查（postcss AST，含 `@media` 內的宣告）。
- `src/styles/index.css` 的 `@theme inline` 清掉 Tailwind 預設色盤、只登錄語意色（`bg`、`surface-*`、`text-*`、`border*`、`scrim`、
  語意色與 `-soft`、職業色 `class-*`、`cost`、`accent*`）、圓角、字級與斷點。元件只用這些 class；`dark:`、任意值色碼、`w-screen`／`h-screen`、
  任意值固定寬都被 ESLint 擋。
- 職業色只小面積用：元素帶 `data-class="<職業>"`，CSS 把 `--class-color` 設成該職業 token，`@theme` 登錄 `--color-class-current`，
  元件寫 `border-class-current`。
- 決定實際主題與強調色的純函數 `domain/theme.ts`：`resolveTheme(prefs, systemDark) → { theme, accent }`。
- 主題預繪 `public/theme-boot.js`：`<head>` 內同步載入（非 module、在 CSS 之前），只讀 `sve-kit:prefs` 的 `theme`／`accent` 兩欄、白名單驗值、
  整段 try／catch，設 `data-theme`／`data-accent`。它在 `src/` 外、不受 storage 的 ESLint 規則管，是唯一例外；有 Vitest 測壞資料與 storage 拋錯，
  且與 `resolveTheme` 結果一致。
- 字型：IBM Plex Sans（latin 子集）配系統中日文字型堆疊。

共用數值與 [`index.css`](../../sim/web/src/styles/index.css) 一致：

| 項目 | 數值 |
| --- | --- |
| 字級 | 11、12、13、14、15、16、17、18、20、22、24；實際值為該數字除以 16 的 rem，隨根字級縮放 |
| 效果文字行高 | 1.75 |
| 圓角（px） | badge 5、sm 6、card 8、control 10、button 12、block 14、pill 16、dialog 18、sheet 20 |

## 9. 自動把關與測試

| 層        | 內容                                                                                                                                                                                                                          |
| --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| ESLint    | 顏色只能 token、禁 `dark:`、禁 `w-screen`／`h-screen`、禁任意值固定寬、禁寫死介面文字、資料／設定層邊界、a11y strict（規則 fixture 有測試）                                                                                   |
| Stylelint | 非 token 檔禁色碼、禁 `vh`、禁 `!important`                                                                                                                                                                                   |
| 型別      | TS strict＋額外選項；快照型別與常數表                                                                                                                                                                                         |
| Vitest    | `domain/`、`data/`、`settings/` 的單元測試；元件測試只測行為（面板草稿／套用、鍵盤、卡文渲染、卡圖佔位、路由呈現規則、對話框焦點）；靜態檢查（i18n、token、theme-boot）；假資料可被 reader 載入；契約反例（每個拒絕條件一例） |
| 腳本      | `knip`（手動）、首屏體積基準（只記錄不設門檻）                                                                                                                                                                                |
| 真瀏覽器  | 不在 `bun run check` 內；版面以真瀏覽器在固定裝置尺寸與深淺色下的 DOM 幾何量測驗收（無水平捲動、主要觸控目標尺寸）                                                                                                            |

`bun run check`（Prettier、ESLint、Stylelint、`tsc -b`、Vitest、build）是合併的最低門檻。

## 10. 依賴

`react-router`、`clsx`、`lucide-react`、`@fontsource/ibm-plex-sans`；測試 `@testing-library/user-event`；工具 `knip`。
不加狀態管理套件或 UI 元件庫；Schema 驗證使用已釘版的 Ajv standalone validators。

## 11. 與其他部分的接點

- **卡表管線**：快照格式是契約，前端不對格式未規定的地方另作假設；格式改變時同步改 `data/format-v3/`、版本入口與 `Locator`。
- **建牌**：同一個搜尋工作區加牌組面板；列表元件預留每張卡的動作列插槽，`useBlocker` 處理離開編輯頁。
- **上線**：登入、PWA（Service Worker、CacheStorage、IndexedDB 索引持久化，namespace 帶根目錄與 `data_version`）、正式部署
  （`VITE_CDN_BASE` 指向 CDN 網域）。

## 快照 3.0 與圖片更新邊界

現行 reader 僅支援 3.0；圖片沿用 [傳輸契約 §5.4](../schema/export/snapshot-transport.md#54-卡包-media-與-id-圖片) 的卡包 media 與 Index v2。
公開 annotation 與 JP 來源閉包依 [公開 annotation 契約](../schema/export/public-annotation.md)，不能只放寬版本範圍。
卡包 media 提供 card／art 版本與實際尺寸，int_id＋永久 face.ordinal＋size 直接組背景圖片 URL；玩家頁面路由不變。
切新快照時更新 src／srcset、取消舊工作、拒絕晚到舊 response；SW 以完整含 v URL 匹配，不忽略 query 或回退舊圖。
新圖未完成／失敗用 placeholder；離線舊 active 明示時效，不宣稱圖片最新。
只在 current／previous 或本機已驗完整 active 選相容者，無相容者更新 reader；落後多版、舊片回收則重取 current。
本機 pin 不延長伺服器保留。query 分離已實測，瀏覽器／SW 的暖快取與故障整合仍須實作驗收。
