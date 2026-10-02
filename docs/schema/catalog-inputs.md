# 職業、卡種的正式 catalog 輸入

本文件細化 #28 的[詞彙採納契約](catalog-route-adoption.md)及 class/type 首批資料；文件與代碼清單不代替逐筆採納。採用 2026-10-02 協調者轉達的契約修正，不記為維護者親自核可資料。只擴充既有入口，不新增 authored 目錄或公開欄位；程式支援另行實作。

## 1. 唯一入口與建置釘選

唯一權威是 `authored/catalog-adoptions/index.yaml` 及 `catalog-adoptions/vocabulary/<filing_key>/<sequence>.yaml` 的有效 `vocabulary_adoption`：subject 為 `{kind,code}`，value 為 `{label,raw_mappings,active}`。YAML、封套、來源證據、決定、依賴、續版鏈及 F1 pins 全依 catalog-route-adoption §2–§4，不另存 `catalog.yaml`／`bindings.yaml`。

既有 `catalog.models.Catalog`／`text_observations.vocabulary.Vocabulary` 是建置時從完整驗妥的有效採納推導的記憶體物件，不是另一份權威檔。typed API、caller configuration 或私有 preview JSON 不提供採納。建置釘完整 authored revision、入口及全部歷史分片的 exact bytes／canonical hash、程式／parser／依賴／配置；工具將當次有效映射與採納追溯寫入 F1。

## 2. 永久代碼

`(kind,code)` 不隨譯名重配，停用亦不重用。首次配發及後續完整 value 修訂須由 repo 明列的維護者確認完整值，包含全部原文映射，沿契約 §2.1 保存精確成員與事件收據；不設委託例外。首批目標如下，清單本身不算配發：

| kind | code 清單 |
| --- | --- |
| class | elf、royal、witch、dragon、nightmare、bishop、neutral |
| type | follower、spell、amulet、crest、equipment、leader、ep、sep |
| special_kind | evolve、advance、token |

保留 preview 已使用的六職業與 follower／spell／amulet／leader，修正 neutral 與其他基本卡種的暫碼。`raw_jp_…`／其他 `raw_…` 不繼承為永久 code；已有正式採納的 code 改動須另審遷移與資料改版。特殊標記不是基本卡種，不新增本次 vocabulary_choice 的 kind。

## 3. JP／EN 原文映射

以現有 preview 詞彙檔為對照，保存內容 hash／preview 版本供驗收，私有檔不進 repo。JP／EN 在同一採納入口以 `{region,lang,raw,source_ref,special_kinds}` 保存完整 exact 原值；特殊標記的欄位、依賴與來源核對沿契約 §4.1。`registry/inputs.py::CLASSES` 只供一致性檢查，不是配發收據；`-` 沿缺值 recipe 投影 null，不配成職業、不冒作 neutral。

例如經來源核對的「フォロワー・エボルヴ」／「Follower / Evolved」在 type:follower 的 raw_mappings 帶 `[evolve]`；「イクイップメント・トークン」在 type:equipment 帶 `[token]`。這些是示意，不宣稱實際資料已核對。每個完整 spelling 保留自己的凍結 source_ref，不切字後假造 ref。不靠通用 split／trim／大小寫轉換猜新對應；未知原值拒絕／列缺項，不生成暫碼。新增 spelling 或修訂標記須新 adoption 與收據。

## 4. 15 筆繁中選詞

七職業、八基本卡種的 zh-Hant 選詞屬 #51，沿 translations 的 `vocabulary_choice`，不塞進 catalog label。首次 code 配發不能代簽翻譯；本人／委託、origin／證據、續版及撤回依[翻譯契約](translation-contract.md)。resource EP／SEP 與 type EP／SEP、trait 精霊與 class エルフ即使同譯仍分別定位，不合併 glossary。EN 原值映射不等於此次採納 EN 術語翻譯。

缺有效繁中選詞時，模板的整個 context 回原文並報 `missing_term_translation`；UI fallback 不套用卡文。標籤翻譯選用投影屬 #53，必須驗有效 catalog 主體與選詞採納，不由相同字串或裸 FK 借用其他概念的翻譯。

## 5. preview 與前端驗收

以有效採納推導的 JP／EN 映射重建 preview，保留舊版，逐項列出保留 code、暫碼替換、新增標記及未覆蓋原值，重驗 class/type/special_kind 引用閉包。其餘 preview 詞彙不能因本次職業／卡種採納而自動升為正式資料。

前端核對職業快速列、卡種篩選、特殊標記及牌組條件，包含 neutral、ep／sep、equipment／crest、進化／進階／衍生物實例；寫死暫碼或靠原文猜卡種的地方須清掉。JP／EN 同概念同 code，未知 raw 拒絕，缺譯降級分別驗收。公開欄位及格式版本不變、資料版本更新，詳見 [snapshot-format](snapshot-format.md)。
