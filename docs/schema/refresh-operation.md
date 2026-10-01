# 受保護抓取的操作條件

本頁說明 `crawl` 的來源保護設定；歷史、收據與缺口政策仍以[來源歸檔與凍結輸入](source-archive.md)為準。
只有替換協議完成本機審核、合併且備份／還原驗收通過後，才在正式來源上啟用 refresh。

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

## 收尾與容量

替換前驗證舊 raw、descriptor、正式收據與獨立備份。已封存舊版重用既有閉包，304 不新增更新前 DB。
同次 Writer session 的候選，以及尚未首次封存舊版的 304，共用必要的更新前一致快照；304 舊版收據以更新前的 Resource 驗證。
每筆已提交寫入立即備份 raw、原始儲存表示及小型提交紀錄。
候選與提交紀錄不等於正式 observation receipt，也不授權清理 latest。

提交後的正式收據在批次收尾共用一致 manifest 副本。同一 URL 在收尾前再次換版時，須先保存該中間版的正式收據與必要快照，才能替換。
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
