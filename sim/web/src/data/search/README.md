# Worker 查卡介面

`SearchSession` 擁有一個 Worker 與完整的索引世代。以 `createChannel()` 建立獨立的
`SearchChannel`，建議清單與結果列表可共用同一份索引，各自訂閱 `getStatus`／`subscribe`。
通道保存最後一筆輸入；下載完成前只有 pending，完整世代就緒後自動查詢。
Worker 在 task 邊界合併每個通道尚未執行的查詢，只掃描最新 queryId；已開始的同步掃描不會中途搶占。
關閉通道會丟棄該通道排隊中的查詢，其他通道繼續使用索引。

世代的 status 帶 `root`、`edition`、`generation`、`manifestHash` 與 `dataVersion`。
更新期間這些欄位仍指向目前可查詢的世代，`loading` 表示正在下載的根與地區。
結果也帶 root 與 generation，晚到的 queryId／generation 不會覆寫新結果。
載入失敗保留舊索引；切換地區的查詢必須等該地區完整就緒。
量測由建構子的 `onMetrics` callback 另取，不屬於公開 status。

查詢錯誤與載入錯誤分開：通道結果的 `error.kind` 為 `unsupported-query`、
`edition-not-ready`、`query-failed` 或 `load-unavailable`；不支援的條件列在 `facets`。
世代的 `error.kind` 為 `load-failed`、`worker-failed` 或 `cancelled`。
目前 Worker 支援文字、職業、版次所屬卡包、費用、卡片單位與卡號排序。
**Worker 路徑對不支援的條件回 error；同步 Catalog 路徑會略過。**
同步 Catalog 目前還會略過費用與卡包篩選，因此 V1 暫時 hook 不能宣稱兩條路徑已有相同語意。
此差異不修改 `domain/search`。

查詢比對直接讀 Uint32 欄位與共用字串池，不把所有 entries 還原為物件。
建索引逐卡處理，只保留所選地區的版次與摘要；跨地區名稱仍可比對。
只有當頁命中會還原 `CardSummary`，不載入完整卡片詳細資料。
現有 N0 輸入仍混放兩地區，完整讀取與驗證後才投影；分包 plan 可由後續格式工作替換。

CacheStorage 依 root 分 namespace，以內容定址跨版重用。完整世代發布後，保留 JP、EN
各自目前與上一份不同的成功 plan 所需 blob，淘汰其餘 key；重複載入同一份 plan 不擠掉上一份。
plan 記錄存在相同 cache，因此 Worker 重啟後仍可淘汰。快取無法使用或 quota 耗盡時可直接下載。
不同頁籤同時寫同一 root 的 cache 沒有跨頁籤協調，可能造成重新下載；不影響內容驗證。

Worker 發出 error／messageerror 時會清掉不可用世代，下次 load 或 retry 建立新 Worker，從 cache 重建。
瀏覽器因 OOM 殺掉 Worker 時**不保證觸發 onerror**，可能持續停在 downloading／pending；
目前沒有 watchdog，手機驗收需觀察此限制。整個 Worker 終止後不能立即保留舊索引的可查詢性。
