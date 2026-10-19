# 官方 SVE 測試頁面來源

`official_en/` 的 6 檔與 `official_jp/` 的 10 檔，從私有 testdata 固定 commit
`309b766885d49faff9db80f5b97cd56d8601c55b` 的 `fixtures/carddb/` 原樣搬入。
更早來源是 sve-kit commit `7c946d1a64d7cade48267bd4c6c39d3f39f6e2f0` 的同名測試檔。
它們是既有整理過的 fixture，不宣稱每份都是完整 HTTP 回應；原始抓取時間與裁切細節未可靠保存，
不能由 Git 日期推測。遷移沒有發出網路請求，也沒有搬入圖片。

[official-pages.json](official-pages.json) 保存逐檔 SHA-256、原案例 F01–F16 及既有逐欄位期望 hash；
期望值沒有由本次 parser 重新產生。英文／日文官網分別為
[英文卡表](https://en.shadowverse-evolve.com/cards/) 與
[日文卡表](https://shadowverse-evolve.com/cardlist/)。

F17–F19 的其他遊戲資料仍在永久私有 testdata，未包含於本目錄。
官方名稱、卡文、商品與圖片網址等第三方內容不適用專案授權，見 [LICENSING](../../../LICENSING.md)。
