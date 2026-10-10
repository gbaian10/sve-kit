# 架構決策紀錄（ADR）

只在三個條件同時成立時寫 ADR：難以回頭、缺少背景會令人不解，而且確實比較過有取捨的替代方案。
一份只記一個決定，以一段一至三句交代背景、選擇與理由；標題直接寫決定。
只有能補充實質資訊時才加 `Status` frontmatter、Considered Options 或 Consequences。
欄位、流程及驗收規格放 `docs/schema/` 或 `docs/dsl/`，ADR 連到它們；排程、進度及待決問題放 issue／milestone。

全 repo 共用序號，檔名為 `NNNN-<主題>.md`，新增時取現有最大編號加一，內文用繁體中文。
決定改變時新增一份，舊文以 frontmatter `Status: superseded by ADR-NNNN` 指向新決定；不在舊文底下疊落地註記。
文件整理可修正文字與連結，不應讓舊決定和現行規格相互覆蓋。
專案用語依[名詞表](../terminology.md)；來源與引用界線依[文件引用說明](../quotations.md)。

| 編號 | 決定 |
| --- | --- |
| [ADR-0001](0001-engine-skeleton.md) | 引擎只維護一份權威狀態 |
| [ADR-0002](0002-author-ir.md) | 效果只從單一撰寫語法降低成有型別 IR |
| [ADR-0003](0003-decisions.md) | 輸入點依規則階段保留 |
| [ADR-0004](0004-capture.md) | 讀值捕捉與條件求值分離 |
| [ADR-0005](0005-object-ref.md) | 以世代限制物件引用與跨區追蹤 |
| [ADR-0006](0006-receipts.md) | 動作使用有型別結果而非成功布林 |
| [ADR-0007](0007-history.md) | 以適正打出的單一事實記錄 UB |
| [ADR-0008](0008-continuous.md) | 持續特徵求值與事件提交分離 |
| [ADR-0009](0009-rulings-evidence.md) | 集中裁定並逐結論保留證據強度 |
| [ADR-0010](0010-version-meta.md) | DSL 語義版本與驗收證據分開記錄 |
| [ADR-0011](0011-license-policy.md) | 按路徑授權自有貢獻並排除第三方內容 |
| [ADR-0012](0012-image-url-version.md) | 卡圖採永久版次路徑與按需版本 |
| [ADR-0013](0013-snapshot-retention.md) | 公開快照留兩版而卡圖只留當前 |
| [ADR-0014](0014-translation-validation.md) | 翻譯以當前值檢查取代核可帳本 |
| [ADR-0015](0015-upload-from-export.md) | 上傳直接讀取匯出閉包 |
| [ADR-0016](0016-four-layer-translation.md) | 翻譯與 DSL 共用四層來源綁定 |
