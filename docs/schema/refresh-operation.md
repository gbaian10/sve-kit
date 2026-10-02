# 受保護抓取的操作條件

本頁說明 `crawl` 的來源保護設定；歷史、收據與缺口政策仍以[來源歸檔與凍結輸入](source-archive.md)為準。
程式與本文件合併本身不授權對真實資料執行 refresh；完成下列前置條件並經維護者明示同意後才可啟用。

## 啟用前置條件

1. 替換協議與本操作文件須已審核、合併，並依下節一起配置四個環境變數；備份根須在獨立裝置上。
2. 確認本次會碰到的來源範圍完整，還原工作根能容納該範圍的完整閉包與預留空間；碰到卡圖時須能還原整個地區的卡圖閉包，空間計算見下文。
3. 先完成小卡包的受控演練，核對舊版保存、獨立備份／還原與容量報告，再擴大範圍；涉及真實資料的演練本身也須先經維護者明示同意。

## 設定

以下設定須一起提供；缺任一項時，非 dry-run 的 refresh 或部分配置的 crawl 都會在開啟 manifest、連網前停止。
未配置時，普通 crawl 沿用會阻止覆寫不同 raw 版本的 Writer。

| 環境變數 | 用途與限制 |
| --- | --- |
| `SVE_ARCHIVE_ROOT` | 不可變來源庫的絕對路徑，須在 latest 資料根之外，不得經過 symlink |
| `SVE_ARCHIVE_STORE_ID` | 穩定的來源庫識別名稱，不含 `/` 或 `..`；須與既有庫一致 |
| `SVE_ARCHIVE_BACKUP_ROOT` | 獨立裝置上的備份根絕對路徑，不能與 latest 或來源庫互相包含 |
| `SVE_ARCHIVE_RESTORE_ROOT` | 還原演練工作根的絕對路徑，不能與 latest、來源庫或備份根互相包含，也不得經過 symlink；不預設使用系統暫存 |

目的地與裝置由維護者配置；正式文件不記私人儲存路徑。配置後的 resume、repair 與 refresh 都使用受保護 Writer。
`--dry-run` 不產生來源版本或備份。

