# 引擎測試輸入

`card-input.json` 明示公開 `card-data/` 的 JP 和 EN 批次。執行
`python3 .github/ci/rust_tests.py` 或手動 `cargo-test` hook，會由 carddb 的
`tests.support.repository_current` 產生暫存 JSONL，再執行完整 Rust 測試，結束後刪除暫存。
每列只有 `number` 與依官網面序保留的 `faces`。JP 保留整批，EN 只收身分登錄已確認無 JP 版次的卡。
引擎另載入 authored 效果、規則名稱與 token 選用，衍生卡表不提交。

測試底座以完整 JSONL 位元組計算 `input.snapshot_sha256`，填入交給引擎的規則檔。
變更批次、投影或資料時，須比較全部面欄位並通過引擎測試。
遷移至公開輸入時，JP 基線保留 7,369 張、7,383 個面。相較舊私有測試卡表，既有面欄位只有 CP04 的 29 張族群有意修正：
`〈ジオ・ゲヘナ〉`、`〈ジオ・テオゴニア〉`、`〈ジオ・ニヴルヘル〉`、`〈レイジ・レギオン〉`
保留為整個族群，不在括號內的中點拆開，與已審查 `trait_raw` 相符。卡型、費用、攻擊、體力、名稱、文字及面序保持一致。
`trait_raw` 是新增來源欄位，供核對來源，不改引擎選用規則。

舊私有卡表仍永久保留。題庫來源 metadata、registry 歷史證據與 v0 效果分類引用的舊 SHA-256
描述當時的來源，不隨引擎目前的測試輸入 pin 改寫。此次沒有改動 scenario 的預期答案。
