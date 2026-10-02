# Cloudflare 開發環境設定與 preview 上傳

這份清單供維護者親手操作，agent 與 CI 不取得 Cloudflare 憑證。設定清單與上傳工具
不代表環境已上線；以下驗收全部完成後才可提供開發資料。此處未登入或查詢 Cloudflare，
控制台選項名稱與可用方案須由維護者依當下介面及官方文件確認。

既有資料契約見 [preview 建置與接線](../schema/preview-handoff.md)。工具只上傳
JP preview，沒有正式發布、正式 index 或日英首發放行功能。

## 1. 先填環境表

下列開發名稱是建議，維護者可在設定前替換；正式名稱只先保留，不上傳資料。

| 項目 | 開發 | 正式 |
| --- | --- | --- |
| 前端 | `dev.svekit.app` | `svekit.app` |
| 公開物件入口 | `cdn-dev.svekit.app` | `cdn.svekit.app` |
| R2 bucket | `svekit-dev` | `svekit-prod` |
| Pages project | `svekit-dev` | `svekit-prod` |
| 存取 | Access 指定人員，預設拒絕 | 公開前另行確認 |
| token | 只限開發 bucket | 不共用開發 token |

主站使用 `svekit.app`，查卡、建牌、對戰共用前端與路徑，不另建查卡網站。
`svekit.com` 保留到正式主站就緒後才設定轉址；本輪不建立轉址或對戰伺服器。

維護者記下 account ID、bucket、project 及核准登入人員；秘密只在本人本機。
不要把 token、S3 key、登入 cookie、私人建置路徑寫進 repo、PR、CI 或分享的報告。

## 2. 維護者建立 R2 與網域

1. 登入已啟用 MFA 的 Cloudflare 帳號，確認 `svekit.app` zone 及 account。
2. 在 R2 建立兩個不同 bucket，分開保存開發 preview 與未來正式產物。
   開發資料不得先放入正式 bucket；兩者不得共用寫入 token。
3. 關閉開發 bucket 的公開 `r2.dev` 存取，不建立其他未受保護的公開入口。
4. **先完成第 4 節 Access 規則，再連接開發 custom domain。** 將
   `cdn-dev.svekit.app` 綁定開發 bucket，依控制台指示建立或核對 zone DNS，等待 TLS 生效。
   不自行把 S3 account endpoint 當作 CDN，不另開繞過 Access 的 Worker 或網域。
5. 正式 bucket 先保持無公開入口；`cdn.svekit.app` 的公開設定與正式發布另行執行。

