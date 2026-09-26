# AGENTS.md

給 AI 開發工具（Claude Code、Codex 等）的專案說明。**這份是專案架構與資料規則的權威**；
語言、commit、註解規則以 `CONTRIBUTING.md` 為準。`CLAUDE.md` 只負責引用這兩份，不要在其他地方重複寫規則。

這份會進版控，只寫**任何 clone 這個 repo 的人都適用**的規則。
個人機器上的路徑、本機限定的檔案、個人工作流程，**不要寫進這裡**。

## 這是什麼

Shadowverse: EVOLVE（實體卡牌遊戲，簡稱 SVE）的非官方工具組，一人專案。

- **卡表**：爬日文與英文官網，合併人工資料，匯出有版號的 JSON 快照
- **模擬器**：網頁版對戰（之後可能打包成 PWA、Tauri）

不是官方產品，名稱與內容都不要使用 `shadowverse` 當專案名。

## 目錄與職責

| 路徑        | 職責                                                            | 技術        |
| ----------- | --------------------------------------------------------------- | ----------- |
| `carddb/`   | 爬取、解析、合併 `authored/`、建置 SQLite、匯出 JSON 快照       | Python、uv  |
| `authored/` | 人寫的資料：跨地區卡片 ID 對應、繁中翻譯、效果 DSL 資料         | YAML        |
| `dsl/`      | 效果 DSL 的 JSON Schema，**語法的唯一權威**                     | JSON Schema |
| `sim/`      | 模擬器（尚未開始）：之後會有 `engine/`、`server/`、`web/`       | Rust、TS    |
| `docs/`     | 進版控的正式文件：ADR、schema、DSL 規格、mermaid 圖（尚未建立） | Markdown    |

依賴方向：`dsl/` ← `authored/` ← `carddb/` → 匯出的快照 ← `sim/`。
`carddb` 匯出的有版號快照是卡片資料的**唯一權威**。`sim/` 可以載入、打包或快取快照（例如 PWA 離線），
但**不要維護另一份獨立的卡表**。

### 前端與部署邊界

- **查卡、建牌、對戰是同一個前端**（`sim/web`），放在同一個主網域，用路徑區分（例如 `/cards`、`/decks`、`/play`）。不要另外做一個查卡網站
- 卡圖、語音、卡表快照放在 Cloudflare R2，由 `cdn.` 子網域提供
- 對戰伺服器（`sim/server`）放在 `ws.` 子網域

## 資料放在哪裡

| 資料                                 | 位置                                    | 進 git      |
| ------------------------------------ | --------------------------------------- | ----------- |
| 原始 HTML、卡圖、語音、來源 manifest | repo 外，由環境變數 `SVE_DATA_DIR` 指定 | ❌ 臨時中轉 |
| 可以隨時刪掉的暫存                   | `carddb/.cache/`                        | ❌          |
| 建置產物（SQLite、JSON 快照）        | `carddb/dist/`                          | ❌          |
| 人寫資料                             | `authored/`                             | ✅          |

`SVE_DATA_DIR` 是上傳 R2 之前的臨時中轉站：卡圖、語音上傳 R2 並確認後可刪；
原始 HTML 建議保留（重新解析用）；**manifest 不可刪**（唯一無法重建的資料）。

原則：**能用程式重新產生的不進 git；專案共用的人寫資料與正式文件進 git。**
個人設定、秘密、本機的研究筆記不屬於這個 repo。
`authored/` 依卡包切檔（例如 `authored/effects/BP01.yaml`），單檔不要超過 1 MiB（1024 KiB）。

## 已定案的設計原則

- **卡片 ID 跨地區對應一律人工確認**，存在 `authored/`。日英卡號不能靠去掉 `EN` 後綴配對
  （實測 BP02 從 070 起錯一號、BP18-SP01 日英是不同卡、PR 編號各自獨立）
- **地區只有 `jp` 與 `en`**，不收簡體中文版
- **卡號照官網原樣保存**（含 `Ⓢ`、小寫 `a`）；卡圖網址**從頁面 `<img src>` 原樣抓**，不能由卡號推算
  - 例外：一代數位版官網 shadowverse-portal.com（`sources/official_sv1.py`，只用來對應數位卡）的卡圖網址格式固定，
    用模板組網址；抓不到（官網會 302 轉到 HTML 頁，或 404）就改抓該卡的卡片頁、取實際的 `<img src>` 再試。
    測試用真實卡片頁確認模板仍與頁面一致
- **SQLite 只在建置時用來檢查資料完整性**，使用者拿到的是 JSON 快照
- 資訊洩漏原則：用戶端只能拿到公開資訊；未公開的對手手牌不能有可追蹤的固定 ID

## 型別邊界

以下函式庫的回傳型別不可信，**一律透過邊界函式存取**，其他程式碼不直接使用原始回傳值：

| 函式庫     | 問題                                                                  | 做法                                                                                |
| ---------- | --------------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| selectolax | `css_first(query)` 的型別是 `LexborNode`，找不到元素時實際回傳 `None` | 使用 `sve_carddb.html` 的 `select_one`／`require_one`，**不要直接呼叫 `css_first`** |
| sqlite3    | 查詢結果是 `Any`，型別檢查在這裡失效                                  | 資料列在 manifest 模組內轉成有型別的物件，並做執行期檢查                            |

## 爬取規則

- 請求之間至少間隔 2 秒，帶瀏覽器 User-Agent（官網的 CloudFront 不帶會回 404）
- **只抓新增或變動的內容**，原始資料已存在就不重抓
- 每個抓下來的檔案都要在 manifest 記錄來源網址、抓取時間、ETag、雜湊值

## 開發指令

從 repo 根目錄執行：

```bash
uv --directory carddb sync                 # 安裝依賴
uv --directory carddb run pytest           # 測試
uv --directory carddb run ruff check       # lint
uv --directory carddb run mypy             # 型別檢查
cargo clippy --locked --workspace --all-targets -- -D warnings   # Rust lint
cargo llvm-cov --locked --workspace --fail-under-lines 90   # Rust 測試＋行覆蓋率門檻（pre-push 也會跑）
cargo deny check && cargo machete          # 依賴的安全公告與未用依賴（pre-push 也會跑）
```

Rust 工具鏈版本釘在 `rust-toolchain.toml`，升級時一併改 `Cargo.toml` 的 `rust-version`。

## 慣例

- Python 套件一律用 `uv`，不要 `pip install`
- 搜尋用 `rg`，不要用 `grep`
- Markdown：code block 要標語言；表格 pipe 兩側要有空格

### 語言、commit、註解

**以 `CONTRIBUTING.md` 為準**（必讀），這裡不重複。重點：註解用英文且只寫「為什麼」、
ADR 與設計文件用繁體中文、commit 格式由 commitizen 檢查。

### i18n

- 介面語言：繁中、日文、英文，之後可擴充
- **卡面語言與介面語言分開設定**；某語言缺翻譯時要有退回規則，不能顯示空白