`crawl errata-new --urls <已核對的 JSON 清單>` 是獨立的只新增入口，沒有 resume／repair／refresh 模式。
它不啟動受保護替換 Writer，不處理其他任務的恢復或收尾；已有可信來源只跳過，不改 metadata，
既有不可信來源、目的檔案或未完成任務則停止。即使四項設定完整，也不自動封存或備份；
部分配置仍在開啟 manifest、連網前拒絕非 dry-run 執行。真實抓取須另獲維護者授權，
操作方須在抓取前後備份 manifest，抓取後明確 seal 對應 scope、獨立 backup 與本批 restore-check。
URL、轉址、重試與中斷限制見 [README](../../README.md#adding-reviewed-jp-errata-sources)。

### 勘誤新增入口的單一 URL 試跑

`main`／`article`／`.entry-content` 是暫定的保守內容容器條件；目前沒有已保存的官方 JP
errata／news 頁面可據以確認它們適用於真實勘誤正文。程式合併不取代這項驗證，也不授權試抓。
維護者另行通知可以抓取後，由指定操作者依序執行：

1. 對已核可的 16 URL 清單做 `crawl errata-new --urls <清單> --dry-run`，確認目的地與資料根沒有 symlink、沒有未完成任務。先取得新的受鎖 manifest 備份。
2. 從這 16 URL 中選一個，另建只含它的清單，再執行 `crawl errata-new --urls <單一 URL 清單>`。不先對其餘 URL 發出任何請求。
3. 命令成功後做 after 備份，從關閉副本與已保存 raw 唯讀核對 URL、hash、大小、HTTP metadata 與非空 title／正文容器。只回報 URL、hash、數量與結構旗標，不複製正文、不解析成正式勘誤。
4. 先 seal 試跑來源的 `jp:errata` scope、獨立 backup 並通過本批 restore-check，再對其餘 15 URL 的獨立清單執行相同備份→抓取→seal→backup→restore-check 流程。成功試跑的 URL 不再重抓；後一批 scope 仍須包含已知試跑版本的閉包。

試跑出現轉址、驗證失敗、中斷或任何不確定即停止，不執行其餘 15 URL，也不先放寬驗證。
驗證失敗的 body 不會被本入口保存，不能宣稱已對照其結構；要取得不同的診斷證據或修改選擇器，須另行提出、審核工具及授權範圍。
單一頁面通過亦不證明其餘頁面結構一致，其餘 URL 仍逐則接受相同驗證。

### 勘誤新增入口中斷後的處置

由**維護者或其明確指定的操作者**負責，協調者安排獨立恢復方案與程式審核。
目前沒有只處理指定 errata request／path 的已核准恢復 CLI。入口被擋住時，不能改用一般
`crawl`（包括 resume／repair）或 `refresh`；它們的全域恢復可能修改其他任務的 STARTED
紀錄或刪除 tmp。也不能刪鎖檔、直接改 SQL、把中斷前備份覆蓋回 live、以 tmp 猜測成功來源，或先移除來源再重抓。

1. **停下並定位所有者**：停止本次入口，確認沒有其他 writer 持鎖；只核對自己的行程，不終止他人任務。記錄已完成 URL、最後結果與未嘗試範圍，通知維護者。若 blockage 屬於其他任務，交回該任務所有者，不把它納入本次恢復。
2. **保存新的中斷後副本與證據**：用既有 `sve-carddb manifest backup <新的中斷後備份.sqlite>` 取得受共同鎖及 SQLite backup API 保護的副本，保留中斷前備份；驗 integrity／hash。該指令只備份，不完成 request 或恢復 raw。若拿不到鎖或備份失敗就停。對可疑 tmp、未登錄 raw 與 archive marker 記錄精確 path、hash、大小並保留原檔；不得清理或改寫。
3. **只讀檢查關閉副本**：檢查 fetch_log 的 request ID／run ID／URL／outcome 與對應 Resource；比較原備份、實際 raw／tmp 及已封存證據。區分「已提交且可信」、「STARTED 且未發布」、「完整但未登錄 raw」與其他任務。只憑檔名或 mtime 不能證明請求成功；缺少 status、ETag 等觀測也不能憑 raw 補造 metadata。
4. **另案開發及核准精確恢復工具**：目前在這一步等待，不存在可直接執行的恢復命令。工具須走獨立本機審核／PR，以指定備份 hash、request IDs、URL、path／bytes hash 作前置條件，持共同鎖重新核對；先在合成資料與副本演練。計畫須明定如何結束仍 STARTED 的指定請求（按已核對證據標記 unknown／失敗，不能猜成成功），以及在獨立保存及驗回後如何隔離本次未登錄 raw／tmp。保留原始觀測，不刪改已完成、已失敗紀錄，不觸碰其他任務。無法證明範圍或完整性就繼續停止，不能改用全域 mark_interrupted／清理。
5. **恢復後再次驗證再續行**：工具審核合併且維護者明示授權後，指定操作者才執行它；再取得新的 after-recovery 備份，使用 `sve-carddb manifest check`（需外部唯讀根時明示 `--allow-root`）驗現存來源，對核可清單做 errata-new dry-run，確認已可信來源跳過、僅剩新 URL、沒有未完成狀態。保存恢復報告及證據，取得續行通知後才回到上述單一 URL／剩餘 URL 流程。任何異常即停。

例如將第 2 步已驗證、關閉的備份命名為 `interruption.sqlite`，可用下列唯讀查詢定位未完成請求；
它不讀 live、不改 outcome，也不是恢復命令：

```bash
sqlite3 -readonly 'file:interruption.sqlite?immutable=1' \
  "SELECT id, run_id, url, requested_url, started_at, outcome FROM fetch_log WHERE outcome = 'started' ORDER BY id;"
```

## 收尾與容量

替換前驗證舊 raw、descriptor、正式收據與獨立備份。已封存舊版重用既有閉包，304 不新增更新前 DB。
同次 Writer session 的候選，以及尚未首次封存舊版的 304，共用必要的更新前一致快照；304 舊版收據以更新前的 Resource 驗證。
每筆已提交寫入立即備份 raw、原始儲存表示及小型提交紀錄。
候選與提交紀錄不等於正式 observation receipt，也不授權清理 latest。

提交後的正式收據在批次收尾共用一致 manifest 副本。同一 URL 在收尾前再次換版時，須先保存該中間版的正式收據與必要快照，才能替換。
同一 URL 換版後又收到 304，也會多留一份快照；這是非正常流程，正常抓取一次執行只處理該 URL 一次。
因此 DB 成長取決於 session 與必要的中間版本檢查點，並非每個 URL 一份。raw 與原始壓縮表示分別以內容 hash 去重；壓縮表示也要列入空間預算。

收尾只封存本次實際寫入、304 或中斷恢復涉及的 provider／kind；依 sealed inventory 契約，該範圍內所有當前資源與已知版本都須驗證。
同一範圍內缺檔仍會阻止封存；其他範圍不會被順便首次封存。
中斷後尚未收尾的提交紀錄會在下次執行補做收尾。

新批次的 `manifests/` 與 `batches/…/manifest.sqlite` 在來源庫、備份與還原根內各自共用 hardlink；跨根仍複製獨立 bytes 並驗 hash。
容量報告的 `backup_closure_bytes` 仍按各閉包路徑的邏輯大小加總，因此下述還原空間預留會保守高估 hardlink 節省的部分。

每批備份後都在配置的還原工作根建立空目錄驗回，完成後移除演練目錄。
開始複製前須至少有整個備份閉包大小，加上 10% 或 1 MiB（取較大值）的可用空間。
空間不足、備份中斷或還原驗證失敗即停止，不記完成；重新執行會重試尚未完成的收尾。

## 遺失的舊內容

repair 重抓的 bytes 若與舊 Resource 的 raw hash 及大小完全一致，可補回該版本；ETag 相同不能替代 hash 驗證。
重抓內容不同時仍先停止，列出 URL、預期 hash 與缺失原因。

舊版在首次封存前已永久遺失，且獨立備份也找不到時，維護者須先核對預期 hash，再明示記錄缺口：

```bash
sve-carddb archive acknowledge-gap 'https://example.invalid/cards/lost' \
  --expected-hash 'sha256:<完整 64 碼舊 raw hash>' \
  --reason '已核對獨立備份，首次封存前的舊 bytes 已不可取得'
```

命令使用上述完整配置，不連網。只有 manifest 留有持久來源引用、當前 raw 不可用、且該版本未封存也沒有保留 blob 時才接受。
可用或已封存版本不能用此命令略過；hash 不符、空白理由與缺少持久引用也會被拒絕。
缺口的原 Resource、理由及一致 manifest 副本先存入來源庫與備份，再允許後續抓取從可得的新版本建立歷史。
既有 fetch／generation／連結引用保持原樣，sealed inventory 仍列 `missing_history`；有缺口不能宣稱完整歷史驗收通過。

恢復以已提交 manifest 為準；磁碟上的內容若既不屬於舊版也不屬於候選新版，就停止並保留該檔案。
新協議保存原始儲存表示，恢復不要求新版壓縮庫重現舊輸出；舊意圖未保存該表示時，重壓縮長度不符仍會停止。
