# 卡文版本與依據版本契約

版次對應的官網原文與翻譯／DSL／搜尋的共享依據分開保存。
跨區身分仍由 registry 人工確認，不從卡號配對。公開欄位依
[快照格式 §2.3](../export/snapshot-format.md#23-卡文版本與依據版本)。

## 1. 比較與選用

有可信完整日精度日期的版次參與比較；未知日期版次保留自己的觀測，但不參與最新日期判斷亦不阻擋。
機械規則保留名稱、類別、種類、數值、特徵、作品標籤與補充段落順序，只忽略效果文字的空白、句子標點與
已識別關鍵字的括號提醒。數字、正負號、任意括號條件與沒有識別的提醒不刪除。
內容 exact 相同與比較後等價是不同概念；來源與文字單元永遠保留 exact UTF-8。

非機械差異由 AI 逐項記錄理由，分類為 same_effect、pending_review、errata 或 unresolved。
AI same_effect 可建立供審閱的依據版本，合併前仍交維護者確認；method=ai 不表示真人核可。
可能不同的效果分類 errata，交由勘誤版本／印刷版本功能呈現。人工紀錄優先於機械與 AI 判定。
最新日期若有多份 exact 不同卡文，維持 unresolved；不能用 hash 或 ID 排定發售順序。

## 2. 判定輸入

可選入口為 `authored/wording/<filing_key>/<sequence>.yaml`。
filing_key 為 ASCII 字母、數字、底線或連字號，sequence 為三位以上數字，單檔小於 1 MiB。
遵守 authored 的嚴格 YAML、禁止 symlink 及封閉欄位規則；同 face_id／region 重複紀錄拒絕。
檔名只作分片，不表示判定年代。不存在 wording 目錄等於沒有額外判定。

```yaml
format: 1
kind: wording_decisions
records:
  - face_id: f:synthetic
    region: jp
    comparison_hash: sha256:0000000000000000000000000000000000000000000000000000000000000000
    classification: pending_review
    method: ai
    reason: 合成案例的條件範圍需要確認
```

| 欄位 | 規則 |
| --- | --- |
| face_id、region | 本次公開的待比較卡面與地區；找不到相應組合即失敗 |
| comparison_hash | 全部觀測 FaceContent fingerprint 的去重排序集合之 canonical JSON SHA-256；包括日期不明觀測，不保存原文 |
| classification | same_effect／pending_review／errata／unresolved |
| method | ai／manual；只有維護者確實判定才記 manual |
| reason | 非空的差異判斷理由，不抄官方卡文、不記私人路徑或捏造核可 |
| basis_printing_id | 選填；僅 manual 且 same_effect 可指定，用於覆寫日期歧義，該版次須有唯一非 null 觀測 |

comparison_hash 與本次內容不符即拒絕，不靜默重用舊判定。內容更改後須重新審閱、重算並修改記錄。
同一選擇鍵直接修改，Git 保存歷史。人工紀錄可否決自動等價；不需再存官方原文副本。

## 3. 建置與公開驗證

建置 `face_wording` 以 face_id／region 為鍵，存分類、方法、理由、比較 hash、依據版次與日期。
same_effect 且有唯一依據時寫 `face_current.basis=latest_known_release`；日期不明的人工指定亦可適用。
既有採納 current 優先，沒有明示人工 basis 不覆寫。缺來源、身分問題及未解更正不得取得新依據。

公開 same_effect 的依據必能連回候選版次的相同 revision 與 face.current，日期須與商品證據一致。
非人工依據須有完整日精度日期且為最新日；其他分類的依據版次與日期均 null。
公開 reason 可作稽核，但 comparison_hash 只留建置及 authored。沒有共享 current 的必要卡面保持 wording_pending 區域阻擋。

全部觀測候選的名稱與數值進啟動包，使沒有 current 的版次亦能顯示自己的卡名與數值。
單卡頁的原文用版次候選，翻譯來源、效果實作及搜尋用共享依據；無依據者不取得自動規則資格。
