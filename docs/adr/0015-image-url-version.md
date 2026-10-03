# ADR-0015：卡圖採永久 ID 路徑與查詢版本

狀態：已採用（2026-10-04）。快照格式：2.0.0；程式實作另行同步。

## 背景

查卡頁原先須先取卡包的 printing_image，再查 global image_variant 分片，才能取得內容定址圖片 URL。
卡圖大多不變，偶爾更正裁切或補圖；維護者要求使用者接受新版快照後看到新圖，且不保存公開舊圖。
尚無正式發布的卡圖或快照，首次選定方案即可上傳，不存在保留舊 namespace 的遷移成本。
玩家卡片頁路由與背景圖片 URL 是不同契約，本決定只改背景資源定位。

## 決定

使用 `images/<size>/<int_id>[-f<ordinal>].webp?v=<version>`。
int_id 為 printing 的永久整數配號；ordinal 是永久 face.ordinal，0 省略後綴，其他面加 -fN。
不由卡號、來源 image_id 或陣列位置推網址；官網來源仍取原頁 img src。

版本由 printing.home_set 的卡包 media 分片提供，card 三檔及 art 兩檔分別使用 card_version／art_version。
media 同時提供狀態與實際尺寸，取得一層局部 metadata 即可組圖，不為首圖下載 global image 詳情。
版本是最後變更的單調發布 revision，不是每次資料更新的時間戳；crop 只換 art，純文字變更沿用圖片版本。
撤下後恢復與 A→B→A 都用新 revision，避免古老 query 已快取另一批 bytes；失敗保留的號也不重用。

先以條件寫入覆寫固定 key，驗 origin 與新 v CDN bytes，再寫不可變 metadata／manifest，最後 CAS 切版本索引。
舊快照帶舊 v 冷讀取得新圖可接受；同 key 不保存歷史 bytes。多尺寸覆寫不原子，必須全組完成才發布新快照。
browser src／srcset、CDN 及 SW cache key 都須區分 v；接受新版後禁止舊圖 fallback，失敗明示。
CDN purge 不會清掉 browser／SW，因此不能代替版本切換；JSON 的內容定址與 hash 驗證仍保留。
精確欄位、版本、發布與失敗恢復見 [snapshot-format](../schema/snapshot-format.md#21-獨立影像清單與-dsl-附件)、
[image-variants](../schema/image-variants.md#20-圖片-url版本與新鮮度) 及 [snapshot-transport §5.4](../schema/snapshot-transport.md#54-format-200-卡包-media-與-id-圖片)。

2026-10-04 的 R2 自訂網域合成圖實測已通過 query 分離：普通 GET 暖 v1 後覆寫同 key，未 purge、未 bypass，
v1 HIT 舊內容，v2 MISS 新內容後 HIT 新內容。這只證明所測 CDN 設定；browser／SW 與發布整合須另驗。
正式部署核對同等設定，Cache Rules／Worker／rewrite 或網域改變時重做回歸。

## 考慮過的選項

| 選項 | 取捨 |
| --- | --- |
| 內容 hash 路徑＋全域對照 | 既有 writer 可延用、可去重，但顯圖需兩層 metadata；永久保留並非內容定址的必要條件 |
| card_version／art_version＋狀態放 printing／face 啟動分片 | 啟動包到齊後零額外 metadata 查詢；整數 revision 的首發 Brotli 增量很小。此版未採用，保留圖片變更與文字啟動包的更新邊界，並沿用完整實際尺寸投影；代價是仍多取一層 media，詳下述量測與取捨 |
| 卡包內放每尺寸 hash（C2） | 一層查詢、快取隔離容易；須傳五個 hash 並回收失去 current 引用的圖片 |
| ID＋版本檔名或不可變路徑段 | 不依賴 query，create-only 較容易；須上傳新 key、切快照後刪舊 key，previous client 需處理舊路徑失效 |
| ID＋8 碼短雜湊檔名 | 同 ID／面／尺寸歷次輸出仍有截斷碰撞風險；需完整 hash 對照或擴長，不只檢兩版 |
| 固定 key 純覆寫／只 purge | 長快取可能卡在舊圖，不能滿足接受新版後顯示新圖的要求 |
| ID＋快照提供的 query 版本 | 採用；免逐次換名清理，舊 v 冷讀仍能取得目前圖，代價是受控覆寫、版本配號及快取驗收 |

2026-10-04 設計審核以 76 個含 printing 的 bootstrap 檔、每面附加兩欄重新壓縮：
發布 revision 整數（首發多為同值）＋狀態增加 **887 bytes Brotli**；32-bit 版本記號對照增加 **94,386 bytes**。
這支持把版本／狀態放進啟動包的可行性，不是完整 2.0 欄序、雙組版本、尺寸例外與手機 heap 的驗收，
也不以這些數字宣稱啟動預算不足而拒絕它。

協調者 2026-10-04 決定本版維持 media：若版本前移，每次圖片版本更新都會改動所屬 printing 啟動分片的 bytes／hash，
使所有需要該片的使用者都必須重抓，而不只是當下要看該圖者；並連帶重建以精確 base FileRef 相依的 printing 文字詳情。
media 把這些圖片變動隔離在按需資料，保留文字啟動快取，是此次未採零查詢方案的主因。
完整實際尺寸也與狀態／版本同片，不另定稀疏例外格式。
零查詢方案可用 config 名目尺寸加稀疏例外，但須明定橫向、小圖與取整差異，不能把尺寸上限當實際 width／height。
這是更新範圍及格式複雜度的取捨，不表示零查詢方案技術上不可採用；media 多一層網路相依是明示代價。
「卡包分片欄位」也涵蓋 printing 所屬 home_set 的啟動分片，不以字面解讀排除該選項。
日後若手機實測顯示這一層查詢是瓶頸，可由協調者／維護者改採前移，屆時須同步 snapshot-format §2.1／§3.1、snapshot-transport §5.4 與機器契約，
讓 media 退為來源／尺寸例外等按需詳情；本版未變更這三處的顯圖資料位置。

本次量測資料的 ID 檔約 2.267 GB，內容去重約 2.164 GB，差約 103 MB；這是首次儲存差額，非兩份全庫遷移。
不以此保證未來比例或手機 heap；新 media 的容量及實際裝置效能須重新量測。

## 後果

採 2.0.0 major，因 printing_image 與 image_variant 欄序／定位責任改變；現有 1.x 機器資源不回寫。
producer、reader、發布器與共用合成樣本須同步，文件採用不表示程式已支援。
image_id 保留作來源與詳情連結，int_id 的穩定性及雙面對應須驗證；ID key 犧牲跨版次圖片去重。
若未來部署無法可靠維持 query 分離，替代為不重用版本檔名，仍須同步新格式契約與 reader。
公開圖片只留 current，快照窗口與來源歸檔邊界依 [ADR-0016](0016-snapshot-retention.md)。
