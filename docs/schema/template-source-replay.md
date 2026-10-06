# 模板來源清冊（建置時產生）

清冊列出全部句型及其來源位置。它不存進 Git：每次建置用當前程式，從建置 recipe 指定的 JP card 封存批次產生，建置結束即丟棄。
authored 只保存定義與候選的綁定鍵（hash 與 role），不保存官方原文、正規化文字或來源位置。

## 1. 清冊項目

項目欄位為 `{id,level,source_ref,line_ordinal,role,normalizer_id,normalized_hash}`。
level 首版為 sentence（clause 盤點沿既有來源，尚不啟用子句拼接）；role=body/reminder/token_header/layout。
項目 ID 為 `inv:`＋H(`[source_ref 去掉 batch_id,line_ordinal,role,[[start,end],...]]`)，H 為 canonical JSON 的完整 SHA-256，segments 由當前分段器依原文順序重建。
ID 不含 store 名稱與封存批次 ID。source_version_id 由來源 provider、kind、URL 與原始檔 hash 決定，所以重新封存同一批頁面時 ID 不變。
項目 ID 不是 T/C 模板 ID；同一模板在不同位置有不同項目。同次建置內 ID 必須唯一。
normalizer_id 指本次支援的具名程式；normalized_hash 是分段後正規化文字 exact UTF-8 的 SHA-256。

## 2. 定義與候選的綁定

定義與候選以 `normalized_hash`＋`role` 綁定。這裡的 normalized_hash 是參數正規化後 pattern 的 UTF-8 SHA-256，也就是模板 payload 的 normalized_text 的 hash；authored 只存 hash。

- 定義匹配同一組（hash＋role）中參數 schema 驗得過的所有位置；驗不過的位置不匹配，不依原文字串合一。
- 一組裡沒有任何位置驗得過，或驗得過的位置 slot 語義角色不一致，建置失敗。
- 同一位置至多匹配一個定義；定義另驗六欄 payload 與 content_hash，見[翻譯契約 §3](translation-contract.md#3-清冊與定義)。
- 候選的 hash＋role 在本次清冊找不到位置時，建置失敗；候選仍不成為可渲染譯文。

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
