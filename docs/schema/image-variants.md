# 卡圖衍生檔契約

來源圖與衍生檔分開：原始 PNG 留在資料來源與備份，不公開至 CDN、不作 image_variant 的必備 original 檔。公開的整張卡圖亦重新編碼為 WebP。來源網址仍保留於 image_asset，2.0 公開衍生檔由永久版次 int_id、面、尺寸與版本組 URL，不能由卡號推算官方來源。1.x 的內容定址發布契約保留原解讀，建置快取亦仍可內容定址。

## 檔位與尺寸

| size_key | purpose | max_width | max_height | 直向來源的輸出 |
| --- | --- | --- | --- | --- |
| card_s | card | 128 | 179 | 整卡寬 128，等比例 |
| card_m | card | 320 | 447 | 整卡寬 320，等比例 |
| card_l | card | 459 | 641 | 整卡寬 459，等比例 |
| art_s | art | 160 | 120 | 插畫裁切 4:3 |
| art_m | art | 384 | 288 | 插畫裁切 4:3 |

上述為五個固定 WebP 檔位，尺寸是上限而非保證每張原圖有足夠解析度。先 decode 並套用來源方向資訊，再依實際 W/H 判斷：W≤H 視為直向；W>H 為橫向；兩種方向均輸出 card 三檔與 art 兩檔。雙面各自處理。2.0 前端讀 printing_image.variants 的實際 width／height（1.x 讀 image_variant.width／height），以 contain 顯示整張卡，不能固定假設每張都是 459×641。

直向 card 縮放倍率為 `min(1,max_width/W,max_height/H)`；橫向 card 三檔分別限制最長邊 179／447／641，倍率為 `min(1,max_height/max(W,H))`，不把直向 max_width 再套到橫向。縮放後寬高用 `floor(x+0.5)`，最小 1；不放大、不拉伸、不裁整卡。例：459×641 產 128×179、320×447、459×641；641×459 產 179×128、447×320、641×459。異常長寬比依高度上限可使直向寬小於名義寬，metadata 永遠記實際尺寸。

## 插畫裁切與覆寫

直向預設裁切採左 8%、右 92%、上 14%，高度由裁切寬乘 3/4 決定。像素框用純整數算式及半開區間：`left=(8*W)//100`、`top=(14*H)//100`、`k=min((84*W)//400,(W-left)//4,(H-top)//3)`，框為 `[left,top,left+4*k,top+3*k)`。`//` 表示非負整數除法向下取整，中間乘積須使用不溢位的整數計算。k 必須正值；此取整維持精確 4:3，最多損失不足 4 個寬像素。459×641 的框為 `[36,89,420,377)`（384×288）。

橫向預設裁切採左 17%、上 4%、寬 65%，亦使用精確 4:3 的整數框：`left=(17*W)//100`、`top=(4*H)//100`、`k=min((65*W)//400,(W-left)//4,(H-top)//3)`，框為 `[left,top,left+4*k,top+3*k)`。641×459 的框為 `[108,18,524,330)`（416×312）。

**使用者 2026-10-01 核可**：確認頁 43 張樣本（含 SP／SSP 與 5 張橫向 SEP）採上述直向／橫向預設框，這批樣本沒有需要手動框的卡。

**使用者 2026-10-03 核可**：倒吊版型的 12 張來源圖（日英各 6 張）下移插畫框，保持倒吊、不旋轉；畫內標誌／簽名的 13 張維持標準框，不另處理。目前僅這 12 張採例外，精確框與來源綁定的採納方式見[插畫裁切覆寫契約](image-crop-overrides.md)。

art_s／art_m 由同一框分別縮放，輸出為 `4n×3n`，n 分別取 `min(k,40)`、`min(k,96)`；不放大，不因原圖太小填補像素。k≤0 屬無法產圖的診斷，不能發布聲稱完整的五檔結果。

