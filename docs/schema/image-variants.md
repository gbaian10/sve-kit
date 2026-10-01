# 卡圖衍生檔契約

來源圖與衍生檔分開：原始 PNG 留在資料來源與備份，不公開至 CDN、不作 image_variant 的必備 original 檔。公開的整張卡圖亦重新編碼為 WebP。來源網址仍保留於 image_asset，公開衍生檔走內容定址 path，不能由卡號推算。

## 檔位與尺寸

| size_key | purpose | max_width | max_height | 直向來源的輸出 |
| --- | --- | --- | --- | --- |
| card_s | card | 128 | 179 | 整卡寬 128，等比例 |
| card_m | card | 320 | 447 | 整卡寬 320，等比例 |
| card_l | card | 459 | 641 | 整卡寬 459，等比例 |
| art_s | art | 160 | 120 | 插畫裁切 4:3 |
| art_m | art | 384 | 288 | 插畫裁切 4:3 |

上述為五個固定 WebP 檔位，尺寸是上限而非保證每張原圖有足夠解析度。先 decode 並套用來源方向資訊，再依實際 W/H 判斷：W≤H 視為直向；W>H 為橫向；兩種方向均輸出 card 三檔與 art 兩檔。雙面各自處理。前端讀 image_variant.width/height，以 contain 顯示整張卡，不能固定假設每張都是 459×641。

直向 card 縮放倍率為 `min(1,max_width/W,max_height/H)`；橫向 card 三檔分別限制最長邊 179／447／641，倍率為 `min(1,max_height/max(W,H))`，不把直向 max_width 再套到橫向。縮放後寬高用 `floor(x+0.5)`，最小 1；不放大、不拉伸、不裁整卡。例：459×641 產 128×179、320×447、459×641；641×459 產 179×128、447×320、641×459。異常長寬比依高度上限可使直向寬小於名義寬，metadata 永遠記實際尺寸。

## 插畫裁切與覆寫

直向預設裁切採左 8%、右 92%、上 14%，高度由裁切寬乘 3/4 決定。像素框用純整數算式及半開區間：`left=(8*W)//100`、`top=(14*H)//100`、`k=min((84*W)//400,(W-left)//4,(H-top)//3)`，框為 `[left,top,left+4*k,top+3*k)`。`//` 表示非負整數除法向下取整，中間乘積須使用不溢位的整數計算。k 必須正值；此取整維持精確 4:3，最多損失不足 4 個寬像素。459×641 的框為 `[36,89,420,377)`（384×288）。

橫向預設裁切採左 17%、上 4%、寬 65%，亦使用精確 4:3 的整數框：`left=(17*W)//100`、`top=(4*H)//100`、`k=min((65*W)//400,(W-left)//4,(H-top)//3)`，框為 `[left,top,left+4*k,top+3*k)`。641×459 的框為 `[108,18,524,330)`（416×312）。

**使用者 2026-10-01 核可**：確認頁 43 張樣本（含 SP／SSP 與 5 張橫向 SEP）採上述直向／橫向預設框，沒有需要手動框的卡。

art_s／art_m 由同一框分別縮放，輸出為 `4n×3n`，n 分別取 `min(k,40)`、`min(k,96)`；不放大，不因原圖太小填補像素。k≤0 屬無法產圖的診斷，不能發布聲稱完整的五檔結果。

少量例外可在建置設定或 authored 維護 `{image_id,source_sha256,left,top,width,height,reason}`；座標為套用方向後的整數像素，框須位於圖片內、width/height 為正且精確 4:3。source_sha256 必須匹配凍結來源 bytes；換來源後舊框失效，不能自動裁同一區。有效覆寫優先於預設框，無效覆寫停止該圖產製並要求修正，不能靜默退回預設。直向與橫向都接受符合上述約束的 art 覆寫。框與人工審核記錄只在建置端，不加進快照。

## recipe、原圖與發布

`image_size.is_original` 保留在建置 schema，表示「原始來源 bytes 的直接副本」；本契約五檔一律 false，card_l 即使與來源同尺寸也不是 original。公開 config.image_sizes 不投影 is_original，發布器拒絕任何 is_original=true 或 format 非 webp 的 variant；image_size 不是 source inventory，不為原 PNG 建公開檔位。

recipe 必須固定解碼／編碼器與底層 libwebp 版本、品質、色彩轉換、alpha、metadata 移除、縮放濾鏡、方向處理及以上取整規則。recipe_version 連到完整建置 recipe；相同來源 hash＋覆寫框＋recipe 必須產生相同 bytes，不以「品質約 80」作可重現性保證。recipe 變更產新 hash/path 並重新分層抽驗版型、地區、來源寬度、文字邊、細線與臉部；不覆寫舊 blob。不同檔位若產出完全相同 bytes 可共用 blob，但保留各自 size_key 的 metadata。

公開 path 是 `images/sha256/<前兩碼>/<64hex>.webp`，hex 為實際輸出 WebP bytes 的 SHA-256。image_variant.bytes/width/height 均取該檔，不用來源檔大小；清單的 hash 只保護清單，下載圖片另驗 path 的 blob hash。

官方圖片經來源驗證成為 approved、第三方圖經人工確認成為 approved 的規則，統一見 [build-db.md §17.1](build-db.md#171-mirror_reviewed)。只有 availability=available 且 publication_state=approved 才可出 variant；missing/unfetched/pending/withdrawn 不出 path。直向與橫向 approved available 的圖都需五個檔位，缺檔不得宣稱影像閉包備妥；文字預覽可明示尚無影像結果。先完成資產閉包才發布引用它的影像清單；第三方圖片逐圖確認規則仍依 [build-db.md §17](build-db.md#17-已決政策非官方圖鏡像與地區-decklog-建牌資格)。

卡圖按需快取或只抓已選牌組與雙面，不預抓全庫；數位卡圖僅連官方頁，不混入 SVE 衍生檔。裁切圖的使用頁仍保留來源與版權標示；原始卡圖不是專案可再授權素材。
