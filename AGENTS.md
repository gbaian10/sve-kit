# AGENTS.md

給 AI 開發工具（Claude Code、Codex 等）的專案說明。**這份是專案架構與資料規則的權威**；
語言、commit 與寫法規範以 `CONTRIBUTING.md` 為準。`CLAUDE.md` 只引用這份，不要在其他地方重複寫規則。

這份會進版控，只寫**任何 clone 這個 repo 的人都適用**的規則。
個人機器上的路徑、本機限定的檔案、個人工作流程，**不要寫進這裡**。

## 這是什麼

Shadowverse: EVOLVE（實體卡牌遊戲，簡稱 SVE）的非官方工具組，一人專案。

- **卡表**：爬日文與英文官網，合併人工資料，匯出有版號的 JSON 快照
- **模擬器**：已有引擎原型、scenario runner 與 Web 用戶端；完整網頁對戰服務仍在開發（之後可能打包成 PWA、Tauri）

不是官方產品，名稱與內容都不要使用 `shadowverse` 當專案名。

## 目錄與職責

| 路徑         | 職責                                                                              | 技術        |
| ------------ | --------------------------------------------------------------------------------- | ----------- |
| `card-data/` | 已審查的日英解析卡片輸入與來源證據                                                | JSON        |
| `carddb/`    | 爬取、解析、合併 `authored/`、建置 SQLite、匯出 JSON 快照                         | Python、uv  |
| `publish/`   | 讀已驗證的公開匯出根，上傳 R2 與遠端 GC；獨立 uv 專案                             | Python、uv  |
| `authored/`  | 人寫的資料：跨地區卡片 ID 對應、繁中翻譯、效果 DSL 資料、裁定登錄                 | YAML        |
| `dsl/`       | 效果 DSL 的 JSON Schema，**語法的唯一權威**                                       | JSON Schema |
| `sim/`       | 模擬器：`engine/`、`scenario-runner/`、`web/`；`server/` 尚未建立                 | Rust、TS    |
| `docs/`      | 進版控的正式文件：ADR（`docs/adr/`）、DSL 規格（`docs/dsl/`）、schema、mermaid 圖 | Markdown    |

專案用語（卡表快照、啟動包、分片、版次等）以根目錄 `GLOSSARY.md` 為準，新增或改名先在那份寫一兩句定義與「避免用」別名，不放實作細節。
記錄架構決定時依 [ADR 規則](docs/adr/README.md) 判斷是否同時符合難以回頭、需背景解釋及真實取捨；規格細節留在對應契約。
新增或修改 ADR、名詞表時參考 `mattpocock-skills` 的 `domain-modeling` skill（Claude Code 由 `.claude/settings.json` 啟用）；名詞表用繁體中文，與 skill 預設不同處以本 repo 規則為準。
`docs/` 只放完成後仍成立的規格與決定；進度、排程、交付點與待決事項放 GitHub issue／milestone。

依賴方向：`dsl/` ← `authored/` ← `carddb/` → 匯出的快照 ← `sim/`；`card-data/` → `carddb/`。
`card-data/` 保存日英解析輸入；產生器與資料變更 CI 驗證 schema、hash、閉包及歷史，普通建置只讀取已驗證資料；`carddb` 匯出的有版號快照仍是使用者與模擬器的**唯一卡片資料權威**。`sim/` 可以載入、打包或快取快照（例如 PWA 離線），
但**不要維護另一份獨立的卡表**。

`carddb/src/sve_carddb/` 依階段分層：基礎 `core/`、`contracts/`；來源 `ingest/`（`crawl/`、`http/`、`archive/`）、`parse/`；
建置 `build/`；資料領域 `domains/`；產出 `images/`、`export/`；最上層 `workflows/` 與 CLI。
`publish/` 只透過 `sve_carddb.export.read_api` 讀匯出。匯入方向由 CONTRIBUTING〈Setup〉列出的邊界測試強制，規則以測試為準。

