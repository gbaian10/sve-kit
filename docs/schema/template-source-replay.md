# 當前模板清冊與重新產生

清冊保留全部句型及來源位置；本文件取代逐次讀取的歷史語義重播契約。
一般 reader 驗格式與引用，CI／建置使用當前程式與指定輸入重新產生清冊並比檔案，不再執行舊 producer。

## 1. 清冊 format 3

`translations/template-sources/<sequence>.yaml` 的完整欄位：
`{template_source_format:3,kind:template_source_inventory,source_batches,entries}`。
source_batches 是排序唯一的 `{store_id,batch_id}` 陣列，由呼叫端提供資料位置；不保存實體 pathname。
清冊以 translations index.inventories 的 canonical hash 索引，hash 只檢查檔案一致，不作核可證明。

entry 沿用八欄 `{id,level,source_ref,line_ordinal,role,normalizer_id,normalized_hash,legacy_fingerprint}`。
level 首版為 sentence（clause 盤點沿既有來源，尚不啟用子句拼接）；role=body/reminder/token_header/layout/name/label/flavor。
entry ID 沿既有來源位置配方：`inv:`＋H(`[source_ref,line_ordinal,role,[[start,end],...]]`)，
H 為 canonical JSON 的完整 SHA-256，segments 由當前分段器依原文順序重建。
它不是 T/C 模板 ID；同模板在不同位置有不同 entry。全入口 entry ID 唯一，命中仍比完整位置內容。
normalizer_id 指本次支援的具名程式；normalized_hash 是 normalized_text exact UTF-8 的 SHA-256，模板 content_hash 另驗六欄 payload，不記舊 producer 程式／環境。
flavor 為全段、level=sentence、line_ordinal=0、normalizer_id=flavor-exact-v1、legacy_fingerprint=null。
來源片段的精確 segments、anchor、參數位置由當前解析結果與定義保留及驗證，不能只驗總筆數。
舊 ID 撞不同內容或同位置出現兩個有效定義都需報錯／列歧義，不默默覆蓋。

## 2. 重新產生

1. 讀版本固定的當前來源輸入、registry、術語及辨識規則；拒絕缺檔、壞 hash、錯 owner。
2. 解析每個完整欄位，列出所有角色／位置；未知、缺失、空字串及空白分別報告，不能漏列未匹配者。
3. 用同一組輸入重產清冊、按穩定鍵排序，比對 Git 中的檔案；差異須隨正常資料／程式修改一起更新。
4. 驗定義固定字、參數及全部引用，然後套用譯文；低信心／未匹配／新句型合併為一張清單。

同次建置共用已讀來源，避免重複解析；一般讀檔不觸發以上全量計算。
重產比檔案只能驗決定性與更新一致，不能證明翻譯語意正確，因此仍保留必要的獨立功能反例。
不新增成功收據、歷史 expected root、持久核可快取或需要使用者操作的驗證命令。

## 3. CI 與來源

官方輸入放既有私有 testdata repo，公開 repo 只釘 commit 與檔案 hash；CI 依既有唯讀取用方式驗證。
原始來源歸檔繼續保留，本處測試輸入不是歸檔替代品。新增私有輸入的發布沿既有維護者權限流程。
可信任 CI 缺輸入失敗；fork 明示只跑合成測試，不輸出官方全文 log／cache／artifact。
建置 inputs 記當次程式、輸入與實際使用資料，不替舊環境生成新的凍結證明。

## 4. 舊清冊的保存

當前入口只接受 format 3，來源覆蓋、定義與可匹配位置依 §2 重產核對，不能僅修改版號冒充有效清冊。
舊 format 1/2 的 producer 環境、輸出摘要與祖先資料留在 Git 歷史，不作現行讀取或重播條件；缺必要來源或無法保留的內容仍須明列原因。