產圖 API 的 `CropOverride` 維持 `{image_id,source_sha256,left,top,width,height,reason}`。其中 `image_id` 是產圖用的 `img:v1:`，不是建置資料庫／公開快照的 `image_asset.id`（`img:binding:`）。正式 authored 列**不存 image_id**，以 `(source_key,source_sha256)` 為鍵；loader 由來源版本推導 `img:v1:` 再組成 `CropOverride`。兩種 ID 的完整公式、分片與輕量核可收據見[覆寫契約](image-crop-overrides.md)。呼叫端設定不能取代 authored 的有效採納。

座標為套用既有方向資訊後的整數像素，框須位於圖片內、width/height 為正且精確 4:3。source_sha256 必須匹配凍結來源 bytes；同一來源資源換 bytes 後，未採納的新 hash 必須停止產圖，不能靜默退回預設或套用舊框。有效覆寫優先於預設框，無效覆寫停止該圖產製並要求修正。直向與橫向都接受符合上述約束的 art 覆寫。框與核可收據只在建置端，不加進快照；來源圖的 publication_state／availability 規則仍獨立適用。

## recipe、原圖與發布

`image_size.is_original` 保留在建置 schema，表示「原始來源 bytes 的直接副本」；本契約五檔一律 false，card_l 即使與來源同尺寸也不是 original。公開 config.image_sizes 不投影 is_original，發布器拒絕任何 is_original=true 或 format 非 webp 的 variant；image_size 不是 source inventory，不為原 PNG 建公開檔位。

recipe 必須固定解碼／編碼器與底層 libwebp 版本、品質、色彩轉換、alpha、metadata 移除、縮放濾鏡、方向處理及以上取整規則。recipe_version 連到完整建置 recipe；相同來源 hash＋覆寫框＋recipe 必須產生相同 bytes，不以「品質約 80」作可重現性保證。recipe 變更產新的建置 hash/path 並重新分層抽驗版型、地區、來源寬度、文字邊、細線與臉部；不覆寫內容定址的建置 blob。不同檔位若產出完全相同 bytes 可共用建置 blob，但公開 2.0 的每版次／面／尺寸 key 仍各一份。

建置 path 仍為 `images/sha256/<前兩碼>/<64hex>.webp`，hex 為實際 WebP bytes 的 SHA-256；這也是 1.x 公開 path。2.0 公開 key 改如下節，不由來源 image_id／hash 決定。image_variant.bytes/width/height 記當次輸出；發布器驗完整 SHA／尺寸／bytes，不能只驗清單。2.0 reader 不用歷史 SHA 去拒絕同 key 的新 bytes；JSON hash 與建置來源 pin 不變。

