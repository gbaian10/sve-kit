# 已審查的勘誤公告輸入

`card-data/errata/` 保存 JP／EN 公告的解析結果；公告原件與來源歷史仍保存在來源歸檔。
`index.json` 釘住 `jp.json`、`en.json` 的 SHA-256 與位元組數，兩區文件採
[解析公告 Schema](parsed-errata.schema.json)。不得存入歸檔 store 名稱、本機路徑、卡圖或語音。

每筆公告包含來源版本、URL、raw hash、抓取時間、parser、batch／descriptor／receipt hash；
建置時由呼叫者提供 store 綁定。`announced_on` 取公告標題日期；`effective_on` 只接受明示的生效日，
未知為 null，不拿公告日代填。`reason` 保存理由，`exchange_offered` 依公告明文記錄。

`blocks` 保存完整誤／正配對與各自的原始 UTF-8 位元組範圍、hash、前文與順序。
圖示沿用日版 `{alt}`、英版 `{filename_stem|alt}` 表記。配對後面若還有說明文字，必須先核對邊界，
不能把修正理由、換卡說明或 Q&A 當成修正後卡文。

`blocks` 是解析結果（`parser_version` 記錄產生版本）；`changes` 是人工採納的已審查輸入，
將已核對的配對或其具名子段落連到同區 card_no／face_id，不宣稱解析器自動完成此對應。
卡名整欄值不含引用符號『』；`blocks` 仍保留公告原樣。
`scope=fragment` 表示公告只提供變更片段；`scope=field` 表示該具名欄位本身的完整內容。
這兩者都不等於完整的實體印刷卡文，也不建立 before／after face_revision。
同卡不同版次共用一個卡面時不重複輸出同一變更，但保留全部 `printings`。

公告列出的版次只記 `listed`，不推定 `confirmed_applies`。同一公告內其他卡的變更，
如果沒有明示版次，配對仍留在 `blocks`，不借用公告主卡的身分；已有獨立公告的 Token 名稱由各自公告輸入。
其他附屬文字可用 `field=other` 保留其明示標題；採用「標題: 內容」的前綴寫法，
例如 Rulenye 的 `Token text: …`，標題不是卡片效果的一部分，也不是補出的完整卡文。僅更正插畫的公告保留理由與列出版次，`changes=[]`。

`sve-carddb parsed-cards validate <絕對輸入根> --previous <前版輸入根>` 同時驗證這份補充輸入：
檔案閉包、hash、Schema、區域、逐欄位公開值域、唯一來源版本，以及前版公告完整保留。
新增觀測須使用新來源版本，不能藉重算檔案 hash 覆寫既有來源觀測。
`source`、`blocks`、公告資訊與明列版次維持不可變；人工採納的 `changes`／`review_note`
可透過獨立審查的資料修正 commit 更正，不冒充新的官方來源觀測。
寫入端須用下列獨立核對指令驗原件與 UTF-8 byte ranges；路徑及 store ID 由操作者明示：

```bash
sve-carddb parsed-cards verify-errata /absolute/card-data /absolute/archive store-id
```

此指令驗 index／資料准入、來源批次與 descriptor／receipt、整個原件 hash、每個誤／正範圍的
邊界及切片 hash，僅讀封存原件，不讀 latest cache 或網路。文字配對完整性、
`changes` 的段落／卡面／欄位選擇仍須人工審查；指令不推定這些語義。
普通建置不呼叫核對指令，只把兩區輸入檔的實際 bytes hash 記入 configuration。

普通離線建置透過 `reviewed_errata.read` 直接還原既有 `ErrataPage` 輸入，不重做以上准入驗證，
不讀 HTML、latest cache 或 live manifest。資料庫既有 FK／CHECK 繼續保護寫入的引用關係。
公告匯入不清除 current 待確認限制，也不宣稱完整勘誤覆蓋，更不啟用印刷卡文反推。