### 前端與部署邊界

- **查卡、建牌、對戰是同一個前端**（`sim/web`），放在同一個主網域，用路徑區分（例如 `/cards`、`/decks`、`/play`）。不要另外做一個查卡網站
- 卡圖、語音、卡表快照放在 Cloudflare R2，由 `cdn.` 子網域提供
- 對戰伺服器（`sim/server`）放在 `ws.` 子網域

## 資料放在哪裡

| 資料                                       | 位置                                    | 進 git        |
| ------------------------------------------ | --------------------------------------- | ------------- |
| latest cache（HTML、卡圖、語音）           | repo 外，`SVE_DATA_DIR`                 | ❌ 可替換副本 |
| 抓取 manifest                              | repo 外，`SVE_DATA_DIR`                 | ❌ 不可刪     |
| 歷史 raw、來源版本 inventory、凍結輸入批次 | repo 外，來源歸檔 store                 | ❌ 不可刪     |
| 可以隨時刪掉的暫存                         | `carddb/.cache/`                        | ❌            |
| 建置產物（SQLite、JSON 快照）              | `carddb/dist/`                          | ❌            |
| 人寫資料                                   | `authored/`                             | ✅            |
| 日英解析卡片輸入與來源證據                 | `card-data/`                            | ✅ 審查後保存 |
| 官方 SVE 日英卡文                          | `authored/` 證據資料、測試 fixture 等   | ✅            |
| 私有其他遊戲案例與歷史測試卡表             | 專用私有 GitHub testdata repo           | 見下述流程    |

`SVE_DATA_DIR` 的 latest cache 只保存可替換的工作副本；清理或替換前，所需來源版本須已歸檔並可從獨立備份驗回。
**抓取 manifest 與歷史 raw／來源版本 inventory 都不可刪**；官網回寫後無法靠重抓還原，建置 DB 可重建不代表來源歷史可丟棄。
HTML、PDF、API JSON 與卡圖 PNG 均屬凍結來源；WebP 已發布不代表原 PNG 可刪。
歸檔以內容 hash 去重；圖片的大量 hash／複製在鎖外準備，鎖內重驗來源並封存 inventory 與 SQLite backup API 副本。
每批歸檔即備份並驗 restore；allow-root 僅授權讀取，私人儲存與備份路徑不進文件。完整契約見 [來源歸檔與凍結輸入](docs/schema/ingest/source-archive.md)。

原則：**能用程式重新產生的不進 git；專案共用的人寫資料與正式文件進 git。**
官方 SVE 日文與英文卡文可以進 git，並記錄來源與版本；卡圖與語音仍不進 git。
`card-data/` 是可由程式產生但須保留審查歷史的明示例外；原始歸檔與備份仍須保存。
產生、驗證或更新解析輸入時，依[解析卡片輸入契約](docs/schema/ingest/parsed-card-inputs.md)檢查完整閉包與前一版歷史；只保存新解析結果，不覆寫舊來源觀測。
此決定不涵蓋 SV1、Worlds Beyond 等其他遊戲的原檔。
官方內容仍排除於專案的 Apache／CC0 授權之外，依 [LICENSING.md](LICENSING.md) 標示。
個人設定、秘密、本機的研究筆記不屬於這個 repo。
`authored/` 依卡包切檔（例如 `authored/rules/effects/BP01.yaml`），單檔不要超過 1 MiB（1024 KiB）。

