# 標誌卡與編號卡

`authored/catalog/printing-marks/<卡包>/<數字>.yaml` 保存已確認的版次標誌與編號卡屬性。
建置只讀這些分片，不讀卡圖、不執行 OpenCV 或 OCR。分片小於 1 MiB；封套與其他 authored
分片相同，固定 `format: 1`、`kind: printing_marks_shard`、`records`。

```yaml
format: 1
kind: printing_marks_shard
records:
  - printing_id: p:0123456789abcdef0123456789abcdef
    region: jp
    card_no: SYN-001
    stamp:
      kind: anniversary
      year: null
    serial: false
```

每筆以永久 `printing_id` 定位；`region`、`card_no` 必須與登錄身分一致，不由卡號推定標誌或
編號屬性。同一版次只能出現一次。`stamp: null` 是未記錄標誌；非空物件包含 `kind` 與 `year`，
可各為 null，表示看得到標誌但種類或年份尚未辨明。種類保留 logo 上的英文或日文原名；
週年標誌統一用 `anniversary`。顏色、名次及排版不自動推導成不同賽事，無法確定的字樣不猜。
年份是 logo 上的顯示文字，允許 `24/25` 等賽季；不由發行日期、卡面版權年份補值。

`serial` 只表示插圖印有序號牌，包含官方圖的空白分子樣板；不保存個人持有的第幾號。
它與既有 `serial_total`（卡面分母的人工補充）分開，不能由其中一個欄位推導另一個。
沒有 reviewed 記錄的版次，其建置／快照 `serial` 為 null；已核對無序號牌為 false。
標誌與編號屬性可以並存；簽名與 BCS 賽季數字都不能直接當成序號牌。

## 新增 PR 卡後的檢查

先完成解析輸入與永久版次／同圖登錄，備妥本機 `SVE_DATA_DIR` 的來源圖片。
使用可選的 `art-matching` extra 執行作者工具：

```bash
uv --directory carddb run --extra art-matching sve-carddb images detect-stamps \
  --scope pr --authored /absolute/checkout/authored \
  --parsed-cards /absolute/checkout/card-data \
  --data-dir /absolute/source-cache --output /absolute/private/stamp-review \
  --unreviewed-only
```

移除 `--unreviewed-only` 可重掃全部。範圍包含 home family 為 PR 的版次，以及 BSF／NY 等
圖片實際位於 PR 來源目錄的活動版次。工具由解析資料的實際 `<img src>` 取得圖片，
不由卡號組檔名，不開啟抓取 manifest。一般版優先，沒有一般版時使用登錄同圖的其他 PR；
SIFT／RANSAC 對齊後比較插圖區差異，保留左側、高而寬的週年標誌與低對比細字。
沒有可用對齊參照時產出金色區域候選。差異與無差異都不能單獨證明有無標誌；同圖 PR
可能共用相同標誌或序號牌，彩色／銀色且無參照的標誌也可能漏掉。

看 `candidates.json` 的切圖、暫定形狀群與參照，核對種類與顯示年份。群號只供本次核對，
不當永久種類。文字辨識不是必要依賴；種類名稱由人確認。
需要補查已知序號牌樣式時，將已看過的本機範例寫成私有 JSON，再傳 `--examples`：

```json
[
  {
    "printing_id": "p:0123456789abcdef0123456789abcdef",
    "label": "serial",
    "box": [210, 330, 210, 25]
  }
]
```

座標是正規化到 459×641 的 `[x,y,width,height]`，選金色牌匾的特徵裝飾，包含周圍空隙，
不是整塊單色。工具執行時才從本機來源擷取圖樣，以金色與銀色遮罩、多尺度比對整個插圖區，
因此可補查插圖中段、淡金或銀色的同樣式牌匾。分數是相似度，不是正確率。
不把官方切圖當模板提交，也不靠固定卡號白名單判定新卡。

清楚可辨且圖樣與既有確認卡一致的高信心候選可准入，並在 PR 本文列出；不同圖樣或
看不清楚者先交維護者確認。確認後人工更新 authored 分片，再建置快照。命令不自動改 authored。
沒有候選不代表查無標誌，仍須看來源圖；掃描輸出與切圖留在 repo 外，不進 git。

## 建置與匯出

建置 `printing.stamp` 是符合 `StampMark` 的 JSON 物件或 null，`printing.serial` 是 Bool?。
兩者直接投影到快照 printing 的 bootstrap，讓有無篩選不必下載詳情分片；種類與年份同時可供
單卡頁顯示。快照 Schema、格式指紋與共用合成樣本必須一起更新；新增欄位也須跑 publish 測試。