參考 [R2 custom domains](https://developers.cloudflare.com/r2/buckets/public-buckets/)。
驗收時必須檢查 `r2.dev` 與所有已綁定入口，不能只檢查預期的 CDN 名稱。

## 3. 維護者建立單 bucket token

1. 在 R2 的 API token 管理入口建立供本機上傳的 token，選擇 Object Read & Write，
   範圍只包含開發 bucket；不選全部 bucket 或管理帳號的權限。
2. 保存該 token 產生的 S3 access key ID 與 secret access key，與 account ID 分開辨識。
   上傳工具使用 S3 相容 API 的 key pair，不使用一般 Cloudflare Bearer token。
3. token 只留維護者本機的私人儲存，不進 git、不交給 agent、不設定成 CI secret。
   正式 bucket 要另建 token，不用本輪 key 操作。
4. 每次執行只把 key pair 載入當次本機 shell 的 `SVE_R2_ACCESS_KEY_ID` 與
   `SVE_R2_SECRET_ACCESS_KEY`；使用不回顯的輸入或本機秘密管理工具，勿將字面值打進歷史。
   結束後 unset 兩個變數。

參考 [R2 tokens](https://developers.cloudflare.com/r2/api/s3/tokens/)。讀取權限是逐物件
驗 hash、bytes 與 metadata 所需。平台的寫入權限可能同時允許刪除或覆寫；**create-only
是工具的條件寫入契約**，不宣稱 API token 自身能禁止所有覆寫。工具沒有 delete 操作。
維護者須確認 token 不能操作正式 bucket，並設定適合的到期／撤銷方式。

## 4. 維護者設定 Access 與跨來源存取

1. 在 Zero Trust 設定本人要用的登入方式，例如一次性登入碼或既有身分提供者。
2. 為開發前端及開發 CDN 建立涵蓋全路徑的 Access application，Allow 規則只包含
   維護者列明的 email／群組。未命中者預設拒絕，不設 Everyone 或任意 Bypass。
3. 不建立給 agent 或 CI 的 service token；R2 S3 API 上傳仍使用第 3 節的單桶 key pair。
4. 建立 Pages 後，逐一檢查 project 的 `pages.dev`、branch alias、每個 deployment/hash
   URL。只保護 custom domain **不足以證明其他別名被保護**。若平台能全面保護就啟用；
   若不能，須先有獨立前端／Function／Worker 的 host 拒絕保護，涵蓋靜態路徑與所有入口，
   並實測無登入回應，完成前不可放開發卡片資料。
5. R2 CORS 只允許開發前端的精確 HTTPS origin，方法限 GET、HEAD、OPTIONS；不允許
   萬用來源配合 cookie，也不開啟瀏覽器 PUT／DELETE。若驗 hash 的 reader 需要讀 ETag，
   將 ETag 列入 expose headers。CDN 與前端的 Access 登入、cookie、fetch credentials
   及跨來源回應須共同驗證。
6. 若 Access 擋住瀏覽器 preflight，依當下官方功能設定 OPTIONS 處理；不以放行
   GET 或整個 CDN 的 Bypass 解決。登出後 GET 仍須拒絕，登入後真實跨來源讀取須成功。

參考 [Access self-hosted applications](https://developers.cloudflare.com/cloudflare-one/access-controls/applications/http-apps/)
與 [R2 CORS](https://developers.cloudflare.com/r2/buckets/cors/)。

Access 與 CORS 控制不同事情。CORS 不會使未登入的直接下載者失去存取權；所有別名與
public bucket URL 都須另驗拒絕。也不能只看到登入頁就宣稱前端能載入 CDN。

## 5. 維護者建立 Pages project

1. 建立獨立開發 Pages project；可先使用本機建置後的 Direct Upload，避免把 R2 token
   放入建置 pipeline。Pages 建置本身不需要 R2 上傳 key。
2. 前端產物由 repo 根執行既有建置指令產生：

   ```bash
   mise exec -- bun run --cwd sim/web build
   ```

3. 本機產物檢查通過後，由維護者親手依 Pages 上傳介面部署 `sim/web/dist`。
   不把 preview root、來源庫、private/ 或 reports/ 塞進前端產物。
4. 在第 4 節保護全部入口後，連接 `dev.svekit.app`，核對 DNS 與 TLS，驗證登入與登出。
   正式 project 的發布與 `svekit.app`／`.com` 轉址另行安排。

**目前上線前置缺口：** `sim/web` 的 preview base 固定 `/cdn-preview`，該路徑由本機
Vite middleware 提供，Pages 靜態產物沒有這個服務。遠端 preview base、開發切換及所有
Pages alias 的 host gate 必須另作 `sim/web` 元件變更並審核；不能用不存在的環境變數
假稱已接通。既有正式 CDN 設定不等於 preview 接點。可建立空 project，完成前勿部署
會暴露真實資料的產物，也不宣稱 #200 完整上線。

## 6. 維護者先做本機離線對帳

來源是含 `snapshots/` 與 `images/` 的 preview root，須為絕對且無 symlink 的路徑。
工具不用 `SVE_DATA_DIR`、不開 live manifest、不抓官網，也不修改 preview。

```bash
uv --directory carddb run sve-carddb r2 upload-preview \
  --preview-dir /explicit/preview --dry-run
```

有 `.br` 時加 `--brotli-command /explicit/trusted-encoder`，使用 producer 已選用且
支援 `--version`、`-q 11 -c` 的可信任離線程式。未提供或重新壓縮的 bytes 不同即停止；
不得刪掉 `.br` 或改配方讓驗證放行。沒有 `.br` 時只需既有 Python 依賴。

核對 `candidate_files`、`candidate_bytes` 與 `files_by_kind`／`bytes_by_kind`，再核對
manifest pin 是否為要發布的 preview。`private/`、`reports/` 明示排除且不讀內容。
遠端存在與新增上傳量在離線模式未知，不能將 candidate 數字稱作遠端新增量。
PNG、未知檔案、未引用資產、私有欄位／配方、本機路徑、壞 hash／壓縮／圖片尺寸均停止。

## 7. 維護者驗條件寫入後，當次授權上傳

任何真實請求都由維護者當次同意後親手執行；本機測試只用了合成資料與 MockTransport，
尚未驗證真實 R2。先以**合成 preview 與開發 bucket**驗首次寫入、相同內容重跑、
既有物件不同內容拒絕、If-None-Match 與 If-Match 競爭。先證明平台支持所需條件寫入；
失敗就停，不加強制覆寫或改用通用 sync。使用同一公開布局，勿往同桶混入無關測試物件。

核對本機對帳、目標桶、單桶 key 及當次授權後，才可上傳真實 preview：

```bash
uv --directory carddb run sve-carddb r2 upload-preview \
  --preview-dir /explicit/preview \
  --account-id "<32-lowercase-hex-account-id>" --bucket "<development-bucket>" \
  --execute --confirm-maintainer-authorization
```

有 `.br` 再加明示 encoder。尖括號參數須替換，key 只由當次環境變數取得。
不要向 agent 貼 token 或執行帶 token 的除錯 trace。

工具保留 #190 布局，不另造 `versions/<version>/`：卡圖為
`images/sha256/<前兩碼>/<64hex>.webp`；快照先傳內容定址 `snapshots/blobs/`，
再傳 `snapshots/manifests/` 的不可變版本集合。圖片與這些成員都只新增，不覆寫。
既有物件必須 bytes、hash、content type、cache control 相同才 skip。

全部成員逐一確認且本機清單再次驗證後，最後用舊 ETag 的 If-Match 更新
`snapshots/preview/current.json`；第一次用 If-None-Match。指標競爭就停。
不寫正式 `snapshots/versions/index.json`，不刪遠端物件。

原 JSON 為 `application/json`，WebP 為 `image/webp`；gzip／br sibling 是獨立 key，
用 `application/octet-stream` 且沒有 Content-Encoding，不當作 CDN 自動解壓回應。
開發不可變成員用 `private, max-age=31536000, immutable`，指標用 `no-store`。

## 8. 中斷、恢復與最後驗收

任何異常先停止，由維護者核對記錄。已完成的不可變物件可以留在桶中；用同一份凍結
preview 再次離線核對，重新取得當次授權後重跑，匹配物件會 skip，指標仍在最後更新。
不要清桶、改既有物件、強制更新指標或用一般目錄同步工具繞過驗證。執行期間凍結本機
preview，避免本機修改及並行發布。若指標 PUT 已完成但回應遺失，指標仍只指向先前
已驗完整成員；重跑會再次確認。不同既有物件或指標競爭需查原因，不盲目覆寫。

維護者完成以下實際檢查後才可稱開發環境可用：

- 未登入：開發前端、CDN 全路徑、pages.dev／branch／deployment aliases、r2.dev
  及其他綁定入口皆無法取得卡片資料；正式桶亦未暴露開發資料。
- 已登入：前端能跨來源讀取指標、清單、分片與卡圖；hash 驗證、缺圖占位、pending
  標記正常，登出後不能繼續取得新資料。檢查 Access、CORS 與 cache headers。
- 首次及重跑：報告的 uploaded/skipped 檔數與 bytes 對得上 candidate，重跑不 PUT
  已匹配物件；前端 pin 是本次完整 manifest。中斷後舊完整版本仍可用。
- key pair 已從當次 shell 清除，沒有進入 repo／CI／前端產物／分享的記錄。

本清單及工具不完成 region_text_review、正式 manifest、正式 CDN 發布或正式 Access
政策；這些仍走各自的資料放行與維護者部署決定。