16 個 SVE 日英官方頁面 fixture 存在 `carddb/tests/fixtures/official_jp/`、`official_en/`，來源說明及原有逐欄 hash 保存在旁邊；公開與 fork 測試都執行。合成案例繼續驗證結構與邊界。
引擎測試由 carddb 讀取 `card-data/` 及 `tests/engine/card-input.json` 明示的 JP 批次，產生暫存 JSONL；不提交衍生卡表。引擎仍自行載入 authored 效果、規則名稱與 token 選用，不另造一套卡表規則。
本機 `cargo-test` hook 與 Rust CI 共用 `.github/ci/rust_tests.py`，每次重產輸入且清除暫存；缺資料必須失敗。Rust 不需要私有 checkout 或 deploy key，主 repo 與 fork 均跑完整 workspace，行覆蓋率門檻 90%。
永久私有 testdata 保留舊 JSONL、19 個原檔及全部歷史，不能刪除或改公開；Python 現在只讀其中 F17–F19 三個其他遊戲案例。
私有測試輸入以 `.github/ci/testdata.lock` 釘 repo／完整 commit，`private-pages.json` 釘各檔 SHA-256 與逐欄期望；CI 以唯讀 deploy key 取得指定 commit 並驗 hash。可信任 Python job 缺資料、憑證或 hash 不符即失敗。
更新私有案例時先新增私有 repo commit、保留舊歷史，再以 sve-kit PR 更新 pin。
主分支與專案自己的 Python PR 跑完整測試，行與分支覆蓋率合計門檻 90%（本機 hook 與 CI 相同）；fork 依來源 repo 判定，以 `excluded` 排除三個私有案例，仍執行公開 SVE 與合成案例。
Python fork 使用 `.github/ci/fork-coverage.json` 的獨立門檻，summary 標明排除範圍，不宣稱完整覆蓋率驗收通過；Rust 不使用 fork 降低門檻。
測試卡文與含卡文的衍生產物不放 Actions cache／artifact；失敗 log 不印整行卡文。私有存放只是存取控制，不等於授權；公開保存也是專案的風險決定，不是官方許可。授權相關 PR 仍須審查並在合併前交維護者確認。

## 已定案的設計原則

- **卡片 ID 跨地區對應一律人工確認**，存在 `authored/`。日英卡號不能靠去掉 `EN` 後綴配對
  （實測 BP02 從 070 起錯一號、BP18-SP01 日英是不同卡、PR 編號各自獨立）
- **地區只有 `jp` 與 `en`**，不收簡體中文版
- **卡號照官網原樣保存**（含 `Ⓢ`、小寫 `a`）；卡圖網址**從頁面 `<img src>` 原樣抓**，不能由卡號推算
  - 例外：一代數位版官網 shadowverse-portal.com（`parse/pages/official_sv1.py`，只用來對應數位卡）的卡圖網址格式固定，
    用模板組網址；抓不到（官網會 302 轉到 HTML 頁，或 404）就改抓該卡的卡片頁、取實際的 `<img src>` 再試。
    合成頁測試模板與 fallback；私有真實頁測試驗證已鎖定的來源欄位
- **SQLite 只在建置時用來檢查資料完整性**，使用者拿到的是 JSON 快照
- 資訊洩漏原則：用戶端只能拿到公開資訊；未公開的對手手牌不能有可追蹤的固定 ID

## 型別邊界

以下函式庫的回傳型別不可信，**一律透過邊界函式存取**，其他程式碼不直接使用原始回傳值：

| 函式庫 | 問題 | 做法 |
| ---------- | --------------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| selectolax | `css_first(query)` 的型別是 `LexborNode`，找不到元素時實際回傳 `None` | 使用 `sve_carddb.parse.html` 的 `select_one`／`require_one`，**不要直接呼叫 `css_first`** |
| sqlite3 | 查詢結果是 `Any`，型別檢查在這裡失效 | 各 SQLite 邊界模組（manifest、建置資料庫等）做執行期檢查並轉成型別物件 |

SQLite 的原始 `Any` 不得離開邊界模組；其他層只使用已驗證的型別物件。

## 爬取規則

