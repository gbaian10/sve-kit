# 模板來源清冊（建置時產生）

清冊列出全部句型及其來源位置。它不存進 Git：每次建置用當前程式，從建置 recipe 指定的 JP card 封存批次產生，建置結束即丟棄。
四層目標依[共用契約](four-layer-translation.md)；本文件不表示新 normalizer 已實作。
authored 保存 frame 的來源描述 hash／版本、角色、葉 schema 與語義變體；pending 可釘精確 occurrence。
官方原文與 canonical source 只在建置中重建，不複製到 authored；候選仍未啟用。

## 1. 清冊項目

項目欄位為 `{id,level,source_ref,line_ordinal,role,normalizer_id,normalized_hash}`。
level 首版為 sentence（clause 盤點沿既有來源，尚不啟用子句拼接）；role=body/reminder/token_header/layout。
項目 ID 為 `inv:`＋H(`[source_ref 去掉 batch_id,line_ordinal,role,[[start,end],...]]`)，H 為 canonical JSON 的完整 SHA-256，segments 由當前分段器依原文順序重建。
ID 不含 store 名稱與封存批次 ID。source_version_id 由來源 provider、kind、URL 與原始檔 hash 決定，所以重新封存同一批頁面時 ID 不變。
項目 ID 不是 T/C 模板 ID；同一模板在不同位置有不同項目。同次建置內 ID 必須唯一。
normalizer_id 指本次支援的具名程式；normalized_hash 是分段後正規化文字 exact UTF-8 的 SHA-256。

## 2. 定義與候選的綁定

Frame 以 `source.canonical_hash`、`source.normalizer_version`、role 與 leaf_schema 對本次來源匹配，
再驗 semantic_variant、projection 及完整語義 payload；normalized_hash 單獨相同不足以合併。
清冊的 normalizer_id／normalized_hash 在四層分別對應上述版本與 canonical source hash。

- 每個來源 occurrence 逐一驗型別、角色、合法域、實值與單位；同一位置至多一個有效 frame，歧義保留清單。
- resolved 變體可共用；pending 變體只對精確來源 scope 有效，不跨卡自動合併。
- 定義沒有任何位置驗得過、已知來源錯配或同一槽的角色不一致時拒絕；不是按字串任選定義。
- 候選沿 normalized_hash＋role 定位本次清冊，但沒有完整 leaf_schema 就不能成為有效 frame／target。
  切換時須重算候選的 hash 對應當前 canonical；原稿文字保留，找不到來源即失敗，不維護舊 normalizer 雙軌。
- ID 與 hash 的精確 payload 依[共用契約 §3](four-layer-translation.md#3-frame-與語義身分)，
  SourceBinding 與多對多舊引用映射依該文件 §6／§9；來源換封存批次不重配相同語義 frame。

## 3. 建置

1. 讀版本固定的模板、registry、術語及辨識規則；拒絕缺檔、壞 hash、錯 owner。
2. 解析指定批次的每個完整欄位，列出所有角色／位置；未知、缺失、空字串及空白分別報告，不能漏列未匹配者。
3. 驗定義固定字、參數及全部引用，然後套用譯文；低信心／未匹配／新句型合併為一張清單。

同次建置共用已讀來源，避免重複解析；一般讀檔不觸發以上全量計算。
不新增成功收據、歷史 expected root、持久核可快取或需要使用者操作的驗證命令。

## 4. CI 與來源

官方輸入放既有私有 testdata repo，公開 repo 只釘 commit 與檔案 hash；CI 依既有唯讀取用方式驗證。
原始來源歸檔繼續保留，本處測試輸入不是歸檔替代品。新增私有輸入的發布沿既有維護者權限流程。
可信任 CI 缺輸入失敗；fork 明示只跑合成測試，不輸出官方全文 log／cache／artifact。
建置 inputs 記當次程式、輸入與實際使用資料，不替舊環境生成新的凍結證明。

## 5. 舊清冊

清冊檔（format 1–3）、producer 環境、輸出摘要與祖先資料只留在 Git 歷史，不作現行讀取或重播條件。
