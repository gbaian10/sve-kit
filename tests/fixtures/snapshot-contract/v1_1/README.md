# 1.1.0 合成契約資料

本目錄保留 1.0.0 手寫合成 golden 的完整邏輯值，以獨立離線 wire 重切產生
固定 N=64／band 的傳輸例。產生程序沒有呼叫 exporter；Python 的 independent
reader 驗 join 與 `v1/expected-logical.json` 逐欄相等。資料不含官方卡文。

JSON 是方便審查的縮排表示；驗 hash／bytes 前轉成 canonical-json-v1。
`payloads/` 的檔名是 canonical payload hash，manifest 的 path 是公開 blob 路徑。
`text-all.json` 保留精確 File／hash 聯集。`bucket-cases.json` 明列 UTF-8
canonical 陣列、完整 SHA-256 與 N=64 餘數。新 Schema 封套只接受明示的
1.1.0 capability／N；公開邏輯欄序、PK、nullable 與 joins 沒有改變。

無列的集合沿既有規則視為空；多個 logical buckets 合進同一物理 File 時仍
保留各自 fragment 身分、row_index 與必要 types。這份共享例供後續 web
reader 的相容測試使用；原 1.0.0 Schema 與 golden 保留。
