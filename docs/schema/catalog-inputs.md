# 職業、卡種的正式 catalog 輸入

本文件固定 #51 後續 catalog 資料與程式的五項邊界，依 2026-10-02 協調者在維護者委託下採納的 Opus 計畫修正；不是維護者親自核可資料。正式採納仍依[詞彙與路由契約](catalog-route-adoption.md)及[翻譯契約](translation-contract.md)。此文件不配發資料、不宣稱 loader 已接妥，也不修改公開快照欄位。

## 1. 位置、模型與建置釘選

caller 的定義檔放 `authored/catalog/catalog.yaml`，原文字段對應放 `authored/catalog/bindings.yaml`；使用既有 `catalog.models.Catalog` 與 `text_observations.vocabulary.Vocabulary`，不另造一套詞彙模型。YAML 1.2 經既有嚴格讀取邊界轉 canonical 資料再驗模型，不將私有 JSON 工作檔直接當正式入口。

兩檔只承載定義／binding；採納收據沿 catalog-route-adoption 的獨立封套／索引／歷史規則，不把人名塞進既有模型欄位。建置須同時釘完整 authored revision、兩檔及所用收據的 exact bytes／canonical hash、模型／parser 依賴與配置，記入 F1；缺收據、hash 不符或未支援的入口拒絕，不能靠 caller-pinned 取代有效採納。

## 2. 永久代碼與配發收據

`(kind,code)` 是公開、永久的識別；譯名修改不重配、停用不重用、相同文字不合併概念。首次配發須有精確成員／payload hash、實際決定者、日期精度及授權範圍的收據。協調者代配須另保存維護者的具體委託依據／日期／scope，記實際協調者事件，不冒作維護者親自核可；這是 **catalog code 配發**的明示授權，不沿用 glossary 的 mode 或擴張其他採納門檻。來源原值映射仍須完成 catalog-route-adoption 的來源與核對要求。

正式 code 限以下七職業與八基本卡種。清單是資料配發目標，文件本身不代替逐筆收據：

| kind | code 清單 |
| --- | --- |
| class | elf、royal、witch、dragon、nightmare、bishop、neutral |
| type | follower、spell、amulet、crest、equipment、leader、ep、sep |

保留現有 preview 已使用的六職業及 follower／spell／amulet／leader；neutral 與其餘基本卡種補正式 code。`raw_jp_…`／其他 `raw_…` 是沒有正式配發收據的暫碼，不繼承為永久識別。已正式採納的 code 若要改，須另審相容／遷移及資料改版，不用此次 preview 修正偷偷重配。

## 3. JP／EN 原文到基本型別與特殊標記

以 Opus 審查過的現有 preview 詞彙檔為起點，保存其內容 hash／所屬 preview 版本供對照；私有路徑不進 repo。逐項核對已有 binding，修正暫碼並補完整對應，不從零生成或從 label 猜 code。

JP 與 EN 共用同一份 bindings，鍵為 `(region,kind,raw exact)`；JP 原文與對應 EN 原文指同一 code。職業的原文對照可參考 `registry/inputs.py::CLASSES`，但那張表不是配發收據；`-` 是缺值，不能當 neutral。資料須從凍結來源驗回 exact 原文，不能只依那張一致性檢查表宣布已採納。

type binding 明列「完整原文寫法 → 基本 type code＋special_kinds」。特殊標記限 `evolve/advance/token`，須有相應有效 `special_kind` 定義；不把它們當基本卡種或繁中選詞。例：已驗的「フォロワー・エボルヴ」／「Follower / Evolved」對應 `follower`＋`[evolve]`；「イクイップメント・トークン」對應 `equipment`＋`[token]`。例子仍需逐項來源／拼法核對，不算實際資料已採納。

不靠通用 split／trim／大小寫轉換猜新 spelling。相同 raw 鍵不得有不同對應；特殊標記不得缺定義或重複。未對應原文一律拒絕／列缺項，不再產生 `raw_…` 暫碼。新增原文寫法須新核對收據，不把舊範圍當通用授權。

## 4. 15 筆繁中選詞

七職業、八基本卡種的 zh-Hant 選詞沿既有 `translations/` 的 `vocabulary_choice`，不把翻譯塞進 catalog label，也不擴大其 class/type 白名單。來源／origin、委託或本人決定、續版／撤回與精確 scope 依翻譯契約；首次 code 配發收據不能代簽繁中選詞。

resource EP／SEP 與 type EP／SEP、trait 精霊與 class エルフ即使同譯仍是不同引用，不合併 glossary。EN 原值 binding 是地區資料對應，不等於此次採納英文術語翻譯。模板引用缺有效繁中選詞時，整個該 context 回原文並報 `missing_term_translation`；UI label 的 fallback 不套用卡文。

## 5. preview 與前端驗收

正式 catalog 完成後以同一份 JP／EN 對應重建 preview，重驗 face 的 class／type／special_kind 閉包；保留舊預覽供比對，不改寫原 raw 或封存來源。報告逐項列出保留代碼、暫碼替換與新增標記，不能只比較詞彙筆數。

前端核對職業快速列、卡種篩選、特殊標記與牌組條件使用同一正式 code；包含 neutral、ep／sep、equipment／crest 及進化／進階／衍生物實例。驗證 JP／EN 原文落到相同概念、全部未知 raw 拒絕、缺有效選詞整 context 回原文。既有正式 code 不改，暫碼修正後產新 preview 資料版；不將舊預覽宣稱已完成正式配發。
