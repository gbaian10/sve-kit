# 卡文版本與依據版本契約

版次對應的官網原文與翻譯／DSL／搜尋的共享依據分開保存。
跨區身分仍由 registry 人工確認，不從卡號配對。公開欄位依
[快照格式 §2.3](../export/snapshot-format.md#23-卡文版本與依據版本)。

## 1. 比較與選用

主文空白表示卡面未印文字，不參與差異判斷，也不能在同一卡面／地區有非空主文時作為共享依據；
自己的空白原文與觀測仍保留。全部版次都空白時維持空白。缺來源的 null 與空字串不同，不能當成全圖版。

非空版次通常由可信完整日精度日期選用；未知日期觀測保留，但不參與最新日期判斷亦不阻擋。
若同一卡面／地區同時有空白與非空主文，且沒有任何非空版次的完整日期，則比較全部非空主文，
效果相同時按既有 candidates 的 printing_id／revision_id 穩定排序選非空依據，不假稱日期已知。
非空版次仍須通過來源完整、身分與更正檢查。現行卡文的選用與效果差異分類分開；選用不宣稱不同版次等義。
機械規則保留名稱、類別、種類、數值、特徵、作品標籤與補充段落順序，只忽略效果文字的空白、句子標點與
已識別關鍵字的括號提醒。數字、正負號、任意括號條件與沒有識別的提醒不刪除。
內容 exact 相同與比較後等價是不同概念；來源與文字單元永遠保留 exact UTF-8。

非機械差異由 AI 逐項記錄理由，分類為 same_effect、pending_review、errata 或 unresolved。
AI same_effect 可建立供審閱的依據版本，合併前仍交維護者確認；method=ai 不表示真人核可。
AI 紀錄若只因空主文或日期不明觀測產生差異，不能覆寫有效非空版次的機械等價結果；原判定仍保留供審閱。
可能不同的效果分類 errata，保留供勘誤版本／印刷版本功能呈現，不因此阻擋現行卡文。人工分類仍優先於機械與 AI 判定。

JP、EN 的有日期非空候選依下列優先序選現行卡文：

1. 非文字欄位相同、主文與各補充段落只差已識別的括號提醒時，選沒有提醒的版本；Token 段落同樣適用。
2. 主文相同、差異只有補充段落的有無時，選完整附段落的版本。段落須按原順序構成較完整版本的子序列；
   段落內容不同或順序互換不能當成有無差異。完整段落內若只差提醒，再選提醒較少者。
3. 其他文字差異，包括未識別的提醒、語序、條件或勘誤，選最新已知發售版次，並在選用稽核報告列為其他文字改動。
4. 同一偏好下先取最新完整日精度日期，再按原樣卡號字典序取最小；卡號相同才用 printing_id 穩定排序。
   卡號排序只解除同日選用歧義，不推斷發售年代。

括號提醒須符合已識別的關鍵字／機制定義形式，且相應關鍵字已出現在該括號前的同段文字。
任意括號條件、數字與算術符號仍保留；不能確定差異類型時走第 3 條。
人工明示 basis 仍可選唯一有效觀測；既有已採納非空 current 仍優先。完全沒有可信日期時不任意選取，
只保留前述空白／非空並存且已證明等義的未知日期例外。

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
| comparison_hash | 全部觀測 FaceContent fingerprint 的去重排序集合之 canonical JSON SHA-256；包括空主文及日期不明觀測，不保存原文 |
| classification | same_effect／pending_review／errata／unresolved |
| method | ai／manual；只有維護者確實判定才記 manual |
| reason | 非空的差異判斷理由，不抄官方卡文、不記私人路徑或捏造核可 |
| basis_printing_id | 選填；僅 manual 且 same_effect 可指定，用於覆寫日期歧義，該版次須有唯一非 null 觀測 |

comparison_hash 與本次內容不符即拒絕，不靜默重用舊判定。內容更改後須重新審閱、重算並修改記錄。
同一選擇鍵直接修改，Git 保存歷史。人工紀錄可否決自動等價；不需再存官方原文副本。

## 3. 建置與公開驗證

建置 `face_wording` 以 face_id／region 為鍵，存分類、方法、理由、比較 hash、依據版次與日期。
依上述偏好及日期選定後寫 `face_current.basis=latest_known_release`；此 basis 包含先套文字偏好再取最新日的規則，
不代表所有候選之中的最新日。pending_review、errata 或 authored 的 unresolved 分類不阻擋有效選用，
也不因取得 current 被改成 same_effect；日期不明的人工指定亦可適用。
空白版次的非空原文例外採 `latest_observed_no_errata`，`basis_released_on=null`，保留未知日期的事實。
既有非空採納 current 優先，沒有明示人工 basis 不覆寫；空白 current 在有非空觀測時撤下，依上述規則重選。
人工指定亦不能在有非空主文時選空白版次。缺來源、身分問題及未解更正不得取得新依據。

公開 same_effect 的依據必能連回候選版次的相同 revision 與 face.current，日期須與商品證據一致。
非人工依據須符合上述提醒／補充段落偏好，再以日期與卡號排序；reader 從公開原文及版次資料重算驗證。
只有上述空白／非空並存且沒有非空版次日期的例外允許 null 日期。
reader 從候選原文與商品證據獨立重驗此例外，不依 reason 字串放行。其他分類的依據版次與日期均 null。
公開 reason 可作稽核，但 comparison_hash 只留建置及 authored。沒有共享 current 的必要卡面保持 wording_pending 區域阻擋。

全部觀測候選的名稱與數值進啟動包，使沒有 current 的版次亦能顯示自己的卡名與數值。
單卡頁的原文用版次候選，翻譯來源、效果實作及搜尋用共享依據；無依據者不取得自動規則資格。
版次原文包含主欄與補充段落，保留原標題及 ordinal；補充原文不翻譯，也不借用共享依據或另一版次的段落。