- 請求之間至少間隔 2 秒，帶瀏覽器 User-Agent（官網的 CloudFront 不帶會回 404）
- **只抓新增或變動的內容**，原始資料已存在就不重抓
- `crawl --mode refresh` 會覆寫既有 latest 原始檔；須依[受保護抓取的操作條件](docs/schema/ingest/refresh-operation.md)完成前置條件並經維護者明示同意才可執行，不能以已有 manifest backup 代替[來源歸檔保護](docs/schema/ingest/source-archive.md#4-refresh-與中斷恢復)。
- 每個抓下來的檔案都要在 manifest 記錄來源網址、抓取時間、ETag、雜湊值

## 開發指令

從 repo 根目錄執行：

```bash
uv --directory carddb sync                 # 安裝依賴
uv --directory carddb run pytest           # 測試
uv --directory carddb run ruff check       # lint
uv --directory carddb run mypy             # 型別檢查
cargo clippy --locked --workspace --all-targets -- -D warnings   # Rust lint
python3 .github/ci/rust_tests.py           # 產生公開測試輸入＋Rust 完整覆蓋率（90%）
cargo deny check && cargo machete          # 依賴的安全公告與未用依賴（pre-push 也會跑）
```

Rust 工具鏈版本釘在 `rust-toolchain.toml`，升級時一併改 `Cargo.toml` 的 `rust-version`。

## 慣例

- Python 套件一律用 `uv`，不要 `pip install`
- 搜尋用 `rg`，不要用 `grep`
- Markdown：code block 要標語言；表格 pipe 兩側要有空格

### 語言、commit、註解

以 `CONTRIBUTING.md` 為準，這裡不重複：註解用英文且只寫「為什麼」、ADR 與設計文件用繁體中文、
commit 格式由 commitizen 檢查。審核依 CONTRIBUTING 的〈Coding standards〉。

### i18n

- 介面語言：繁中、日文、英文，之後可擴充
- **卡面語言與介面語言分開設定**；某語言缺翻譯時要有退回規則，不能顯示空白

## AI 貢獻與審查

- 在 PR 描述中列出使用的 AI 工具與模型版本、負責提交者，以及已知來源。
  產生的內容必須經過審查；不要公開私人提示詞或秘密。AI 產生內容或專案接受內容，
  都不能證明擁有權利，也不代表已取得原作的使用授權。
- 維護者執行的 AI 模型透過維護者持有的 bot 帳號提交。
  推送與建立 PR 前，AI 審查必須先在本機分支通過；之後由審核者在 PR
  發布一則涵蓋所有審查輪次、修正與驗證的總結。
- 使用下表的 trailer 記錄每位真人或 AI 貢獻者與審核者。不要捏造過去的核可；
  `Acked-by` 只用於維護者確實親自核可該變更的情況。
  這些紀錄不構成法律上的作者認定、權利移轉或 DCO 聲明。
- bot 提交 PR 時，權利聲明的勾選框保持未勾選，並註明維護者須確認該 PR 的來源與授權範圍。
  不得因專案採用 Apache-2.0 或 CC0 就推定已完成確認，也不得代替真人聲明權利或核可。
  若維護者已在別處明確確認，應連到該紀錄，不得宣稱是 bot 代真人作出的確認。
- 每個授權相關 PR 都必須通過審查，並在合併前給維護者看過。
  政策定案不能取代對實際 PR 的審閱。

PR 採 squash merge；合併後的 commit 訊息在正文與 issue 參照之後空一行，
再依下列順序記錄 trailer：

| Trailer | 記錄對象 | Example |
| --- | --- | --- |
| `Co-Authored-By` | 每位撰寫部分變更的真人或 AI 模型 | `Co-Authored-By: Codex gpt-6-sol <noreply@openai.com>` |
| `Reviewed-by` | 每位核可最終版本的真人或 AI 審核者 | `Reviewed-by: Claude Opus 5.5 <noreply@anthropic.com>` |
| `Acked-by` | 維護者，僅限確實親自核可該變更時 | `Acked-by: Maintainer Name <maintainer@example.com>` |

AI 模型以產品名稱與版本標示，維護者則使用 `git log` 中的姓名與電子郵件地址。
不要捏造身分，也不要將私人聯絡資訊加入公開文件。
