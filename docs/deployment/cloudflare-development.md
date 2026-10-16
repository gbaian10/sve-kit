# Cloudflare 開發環境設定與快照上傳

部署分工與回放儲存邊界見 [部署文件索引](README.md)。

前端使用 **Workers 靜態資源**，建置與部署為 Vite build → Wrangler deploy。
本清單由維護者親手操作；agent、CI 不取得 Cloudflare 憑證，不執行真實部署或上傳。
資料契約見 [preview 建置與接線](../schema/export/preview-handoff.md)。

第 5 節配置已查核官方文件並以 Wrangler 離線打包驗證，未部署；其餘維護者提供
官方依據的內容標為「依官方文件，未實測」。控制台、Access／網域／預覽與 R2 平台行為仍為
**未驗證，需維護者實測**。範例是設定與驗收方案，不是已部署配置；
其餘官方連結供維護者查核，不代表本輪重新確認了當下的功能、方案或費率。

## 1. 同網域資料入口與環境表

開發環境採同網域方案：**dev.svekit.app 由同一個 Worker 提供前端靜態資源，
並綁私有 R2，在 /cdn-preview/ 提供資料**。桶不開公開網域；前端與資料共用
Access 保護，不建立獨立開發 CDN 網域。

讀取 handler、登入與 host gate 須審核；資料路徑不能落進 SPA fallback，須拒絕
任意 key 與私有內容，避免 cache 洩漏。Worker 轉送可能增加請求／CPU 成本，
平台行為未驗證，需維護者實測。

正式環境維持 **cdn.svekit.app 的 R2 自訂網域直出，不經 Worker**；正式發布與
網域啟用另行安排，不由本清單自動執行。

| 項目 | 開發 | 正式保留 |
| --- | --- | --- |
| 前端 | `dev.svekit.app` | `svekit.app` |
| 資料入口 | 同源 `/cdn-preview/` | `cdn.svekit.app`，R2 自訂網域直出 |
| R2 bucket | `svekit-dev` | `svekit-prod` |
| 前端 Worker | `svekit-web-dev` | `svekit-web-prod` |
| 存取 | 指定人員、預設拒絕 | 公開前另行確認 |
| 寫入 token | 只限開發桶 | 另建，不共用開發 token |

這些開發名稱可在設定前替換。主站使用 `svekit.app`；查卡、建牌、對戰共用前端與路徑，
前端與對戰服務分開部署。`svekit.com` 待正式主站就緒後轉址，本清單不啟用轉址或對戰後端。

## 2. 維護者建立 R2

1. 登入啟用 MFA 的 Cloudflare 帳號，確認 account 與 `svekit.app` zone。
2. 建立不同的開發／正式 bucket，資料與寫入 token 分離；本輪不上傳正式桶。
3. 確認開發桶的公開 `r2.dev` 入口為停用，記下控制台顯示的完整 URL（若有），不由 account ID 猜 URL。
4. 保持開發 bucket 無任何公開 custom domain；由第 5 節 Worker 的 R2 binding 讀取。
5. 正式桶保持未發布；未來 `cdn.svekit.app` 以 R2 自訂網域提供資料，不經 Worker。
   檢查沒有其他 public URL／Worker 可繞過開發入口保護。

