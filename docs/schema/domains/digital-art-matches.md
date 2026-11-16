# 線上版卡圖採納

`authored/digital/art-matches` 保存同卡卡圖的機械採納結果。原圖、特徵、
模型與完整比較報告不進 Git；普通離線建置只讀本入口及已釘住的數位 API，
不讀私人圖片快取、不重跑比較。圖的存在不推導同卡、名稱或語音資格。

## 入口

入口由 `domains.digital.art_matches.loader.load` 一次驗完整檔案閉包、
canonical shard hash、欄位與唯一 subject；未知欄位、符號連結、未列檔案、
重複 art/game 或不合法結論均拒絕。每檔嚴格小於 1 MiB。

| 路徑 | 欄位 |
| --- | --- |
| `digital/art-matches/index.yaml` | `format:1, kind:digital_art_match_index, recipe, opencv_version, as_of, includes` |
| `digital/art-matches/<filing_key>/<sequence>.yaml` | `format:1, kind:digital_art_match_shard, records` |

每筆 record 包含 `art_id, card_id, face_id, game, state, same_card_targets,
witness, candidates, complete, review_level, reason`。art 沿有效 registry 採納分組，
同組 PR、UR、閃卡與跨區版次共用結果；不按卡號／稀有度猜圖。
`same_card_targets` 是當時該卡、該遊戲的同卡目標 ID，排序去重；不是名字搜尋結果。

`witness` 為 `printing_id, face_id, image_url, image_sha256, crop`，不可得時為 null。
candidate 為 `official_id, phase, style_key, image_url, image_sha256, crop, inliers`；
phase 為 normal/evolved，crop 是原生像素的半開整數框 `[left,top,right,bottom]`。
候選依全部同卡目標列舉正常、進化、Prize／異畫及 SVWB styles；任一同圖即有數位圖，
不可只比普通版。圖片 URL 與 hash 是採納斷言，不是假造已封存圖片的 source_record。

## 固定 recipe 與三態

沿 [art-candidates](../images/art-candidates.md) 的
`sift800-ratio075-ransac5-v1` 與 `images.art_sift.features/inliers`。
不另維護特徵或 cosine 主判定。SVE 沿圖片 producer 的 `crop_box` 與已採納覆寫，
保留原生裁圖像素；數位輸入也裁為 4:3，不拉伸。
SV1 框為 left=14%w、top=23%h、width=76%w；SVWB 為 12%w、22%h、82%w，
寬先取不超過比例值的 4 倍整數，高取其 3/4。

| state | 條件與意義 |
| --- | --- |
| matched | 至少一張候選 >=100 inliers；不要求其他候選已齊全 |
| not_found | 完整候選全部 <10；只表示此 as_of、此同卡候選範圍未找到同圖 |
| unknown | 缺資料、灰區或尚未核對；缺列同樣為 unknown |

既有工具 >=40 可合組，但 10～99 同時仍是 review；本入口不將這段直接採為 matched。
首批機械結果保持 `model_reviewed`，不冒稱真人 sampled/confirmed；灰區留 unknown。
not_found 不能因缺 link、低 cosine、PR／UR 標籤或讀圖失敗而產生。

SV1 非零 `resource_card_id` 先解析為圖片資源 ID，0 回自身 card_id；base_card_id
只作家族，不代表同圖。正常與進化面各有明示身分。SVWB `card_resource_id`
沒有 SV1 的沿用語義，不能套同規則；依 common/evo/styles 列圖。
`style_key` 是保存於 authored 的永久標籤；base、SV1 的 ts-rotation 與首次配發的 alternate 標籤保留，
不能隨 API styles 重排重配，也不以圖片 hash 作永久身分。
同遊戲／ID／phase／style_key 不得指向不同 URL 或圖片 hash，跨分片亦會檢查。

## 建置與公開

composer 將入口的 exact index/shard hash、recipe、OpenCV 版加入 BuildContext。
intake 驗 art/face 與 witness 的現行歸屬、fresh 同卡目標集合、數位 API 圖片 metadata；
負面另外驗全部目標的面與 styles 仍被候選涵蓋。任一範圍改變退 unknown，
不能繼續發布舊負面。正面取 >=100 的候選建立 `digital_art`／`digital_art_link`；
`digital_art_coverage` 以 `(art_id,game)` 保存 state/as_of。
數位卡／面與名稱最小閉包重用 `import_digital`，重複插入須逐欄相同，衝突失敗。

採納來源為真正的 authored source_record，parser_version=`digital-art-authored-v1`；
每筆 decision 連原分片，state 保持實際 review_level。不同 render 的 bytes/框/hash
留各筆 authored candidate，數位 art 的永久 ID 不由 render hash 重配。
本入口沒有擴張真人名字或語音資格。

公開 `digital_art_coverage` 恰為 `art_id,game,state,as_of`，按 art owner 的 print
purpose 分片；既有 digital_art_link 保留正面關聯與實際 review_level。
不公開候選分數、裁框、原圖 URL、來源證據或私人路徑。只放行 art，不放行 voice。
reader 拒絕 matched 與正面 link 不一致的 coverage。

前端以所選 printing face 的 art_id 讀取；任一應查遊戲 matched 即有數位圖，
全部應查遊戲為完整 not_found 才可列專屬，unknown 不混入專屬。
卡本身沒有數位對應的判定仍由獨立卡級 coverage 負責。

## 卡片頁連結去重

沿維護者在 [#2277 的決定](https://github.com/gbaian10/sve-kit/issues/2277#issuecomment-6917797088)，
同卡、同當前顯示文字／數值，而且正常／進化所有圖均由同 recipe 確認相同時，
authored/digital/links 只保留來源 base/resource 指回的最原始 ID。
不同圖、不同文字、缺圖或灰區皆保留，不能按 810／7xx 前綴刪除。
例如 810031040／810031050 與 107031020 同圖而去重；
123114010／715114010 為不同異畫，兩個都保留。
Git 保存刪除歷史；不新增第二份 alias 帳本。快照與卡片頁直接讀已去重的關係，
不在前端另訂判定。
