# carddb 程式分層

`carddb/src/sve_carddb/` 按下列職責分層；正式匯入邊界由 CONTRIBUTING 所列架構測試驗證。

| 中文 | 英文（程式／檔名） | 指什麼 |
| --- | --- | --- |
| **共用基礎** | core | carddb 各層共用的 JSON／YAML 邊界、嚴格型別、日期、authored 路徑與純來源溯源資料；不依賴卡片語意或建置 DB |
| **共用契約** | contracts | build 與 export 共用的公開快照 schema、profiles、欄位 descriptor 與模板參數形狀；只依賴共用基礎，不包含辨識規則或建置／匯出流程 |
| **來源取得** | ingest | 抓取協調、HTTP、manifest 與不可變來源歸檔；抓取協調可呼叫純解析，低階 HTTP／歸檔不依賴 crawler 或工作流程 |
| **純解析** | parse | 來源 bytes 轉成欄位與觀測；不載入 authored、不建置 DB；身分與 QA 的領域適配留各領域 |
| **建置基礎設施** | build | DB schema、編譯、型別化資料列、完整性檢查與建置輸出；scalar 值域集中在 scalars 模組 |
| **資料領域** | domains | authored 載入、官網資料合併、領域驗證與 DB 投影；翻譯和數位版各按職責分子套件，不依賴 workflows 或 CLI |
| **工作流程** | workflows | 串接來源讀取、領域準備、建置及匯出的跨階段入口；下層不依賴此層或 CLI |

`publish/` 只透過 `sve_carddb.export.read_api` 讀取已驗匯出；資料權威與跨元件依賴依 [AGENTS.md](../../../AGENTS.md)。
