# Cloudflare 開發環境設定與 preview 上傳

前端使用 **Workers 靜態資源**，建置與部署為 Vite build → Wrangler deploy。
本清單由維護者親手操作；agent、CI 不取得 Cloudflare 憑證，不執行真實部署或上傳。
資料契約見 [preview 建置與接線](../schema/preview-handoff.md)。

本文未連外查證或部署。維護者提供官方依據的內容標為「依官方文件，未實測」；
其餘控制台、Wrangler 語法、Access／網域／預覽與 R2 平台行為仍為
**未驗證，需維護者實測**。範例是設定與驗收方案，不是已部署配置；官方連結
供維護者查核，不代表本輪重新確認了當下的功能、方案或費率。

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
   讀取權限用來驗回 bytes／hash／metadata。控制台權限項目與限制能力未驗證，需維護者實測。
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
工具給外部 Brotli 子行程的環境只含固定 PATH／LANG／LC_ALL，不傳憑證或代理設定。

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
部署設定及 R2 讀取 handler 屬 `sim/web` 元件，須以該元件的 PR 納入與審核；這份 docs 清單
不夾帶配置或程式。執行前須已有包含此工具的 carddb 版次、核可的 Wrangler 版本與配置。
新增 Wrangler 依賴須依專案規則另作依賴 PR，不用會臨時下載任意最新版的命令。

以下為同一個 Worker 提供靜態資源與 R2 讀取的配置示意；entrypoint 須先由
獨立 sim/web 單位實作與審核。所有選項語法與行為未驗證，需維護者以已核可的
Wrangler schema／官方文件核對並實測；不得直接視為已通過部署驗證：

```jsonc
{
  "name": "svekit-web-dev",
  "main": "src/cloudflare/worker.ts",
  "compatibility_date": "2026-10-02",
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
示例以 `assets.run_worker_first: true` 讓 handler 在 SPA fallback 之前驗 host／授權，
資料路徑讀 R2，其餘委派 `ASSETS.fetch`；所有靜態路徑亦會經過 Worker，可能有額外計費。
實際配置與執行順序由後續 sim/web 單位核對、測試，平台行為未驗證，需維護者實測；
不得讓缺少資料 handler 的 SPA 回 index.html 假稱資料可用。

維護者以本機 Wrangler 登入或最小部署權限操作，登入方式／權限需依當下官方介面核對，
未驗證，需維護者實測。部署登入只留本人本機，不把 R2 key pair 當部署 token，不交 CI／agent。
下列 `bun run wrangler` 呼叫 sim/web 本機已核可的開發依賴，不依賴全域 PATH；
尚未納入該依賴或 entrypoint 時先停止，不臨時下載工具。
確認前端型別、測試與建置檢查後，在 repo 根執行：

```bash
mise exec -- bun run --cwd sim/web build
cd sim/web
mise exec -- bun run wrangler deploy --config wrangler.dev.jsonc
```

既有 build script 呼叫 Vite；`dist` 只含前端程式與介面資源，不混 preview root、來源庫、
卡圖、private 或 reports。SPA 要設定未找到靜態檔案回 index.html；維護者登入後用
`/cards` 及一條未對應實體檔的前端路由驗證 HTML／導航。資料路徑另驗 JSON、WebP 與缺件 404，
不能拿 SPA fallback 當上傳成功證據。Wrangler deploy 不能替代本機型別或測試檢查。

參考 [Workers 靜態資源](https://developers.cloudflare.com/workers/static-assets/)、
[SPA routing](https://developers.cloudflare.com/workers/static-assets/routing/single-page-application/)、
[Wrangler configuration](https://developers.cloudflare.com/workers/wrangler/configuration/)、
[Workers 前端部署方向](https://blog.cloudflare.com/full-stack-development-on-cloudflare-workers/)
及 [從 Pages 遷移](https://developers.cloudflare.com/workers/static-assets/migration-guides/migrate-from-pages/)。
本文不依賴 Pages project、Pages Functions 或 pages.dev 別名。

## 6. 可執行的 Brotli 包裝程式與離線對帳

從 repo 根先依既有 setup 準備 carddb `.venv`，再使用版控內的協定入口：

```bash
uv --directory carddb sync --all-groups
carddb/tools/brotli-preview --version
uv --directory carddb run sve-carddb r2 upload-preview \
  --preview-dir /explicit/preview --dry-run \
  --brotli-command "$PWD/carddb/tools/brotli-preview"
