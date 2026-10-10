# 建置中的四層來源位置

正常建置從指定的 JP 封存批次解析精確 owner 欄位，由四層來源核心產生 body、reminder、token_header 與 layout 分段、Unicode trace 及葉位置。
來源原文與 canonical source 只在建置 DB 中重建，不複製到 authored。現行流程不產生或讀取舊 `inv:` 清冊。
Frame、SourceBinding 與 pending 的完整形狀依[四層共用契約](four-layer-translation.md)。

## 1. 精確來源與位置

SourceDescriptor 保存 owner、field、ordinal、source_unit_id、source_hash 與凍結來源定位。
來源 provider 必須解析到該 owner 的同一欄位；另一張卡的相同文字不能授權這個來源。
沒有內容的效果欄位不產生 source span，並計入 empty_source_field。

SourceSpan 保存行號、角色與原文 code point 的半開區間。來源核心產生的 Unicode trace 同時供有限詞彙辨識與 frame 匹配使用，沒有另一個正規化配方。
LeafOccurrence 分別保存原文及 canonical 位置、來源單位與 explicit 狀態；重複出現的葉保留各自 ordinal。
N0 不推導省略來源；必要引用未解時，完整欄位維持原文 fallback。

## 2. Frame 與來源用途

Frame 依 canonical hash、normalizer 版本、角色、leaf schema、語義變體及 projection 匹配。
canonical hash 相同不足以合併不同語義。resolved 變體可以共用；pending 只對精確 occurrence scope 有效。
同一位置至多選用一個有效 frame；人工 match pin 也必須通過同一個來源邊界。

建置先完成來源分類與匹配，再將 authored frame、target 與 form 寫入 DB。
沒有任何完整 owner 欄位支持的 authored frame 列入建置報告（frame ID 與 no_fully_verified_source_owner_field），不啟用該 frame／target；受影響欄位整欄退回原文，建置可完成。候選只保存未啟用草稿，不產生 active binding 或 target。
來源 bindings、輸出葉關聯與原文／譯文 annotation 分別保存，不能按中文字串搜尋位置。

## 3. 輸入與測試

正式入口只讀當期 format 3 authored 與建置指定的凍結來源，不讀舊清冊、歷史 producer 環境或遷移收據。
同次建置共用已讀來源，避免重複解析；一般讀檔不觸發全量分類。
不新增成功收據、歷史 expected root、持久核可快取或需要使用者操作的驗證命令。

官方測試輸入沿既有私有 testdata repo 的鎖定版本與檔案 hash 取得，原始來源歸檔繼續保留。
可信任 CI 缺輸入即失敗；fork 明示排除依賴私有輸入的案例，不輸出官方全文 log／cache／artifact。
建置 inputs 記錄當次程式、輸入與實際使用資料，不替舊環境生成新的凍結證明。