官方圖片經來源驗證成為 approved、第三方圖經人工確認成為 approved 的規則，統一見 [build-db.md §17.1](build-db.md#171-mirror_reviewed)。只有 availability=available 且 publication_state=approved 才可出 variant；missing/unfetched/pending/withdrawn 不出 path。直向與橫向 approved available 的圖都需五個檔位，缺檔不得宣稱影像閉包備妥；文字預覽可明示尚無影像結果。先完成資產閉包才發布引用它的影像清單；第三方圖片逐圖確認規則仍依 [build-db.md §17](build-db.md#17-已決政策非官方圖鏡像與地區-decklog-建牌資格)。

卡圖按需快取或只抓已選牌組與雙面，不預抓全庫；數位卡圖僅連官方頁，不混入 SVE 衍生檔。裁切圖的使用頁仍保留來源與版權標示；原始卡圖不是專案可再授權素材。

## 2.0 圖片 URL、版本與新鮮度

依 [ADR-0015](../adr/0015-image-url-version.md)，公開 key 為
`images/<size>/<int_id>[-f<ordinal>].webp`，reader 加上 `?v=<version>`。
f0 省略，其他面用永久 face.ordinal；printing.int_id 永不重配，不能用卡號、image_id 或列位置代替。
例如 `images/card_m/20001.webp?v=3` 與 `images/art_s/20001-f1.webp?v=7`。
五個 size_key 固定；int_id／ordinal／version 的 URL 向量須由 Python／TS 共用合成測試驗證。

版本由 printing.home_set 的 printing_image media 列提供。card 三檔共用 card_version，art 兩檔共用 art_version。
使用發布配號器保留的單調正安全整數 revision，各組沿用最後變更號，不需每圖連號；純文字更新不全庫換 v。
依輸出 bytes、尺寸、binding／可用性比較是否變更；crop 只換 art，組內一尺寸變更可使該組一起換 URL。
缺圖／撤下後恢復、改面與 A→B→A 回復均配新號，不能回用古老 v；舊 query 冷讀可能已快取較新的 bytes。
失敗預留號也不回收，相同計畫重試必須同號同輸出。此狀態耐久保存並備份，不由兩版 CDN 索引反推或重設。
輸出 recipe、瀏覽器時間及每次全庫 data_version 都不能直接代替此事件版本。

發布流程：

1. 單一寫入者或等效鎖定，釘 current revision、候選內容 SHA／尺寸及 remote ETag，耐久記錄上傳計畫與保留 revision。
2. 新圖片 key create-only，既有圖片以 If-Match 條件覆寫；只寫實際變動檔，未知 bytes／競爭停止。JSON／manifest 仍不可覆寫。
3. 全組 origin bytes、尺寸及新 v CDN response 驗妥後，寫入新版不可變 JSON／manifest，最後 CAS 切 current／previous 索引。失敗不發布半套 current。
4. 提交後按 current 圖片集合清理撤下／不再使用 key，metadata 只保 current＋previous 聯集；GC 重驗 revision 並避開合法在途發布。

同一組多尺寸覆寫並非跨物件原子交易；舊快照在寫入途中可能看到新舊尺寸混合，新快照只在全組就緒後發布。
previous 的舊 v 冷讀拿到目前 bytes 可接受，metadata 不重寫，也不要求 origin 同時符合兩版圖片 hash。
中斷後先核對未完成計畫並向前恢復；不能只看 current manifest 的差異就忽略 origin 已寫入未發布 bytes。
若改採另一批輸出，配新號再驗，不假設仍能取回舊圖。

新快照啟用後，browser 的 src／srcset 和 SW cache key 一律切新 v；SW 禁止 ignoreSearch 或只按 ID／尺寸匹配，
舊 response 晚到不得回寫新版 view。新圖下載／decode 未完成顯示 placeholder，失敗明示，不拿舊 v 當 fallback。
尚未取得新版的離線裝置無法得知更新，須明示時效；接受新版後不得等待舊 TTL 才換圖。
CDN cache key 必須包含 v，rewrite／Worker 不得去掉 query；只 purge CDN 清不到 browser／SW。
版本入口 no-store 或每次強制重驗。圖片可用 `public,max-age=86400,must-revalidate` 作初始配置，
正確性靠新 URL；must-revalidate 不表示 fresh cache 每次重驗，舊 v 可取新 bytes，因此不把 URL 宣稱永久 immutable。

2026-10-04 已在 R2 自訂網域用合成圖驗證：普通 GET 暖 v1=A，未 purge 覆寫為 B，v1 仍 HIT A、v2 MISS B 後 HIT B。
query 分離門檻已通過，未代驗瀏覽器／SW。正式網域核對設定，Cache Rules／Worker／rewrite／網域變更時回歸同一測試，
不能用 bypass-cache 請求冒充通過。發布前不預暖尚未上傳的新 v；若新 v 已有錯 bytes／負快取，驗收失敗，處理後重驗。
另驗 browser／SW 全暖、網路／quota 失敗、撤下、晚到舊請求與回復版本；opaque response 不能稱為已驗 SHA。
若部署無法可靠維持 query 分離，改採不重用版本檔名，須同步 producer／reader 契約，不能悄悄改 URL。

卡圖只保存 current bytes；永久保留來源 PNG、inventory 與 authored 證據的規則不變。
回收與舊 client 過渡詳 [snapshot-format §4.1](snapshot-format.md#41-發布窗口圖片新鮮度與回收)。