```

`carddb/tools/brotli-preview` 使用本專案 Python，呼叫系統既有 `libbrotlienc.so.1`，
固定檢查 **libbrotli 1.0.9、generic mode、quality 11、lgwin 22**。本機實測的 203 個 br
逐份重壓皆相同。其他版本或缺函式庫即停止，不自動安裝或降級；維護者另安排環境準備。
工具接受 `--version`、`-q 11 -c`（stdin／stdout bytes），預覽來源保持唯讀。

直接安裝官方 brotli CLI 是否採相同預設視窗 **未驗證**；可能不相符，不能以「同樣 q11」
推定 byte 相同。只在重新比對全部 br 都相同後才改用別的 encoder，不能刪 br 讓驗證放行。
沒有 br 時可不提供 encoder。工具的 dry-run 不讀憑證或建立 HTTP client；明示 encoder
會執行本機子行程，但該程式不連網、不抓來源或開 live manifest。

核對 candidate_files／candidate_bytes、各類 totals 與 manifest pin。private／reports
不遍歷、不讀、不傳；公開樹的未知檔案、PNG、未引用資產、私有配方／欄位、本機路徑、壞
hash／壓縮／圖片即停止。離線數字是本機候選，遠端是否存在／實際新增量未知。

## 7. 當次授權與條件上傳

所有真實 Cloudflare 請求都由維護者當次同意後親手執行，agent 與 CI 不代跑。
先以合成 preview 在開發桶實測條件寫入、重跑與同源入口的存取控制；這次合成測試沒有
建立真實 Cloudflare 可用性的證據，平台行為未驗證，需維護者實測。

核對目標桶、當次 key pair、授權與離線數字後，從 repo 根執行：

```bash
uv --directory carddb run sve-carddb r2 upload-preview \
  --preview-dir /explicit/preview \
  --brotli-command "$PWD/carddb/tools/brotli-preview" \
  --account-id "$R2_ACCOUNT_ID" --bucket "$R2_DEV_BUCKET" \
  --execute --confirm-maintainer-authorization
```

先圖、再 blobs、再 manifests 的不可變版本集合，最後才更新 preview/current.json。
不另造版本目錄，不寫正式 versions/index、不刪遠端物件。既有物件須內容／metadata
一致才 skip，差異停；新物件用 If-None-Match。指標用舊 ETag 的 If-Match，首次用 If-None-Match。

每次執行先驗回第一個不可變成員，再以**同一份 bytes**做已存在鍵的 If-None-Match 及
錯誤 ETag 的 If-Match 探測，兩者都須回 412。若平台回成功就停止，不傳其餘成員或指標。
探測無額外 key、無 delete；壞平台可能對此鍵重寫相同 bytes，不能聲稱平台未寫入。
這是當次最低限度檢查，不代替真正的競爭與權限驗收。

GET 只對 timeout、network error、remote protocol error 重試，總共最多 3 次，退避
0.5／1 秒。HTTP 狀態錯誤、驗證錯誤、local protocol error 不重試。PUT 每次只送一次，
不變更條件、不改成無條件寫；回應遺失停止，由維護者核對後重跑。沒有隱含無限 retry。

原 JSON 為 application/json，WebP 為 image/webp；gzip／br 是各自 key 的 octet-stream，
沒有 Content-Encoding。開發不可變成員用 private／一年／immutable，指標 no-store；
Worker handler 必須保留 bytes／metadata，不能自動解壓後繞過 hash 驗證。

## 8. 執行時間與重跑成本

以這批 33,863 檔／1,121,058,055 bytes、203 個 br 為例，本機 CLI 一次完整離線驗證約
5–7 分鐘（不同負載會變）。execute 做三次完整本機驗證，約 15–21 分鐘只花在本機；
不是整次上傳時間預估。先凍結本機 preview，不並行修改或發布。

| 情境 | 不含重試的請求數／成本 |
| --- | --- |
| 首次全部新增 | 約 101,591 次循序請求：每個不可變成員 GET／PUT／GET，加指標與兩次探測；另上傳約 1.12 GB，驗回也需下載約 1.12 GB |
| 全部已一致的重跑 | 約 33,866 次循序請求；沒有新增 PUT，但有兩次預期 412 的探測 PUT；仍下載約 1.12 GB 完整核對，且再做三次本機驗證 |
| 途中出錯重跑 | 已一致成員 skip，未完成成員續傳；不從上次序號盲目跳過驗證，前面物件仍重新完整 GET |

HTTP client 每段 I/O timeout 為 30 秒，GET retry 額外等待最多 1.5 秒／操作；沒有全程
wall-time deadline。真實網路速度／Class A、B 計費與 cache 行為未驗證，需維護者實測；
可能遠超本機時間，不在網路中斷後立刻反覆重跑約 1.12 GB 的核對。

任何錯誤先停，由維護者核對原因與授權。已完成不可變物件可留桶中，用同一凍結 preview
重新離線對帳、重新取得當次授權後重跑。不同既有內容、探測失敗或指標競爭，不用強制覆寫、
清桶或通用 sync 繞過。指標 PUT 已完成而回應遺失時，指標仍只指已驗完整版本。

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
| 同源資料 | "$DEV_URL/cdn-preview/snapshots/preview/current.json" | 同上，不能匿名回 JSON／200／206；登入後為該指標的 JSON |
| workers.dev 前端 | "$WORKERS_URL/cards" | 已關閉者拒絕／不可用；若保留，須同樣受 Access 或 host gate 拒絕，不能 200 |
| workers.dev 資料 | "$WORKERS_URL/cdn-preview/snapshots/preview/current.json" | 已關閉或 gate 拒絕，不能 200／206 |
| 每個 preview URL | "$WORKER_PREVIEW_URL/cards"及其 /cdn-preview/snapshots/preview/current.json | 每個版本逐一測，關閉或全面保護，不能只保護 custom domain |
| r2.dev | "$R2_DEV_URL/snapshots/preview/current.json" | 先確認控制台停用，再驗不可取得；沒有 URL 時記錄未啟用，不猜 host；404／DNS 失效單獨不構成保護證據 |
| 其他綁定網域／既有預覽版本 | 依控制台列的完整 host，同樣測 /cards 及資料路徑 | 都要關閉或拒絕；新舊部署不能留下未受保護入口 |

每個表格 URL 都執行一次以下命令，例如：

```bash
curl -q --max-time 20 --silent --show-error --output /dev/null \
  --write-out '%{http_code} %{redirect_url}\n' "$DEV_URL/cards"