依 [R2 public buckets](https://developers.cloudflare.com/r2/buckets/public-buckets/)
官方文件，bucket 預設不公開，使用 Access 保護時須停用可繞過保護的 r2.dev，**未實測**。
本方案另不開開發桶的自訂網域，只由受保護的 Worker 讀取。
Custom domain、r2.dev 停用後的回應與實際入口保護，仍需維護者實測。

## 3. 維護者建立單桶 token 與本機 shell

1. 在 R2 token 管理入口選 Object Read & Write，只包含開發桶，不選全部 bucket／帳號管理。
   讀取權限用來取得控制清單、JSON 與回收清單。控制台權限項目與限制能力未驗證，需維護者實測。
2. 保存產生的 S3 access key ID 與 secret access key；與 account ID、Wrangler 部署登入分開。
   工具不使用 Cloudflare Bearer token，也不讀 AWS profile 或秘密檔。
3. key pair 只留本人本機，不進 git、CI、shell 設定檔、`mise.local.toml`、前端或分享記錄。
   到期／撤銷由維護者設定。平台 Object Write 可能包含刪除與覆寫，未驗證，需維護者實測；
   **create-only 是工具的條件寫入保證**，不是 token 自身的權限模型。

參考 [R2 S3 tokens](https://developers.cloudflare.com/r2/api/s3/tokens/)。
在沒有啟動 agent、沒有除錯 trace 或終端轉錄的本機終端操作以下 Bash 指令。
不同終端不是同一 OS 使用者之間的安全邊界；不要在執行上傳時讓 agent 讀取進程環境。

```bash
bash --noprofile --norc
set +x
IFS= read -r -s -p 'R2 access key ID: ' SVE_R2_ACCESS_KEY_ID
printf '\n'
IFS= read -r -s -p 'R2 secret access key: ' SVE_R2_SECRET_ACCESS_KEY
printf '\n'
export SVE_R2_ACCESS_KEY_ID SVE_R2_SECRET_ACCESS_KEY
trap 'unset SVE_R2_ACCESS_KEY_ID SVE_R2_SECRET_ACCESS_KEY' EXIT
IFS= read -r -p 'R2 account ID: ' R2_ACCOUNT_ID
IFS= read -r -p 'Development bucket: ' R2_DEV_BUCKET
```

貼上秘密只發生在不回顯的 read 提示，不打進指令或歷史。不要 `env`、`set` 或 trace 輸出值。
作業結束執行 `unset SVE_R2_ACCESS_KEY_ID SVE_R2_SECRET_ACCESS_KEY` 再 `exit`。
Brotli 由鎖定的 Python 套件在行程內處理，不啟動外部壓縮子行程；離線檢查不讀憑證。

## 4. 維護者設定 Access 與所有入口保護

1. 選用本人要用的登入方式，建立精確 email／群組 Allow 規則，未命中者預設拒絕。
   不設 Everyone 或匿名 GET Bypass，不建立給 agent／CI 的 service token。
2. 保護 `dev.svekit.app` 全路徑，包含 `/cdn-preview/*`；前端與資料使用同一個 Access application。
   開發使用的工作階段期限建議設 **一個月**，降低頻繁重登成本；依
   [Access session management](https://developers.cloudflare.com/cloudflare-one/access-controls/access-settings/session-management/)
   官方文件，預設為 **24 小時**，**未實測**。維護者須核對 application／policy 的有效期限；
   較長期限也延長已登入者的存取時間，撤銷／登出與到期後重新登入仍須驗收。
3. 關閉開發 Worker 的 `workers.dev` 與每版 preview URL。若保留任何別名，必須逐一保護，
   並在 handler／靜態路徑都驗收；僅 custom domain 的 Access 不能當作別名安全證明。
4. 關閉或全面保護的設定、既有部署別名是否仍可取用，未驗證，需維護者實測。
   無法證明保護時，使用受審的全路由 host gate，讓別名回 403／404；未通過前不部署真實資料。
5. handler 只接受 GET／HEAD，固定剝除 `/cdn-preview/`，只讀核准 snapshots／images key，
   不列桶、不提供任意 pathname、private／reports／PNG；缺物件回 404，不能回 SPA HTML。

參考 [Access HTTP applications](https://developers.cloudflare.com/cloudflare-one/access-controls/applications/http-apps/)。
同源路徑不需要跨網域資料請求，仍須完整執行 Access 與 host gate。

## 5. Workers 靜態資源與 Wrangler 配置

Wrangler 設定放在 **`sim/web/wrangler.dev.jsonc`**；未來正式設定另放
`sim/web/wrangler.prod.jsonc`，不同 Worker／桶／網域，不共用開發部署配置。
採 JSONC 是因為 Cloudflare 建議新專案使用此格式，並可引用已鎖定 Wrangler 的本機 schema。
配置已關閉 `workers.dev` 與 preview URLs，僅綁 `dev.svekit.app`；gate 亦拒絕其他 origin。
部署設定及 R2 讀取 handler 屬 `sim/web` 元件，Wrangler 依賴另作依賴 PR，核可後才由維護者部署。

目前同一個 Worker 提供臨時首頁與 R2 讀取，配置如下；離線打包不代表通過部署驗收：

```jsonc
{
  "name": "svekit-web-dev",
  "main": "src/cloudflare/worker.ts",
  "compatibility_date": "2026-10-06",
  "workers_dev": false,
  "preview_urls": false,
  "routes": [{ "pattern": "dev.svekit.app", "custom_domain": true }],
  "r2_buckets": [{ "binding": "PREVIEW_BUCKET", "bucket_name": "svekit-dev" }],
  "assets": {
    "directory": "./dist",
    "binding": "ASSETS",
    "not_found_handling": "single-page-application",
    "run_worker_first": true
  }
}
```

`main` 指向受審的 Worker entrypoint，`PREVIEW_BUCKET` 綁開發桶，`ASSETS` 提供前端。
配置以 `assets.run_worker_first: true` 讓 handler 在 SPA fallback 之前驗 host，
資料路徑讀 R2，其餘委派 `ASSETS.fetch`；所有靜態路徑亦會經過 Worker，可能有額外計費。
`/assets/` 下以內容雜湊命名的建置輸出（200／304，且不是 SPA fallback 的 HTML）改為 `private, max-age=31536000, immutable`，
重新整理時不必逐檔回源確認；`index.html` 與 `public/` 的固定檔名檔案維持平台預設的重新驗證。
`/cdn-preview/` 僅接受 GET／HEAD 與 2.0 公開 JSON／五檔 WebP key，缺件或拒絕均不回 HTML；
R2 壓縮旁檔依 [Response 官方文件](https://developers.cloudflare.com/workers/runtime-apis/response/)
以 `encodeBody: "manual"` 保留 wire bytes。Access 保護與平台行為仍需維護者實測。
沿用發布器設定的 Cache-Control，dev 改為 private（僅將 public 換成 private），其餘指令保持原樣。
物件缺少 Cache-Control 時退回 `private, no-cache`。

Worker 對 `snapshots/blobs/<64 位小寫 hex>.json` 與
`snapshots/manifests/<64 位小寫 hex>.json`（含 `.br` 旁檔）查詢
`caches.default`，未命中時讀 R2，成功的 GET 以 `ctx.waitUntil` 寫入完整回應。
這些 key 內容定址且不可變，內部快取使用 `public, max-age=31536000, immutable`；
瀏覽器的 Cache-Control 另存於內部標頭，命中時還原並移除該標頭，維持上述 private 轉換與缺值退回。
快取鍵使用已驗證的同源完整路徑與 GET，不帶 query、Cookie、Range 或條件標頭；
不同壓縮旁檔分別存放。命中後由 handler 處理 ETag／If-None-Match 與 HEAD，保留壓縮位元組；
HEAD 未命中僅讀 R2 metadata，不寫入空 body；304、404 與錯誤不寫入快取。
`snapshots/versions/index.json` 必須取得最新版本，卡圖 key 可能覆寫，即使帶 `?v=` 也不使用邊緣快取，
兩者仍直接讀 R2 並沿用發布器的 Cache-Control。

快取只由 Worker 內部存取，Access 必須在 Worker 前驗證每次請求；命中不新增匿名入口，
也不改變 §4 的全路徑 Access、別名關閉與 host gate 要求。內部的 public 快取回應不直接送給瀏覽器。
依 [Cache API 官方文件](https://developers.cloudflare.com/workers/runtime-apis/cache/)，
此快取只存在當次資料中心、不跨區複寫，各資料中心第一次讀取仍會回源 R2。
官方文件對 Access 前方保護的 Worker 可能無法使用 Cache API 的限制敘述與實測不符，以實測為準：
2026-10-11 在 Access 保護的開發網域上，第二次讀取同一 blob 的回應帶
`server-timing: cfCacheStatus;desc="HIT"`，Worker 讀 R2 的 300～700 ms 消失。
查詢失敗會退回 R2，背景寫入失敗不影響回應；本機測試只驗證程式行為，平台命中以線上實測為準。
邊緣條目一年內不回源驗證，不要覆寫既有 blob／manifest key，內容改變須改用新的內容雜湊路徑。

維護者以本機 Wrangler 登入或最小部署權限操作，登入方式／權限需依當下官方介面核對，
未驗證，需維護者實測。部署登入只留本人本機，不把 R2 key pair 當部署 token，不交 CI／agent。
在 `sim/web` 執行 `bun run deploy:dev`，呼叫本機鎖定的 Wrangler 與開發設定檔，不依賴全域 PATH。
確認型別、測試與建置檢查後，維護者亦可從 repo 根執行：

```bash
mise exec -- bun run --cwd sim/web deploy:dev
```

`deploy:dev` 先以 `VITE_CDN_BASE=/cdn-preview` 執行既有 Vite build，再部署 `./dist`：
前端的正式資料根指向同源 `/cdn-preview/`，讀開發桶的版本索引，與本機伺服器的 `/cdn` 分開。
`dist` 只含前端程式與介面資源，不混 preview root、來源庫、卡圖、private 或 reports。
SPA 設定未找到靜態檔案回 index.html；維護者登入後用
`/cards` 及一條未對應實體檔的前端路由驗證 HTML／導航。資料路徑另驗 JSON、WebP 與缺件 404，
不能拿 SPA fallback 當上傳成功證據。Wrangler deploy 不能替代本機型別或測試檢查。

參考 [Workers 靜態資源](https://developers.cloudflare.com/workers/static-assets/)、
[SPA routing](https://developers.cloudflare.com/workers/static-assets/routing/single-page-application/)、
[Wrangler configuration](https://developers.cloudflare.com/workers/wrangler/configuration/)、
[Workers 前端部署方向](https://blog.cloudflare.com/full-stack-development-on-cloudflare-workers/)
及 [從 Pages 遷移](https://developers.cloudflare.com/workers/static-assets/migration-guides/migrate-from-pages/)。
本文不依賴 Pages project、Pages Functions 或 pages.dev 別名。

## 6. 分開的卡圖與快照離線對帳

先成功執行 `sve-carddb images build-index`，取得公開圖片根與 stdout `image_upload`；文字匯出釘住同批的私有圖片索引。
`IMAGE_UPLOAD` 指向公開 `image-upload/<sha256>.json`，`IMAGE_DIR` 為圖片根，`EXPORT_DIR` 為文字匯出根，均由維護者填入。
卡圖描述只含公開 key、事件版本、SHA-256、bytes 與 producer 身分；不把私有圖片索引或媒體狀態上傳。

```bash
uv --directory publish run sve-publish images upload \
  --image-upload "$IMAGE_UPLOAD" --image-dir "$IMAGE_DIR" --dry-run
uv --directory publish run sve-publish snapshot upload \
  --export-dir "$EXPORT_DIR" --dry-run
```

兩種 dry-run 都完全離線，不讀憑證、不開網路、不讀 bitmap。卡圖只驗描述；快照照常驗 pointer、manifest、
JSON hash／大小／Schema、br 解碼、reader join 與引用閉包，列出圖片需求，但不宣稱遠端已備妥。
`--export-dir` 可讀 `SVE_EXPORT_DIR`；完整操作見 [publish](../../publish/README.md)。

## 7. 當次授權與上傳

所有真實 Cloudflare 請求都由維護者當次同意後親手執行，agent 與 CI 不代跑。
核對目標桶、當次 key pair 與離線數字後，依序執行；僅文字更新可省圖片步驟：

```bash
uv --directory publish run sve-publish images upload \
  --image-upload "$IMAGE_UPLOAD" --image-dir "$IMAGE_DIR" \
  --account-id "$R2_ACCOUNT_ID" --bucket "$R2_DEV_BUCKET" --jobs 16 --execute
uv --directory publish run sve-publish snapshot upload \
  --export-dir "$EXPORT_DIR" \
  --account-id "$R2_ACCOUNT_ID" --bucket "$R2_DEV_BUCKET" --jobs 16 --execute
```

圖片上傳讀 `_publish/images.json`，只上傳新增或變動圖，不逐件 GET／HEAD／讀回。覆寫前先條件移除清單舊列，
所有候選成功才登錄新列；同 bytes 升版只改清單。同版本不同 bytes、版本倒退與壞清單停止，首次沒有清單整批重傳。
快照只核對遠端圖片的精確版本／bytes，JSON 只 create-only 上傳 current／previous 快照清單以外的差集，最後條件更新版本索引。
日常發布不做全量 CDN 讀取；圖片或快取規則變更時另由維護者驗收完整帶 v URL，不把 origin PUT 當 CDN freshness 證據。

`_publish/images.json` 是桶內公開卡圖的成功紀錄，以 `application/json`、`no-store` 保存。
Dev Worker 白名單不放行 `/cdn-preview/_publish/images.json`，登入後仍須 404；publisher 由 S3 讀寫。
正式環境若採 R2 自訂網域直出，接受此清單可匿名讀取，底線前綴不構成權限隔離。

## 8. 執行成本、回收與恢復

未變卡圖不讀本機檔、不逐圖網路請求，同輸入成功重跑零 PUT。快照讀清單一次並在提交前重驗，
JSON 一般成功不逐檔 GET 或讀回，只有 create-only 碰撞才下載該檔比對。`--jobs` 預設 16，I/O timeout 30 秒，進度在 stderr、摘要在 stdout。
失敗先停止並用同一圖片描述及未變動輸出根重跑；清單缺列時快照會拒絕，已完成候選可能重傳。
producer 的私有 `media-state.json` 與備份必須跨次保存，不由遠端清單重建歷史高水位。

圖片建置、上傳與 GC 都不能並行。固定順序是圖片成功 → 快照成功 → 視需要 GC，兩次上傳間不要回收。
`sve-publish snapshot gc` 僅列舉／刪除 JSON，保留 current＋previous；
`sve-publish images gc` 只處理五種卡圖 prefix，保留 current，刪前先移除清單列，亦以 LIST 找中斷孤兒。
兩者 dry-run 都會連遠端、需要憑證，詳細恢復與限制見 publish。

維護者的 dev 演練應記錄首次接管與同輸入重跑的耗時／PUT 數、無圖片掛載的文字上傳、
少量合成圖覆寫／GC 中斷後的同輸入恢復、清單條件寫、登入後控制 key 404、
未登入 Access 拒絕，以及 query 分離與 browser／SW。離線測試不代替實際部署驗收。

## 9. 每個入口的未登入驗法與預期結果

由維護者在不帶登入 cookie 的本機終端驗證。`curl -q` 不讀 curlrc，不加 `-L`、Cookie、
Authorization 或憑證選項；body 丟到 /dev/null，不印官方內容。取**已知存在**的路徑，
登入後確認同一路徑可取，不能把物件不存在的 404 當作保護證據。

先填入控制台記錄的 Worker URL／實際部署版本 URL；下面的尖括號須替換：

```bash
DEV_URL='https://dev.svekit.app'
WORKERS_URL='https://<worker>.<subdomain>.workers.dev'
WORKER_PREVIEW_URL='https://<version-preview-host-from-dashboard>'
R2_DEV_URL='https://<full-r2.dev-host-from-dashboard>'
```

| 入口 | 命令的 URL 參數 | 預期／通過條件（平台回應未驗證，需維護者實測） |
| --- | --- | --- |
| 開發前端 | "$DEV_URL/cards" | 401／403 或導向已核對 Access 登入的 302；不能匿名回 200 的開發 UI |
| 同源資料 | "$DEV_URL/cdn-preview/snapshots/versions/index.json" | 同上，不能匿名回 JSON／200／206；登入後為該指標的 JSON |
| workers.dev 前端 | "$WORKERS_URL/cards" | 已關閉者拒絕／不可用；若保留，須同樣受 Access 或 host gate 拒絕，不能 200 |
| workers.dev 資料 | "$WORKERS_URL/cdn-preview/snapshots/versions/index.json" | 已關閉或 gate 拒絕，不能 200／206 |
| 每個 preview URL | "$WORKER_PREVIEW_URL/cards"及其 /cdn-preview/snapshots/versions/index.json | 每個版本逐一測，關閉或全面保護，不能只保護 custom domain |
| r2.dev | "$R2_DEV_URL/snapshots/versions/index.json" | 先確認控制台停用，再驗不可取得；沒有 URL 時記錄未啟用，不猜 host；404／DNS 失效單獨不構成保護證據 |
| 其他綁定網域／既有預覽版本 | 依控制台列的完整 host，同樣測 /cards 及資料路徑 | 都要關閉或拒絕；新舊部署不能留下未受保護入口 |

每個表格 URL 都執行一次以下命令，例如：

```bash
curl -q --max-time 20 --silent --show-error --output /dev/null \
  --write-out '%{http_code} %{redirect_url}\n' "$DEV_URL/cards"
curl -q --max-time 20 --silent --show-error --output /dev/null \
  --write-out '%{http_code} %{redirect_url}\n' "$DEV_URL/cdn-preview/snapshots/versions/index.json"
curl -q --max-time 20 --silent --show-error --output /dev/null \
  --write-out '%{http_code} %{redirect_url}\n' "$WORKERS_URL/cards"
curl -q --max-time 20 --silent --show-error --output /dev/null \
  --write-out '%{http_code} %{redirect_url}\n' "$WORKER_PREVIEW_URL/cards"
curl -q --max-time 20 --silent --show-error --output /dev/null \
  --write-out '%{http_code} %{redirect_url}\n' "$R2_DEV_URL/snapshots/versions/index.json"
```

只測確有記錄的 URL；workers.dev／每個 preview URL 都須同時驗前端與資料路徑。
302 必須是已核對的 Access 登入入口；其他資料 host 的轉址或不明 404 不能冒充拒絕保護。
任何入口匿名回傳 200／206 都停止，先查設定。別名關閉／Access 設定的有效性不由程式假定。

維護者最後用全新瀏覽器 profile 確認未登入拒絕；登入後可讀指標／清單／分片／WebP、
hash 驗證與 pending 標記正確，缺資料路徑是 404 而非 SPA，登出後不能取得新資料。
不宣稱可以收回已授權下載的 browser cache。key pair 清除後才結束作業。

此清單不放行 region_text_review，不代替來源、採納與正式發布所需的既有守門。

## 10. 未採用的做法與原因

未採用 B 案的獨立 `cdn-dev.svekit.app`。依
[Access CORS 官方文件](https://developers.cloudflare.com/cloudflare-one/access-controls/applications/http-apps/authorization-cookie/cors/)，
Access 登入 cookie 每個網域各一份；未登入第二個網域時，跨來源資料請求會失敗並出現
CORS error，須先個別登入該網域，工作階段到期後也須重新登入。**依官方文件，未實測**。
開發環境因此採同源前端與資料路徑，避免第二個登入入口；不以匿名 GET Bypass 解決此問題。
正式 `cdn.svekit.app` 維持獨立 R2 自訂網域直出，公開操作另行安排。
