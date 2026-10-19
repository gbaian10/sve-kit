# 引擎測試輸入

`card-input.json` 明示公開 `card-data/` 的 JP 批次。執行
`python3 .github/ci/rust_tests.py` 或手動 `cargo-test` hook，會由 carddb 的
`parsed-cards engine-fixture` 產生暫存 JSONL，再執行完整 Rust 測試；結束後刪除暫存。
每列只有 `number` 與依官網面序保留的 `faces`，不收 EN、不猜跨地區對應，也不提交衍生卡表。
原本引擎載入 authored 效果、規則名稱與 token 選用的行為不變。

`authored/rules/engine/index.yaml` 的 `input.snapshot_sha256` 釘選這份確定性產物，
不能以改 hash 取代資料審查。變更批次、投影或資料時，須比較全部面欄位並通過引擎測試後才更新 pin。
目前基線保留 7,369 張、7,383 個面。相較舊私有測試卡表，既有面欄位只有 CP04 的 29 張族群有意修正：
`〈ジオ・ゲヘナ〉`、`〈ジオ・テオゴニア〉`、`〈ジオ・ニヴルヘル〉`、`〈レイジ・レギオン〉`
保留為整個族群，不在括號內的中點拆開，與已審查 `trait_raw` 相符。卡型、費用、攻擊、體力、名稱、文字及面序保持一致。
`trait_raw` 是新增來源欄位，供核對來源，不改引擎選用規則。

舊私有卡表仍永久保留。題庫來源 metadata、registry 歷史證據與 v0 效果分類引用的舊 SHA-256
描述當時的來源，不隨引擎目前的測試輸入 pin 改寫。此次沒有改動 scenario 的預期答案或已知失敗清單。