curl -q --max-time 20 --silent --show-error --output /dev/null \
  --write-out '%{http_code} %{redirect_url}\n' "$DEV_URL/cdn-preview/snapshots/preview/current.json"
curl -q --max-time 20 --silent --show-error --output /dev/null \
  --write-out '%{http_code} %{redirect_url}\n' "$WORKERS_URL/cards"
curl -q --max-time 20 --silent --show-error --output /dev/null \
  --write-out '%{http_code} %{redirect_url}\n' "$WORKER_PREVIEW_URL/cards"
curl -q --max-time 20 --silent --show-error --output /dev/null \
  --write-out '%{http_code} %{redirect_url}\n' "$R2_DEV_URL/snapshots/preview/current.json"
```

只測確有記錄的 URL；workers.dev／每個 preview URL 都須同時驗前端與資料路徑。
302 必須是已核對的 Access 登入入口；其他資料 host 的轉址或不明 404 不能冒充拒絕保護。
任何入口匿名回傳 200／206 都停止，先查設定。別名關閉／Access 設定的有效性不由程式假定。

維護者最後用全新瀏覽器 profile 確認未登入拒絕；登入後可讀指標／清單／分片／WebP、
hash 驗證與 pending 標記正確，缺資料路徑是 404 而非 SPA，登出後不能取得新資料。
不宣稱可以收回已授權下載的 browser cache。key pair 清除後才結束作業。

此清單不放行 region_text_review、不產正式 manifest，不完成日英首發或正式 CDN 發布。

## 10. 未採用的做法與原因

未採用 B 案的獨立 `cdn-dev.svekit.app`。依
[Access CORS 官方文件](https://developers.cloudflare.com/cloudflare-one/access-controls/applications/http-apps/authorization-cookie/cors/)，
Access 登入 cookie 每個網域各一份；未登入第二個網域時，跨來源資料請求會失敗並出現
CORS error，須先個別登入該網域，工作階段到期後也須重新登入。**依官方文件，未實測**。
開發環境因此採同源前端與資料路徑，避免第二個登入入口；不以匿名 GET Bypass 解決此問題。
正式 `cdn.svekit.app` 維持獨立 R2 自訂網域直出，公開操作另行安排。
