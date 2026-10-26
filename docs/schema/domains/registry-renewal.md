# 身分登錄查核續版與插畫擴組工具

這是 [identity-transition-v1](identity-repair.md) 的兩個已實作窄入口：
`confirmed_none` 完整重審續版，以及既有 art 增加已人工確認同圖的 uses。
不改原 registry／配號分片、不移轉永久 ID、不產生 `identity_change`。
merge、split、reassign、revert、停用 review、來源更正及 art 重新分類仍拒絕。

## 執行前提

- 卡片身分與插畫同圖結論由人工確認；指令只驗證採納範圍、前件與來源，不能代替比對。
- 使用穩定 checkout；程式、lock 及 registry 基準先提交，配方釘完整 commit SHA。
- 所有輸入／輸出路徑明示，來源只讀 sealed archive；不開 live manifest、不抓網路。
- 新永久輸入批次遵守 [authored-layout §3.6](authored-layout.md#36-輸入保存)，保留舊批次 exact bytes。
  初始化決定、候選與原始確認仍各自保存；續版決策是下述獨立輸入，不把舊初始化決定當成新批准。
- 既有 EN-only 卡新增版次、換皮關係改動仍須完整採納流程，不能用這個純續版指令繞過追加工具的拒絕。

## 私人執行配方

配方 JSON 放 repo 外，欄位如下。這是工具的執行設定，沒有新增 authored kind 或更改 transition wire。

| 欄位 | 意義 |
| --- | --- |
| repo、archive、store_id | 穩定 checkout、sealed archive 根與 store 識別；兩個根為絕對路徑 |
| program_revision | 本次程式的完整 40 碼 Git commit；來源程式與依賴工作樹必須乾淨且吻合 |
| sources | 按 en、jp 排序，各含 region、card_batch、parser_version；art 擴組另需各受用區域的 image_batch |
| registry_review_inputs | 與離線配方相同的 root、batch_id、inventory_sha256、coverage_hash、bytes、as_of |

`registry_review_inputs.root` 同時定位歷史續版使用的永久批次；所有歷史批次必須保留。
art 擴組不把這份 JP 查核 pin 當作同圖證據，但仍需此根以重建鏈上既有 absence 續版。
正式建置繼續使用原離線配方，不改它的欄位；文字與圖片建置共用 `registry_plan`。

## confirmed_none 續版

先由指定凍結批次匯出**目前全部已登錄 JP printing** 的 Card JSONL，供建立新永久輸入批次：

```bash
uv --directory /absolute/checkout/carddb run sve-carddb registry review-jp-input \
  --inputs /private/renewal-recipe.json --output /private/new-input/jp.jsonl
```

指令拒絕覆寫輸出；輸出不得在 repo 或 archive 內。它不採納任何無對應結論。
只查 BP22 而尚未查 PR 時，不能把未登錄的新 PR 偷加入受審全集。
續版採納要求 JP JSONL 卡號集合**恰等於基準 registry 的完整 JP 集合**，並逐張對回凍結來源觀測。
不接受只有差集、缺卡、額外卡、舊檔原地修改或缺少來源的輸入。

完成新批次 inventory／SHA256SUMS 後更新私人配方 pin。明示決策 JSON 為：

```json
{
  "card_ids": ["c:00000000000000000000000000000000"],
  "conclusion": "confirmed_none",
  "reason": "已逐張比對新範圍，完整來源仍無 JP 同卡"
}
```

此例的 ID 為合成值，不能當真實採納。card_ids 必須排序唯一、非空，全部已有原 absence review。
指令從 registry 取每張卡全部 EN printing 的完整 observation，再逐一核對凍結 EN 來源；不接受漏面、
漏版次、跨 card 或以 confidence 代替人工結論。

```bash
uv --directory /absolute/checkout/carddb run sve-carddb registry renew-reviews \
  --inputs /private/renewal-recipe.json --decisions /private/review-decisions.json
```

預設只產生計畫摘要。`--write` 在現有全域 registry 鎖內，完整驗證後追加一個 transition 分片；
`--check` 要求所有選定 review 已匹配，不能與 `--write` 同時使用。重跑已完成採納時不寫檔。
過大的決策必須明示分組，單片仍小於 1 MiB。

## 既有 art 擴組

輸入是完整 after art 資料，不是依卡號自動分組或局部 patch：

```json
{
  "arts": [
    {
      "id": "a:00000000000000000000000000000000",
      "card_id": "c:00000000000000000000000000000000",
      "face_id": "f:00000000000000000000000000000000",
      "classification": "unclassified",
      "uses": [],
      "observation": {}
    }
  ],
  "reason": "已人工核對新增成員與原組為同一幅插畫"
}
```

這是欄位示意，空 uses／observation 與合成 ID **不是合法採納**；實際須保留原完整 observation、
原 uses 作為前綴並增加非空、唯一成員。arts 依 id 排序唯一。
ID、owner、card、face、classification、原 observation 都不可改；不移除或重排舊 uses。
完整集合必須同 card／face，且不能與其他 art 搶占相同 printing＋face。

```bash
uv --directory /absolute/checkout/carddb run sve-carddb registry extend-art \
  --inputs /private/renewal-recipe.json --decisions /private/art-decisions.json
```

同樣支援 `--write` 與 `--check`。每個原有及新增成員都驗凍結卡頁 observation、來源面、
頁面實際 img src 及 image_batch 中的原圖 bytes hash，不由卡號拼 URL。
圖片核驗只證明人工比對使用的凍結來源，不自動宣稱兩張圖相同。

## 歷史、建置與後續追加

- transition 前件使用完整 record hash 與最新 producer，previous 釘整筆鏈尾；每個歷史 registry_basis
  由 exact Git blobs 重建，舊分片 bytes 與配號游標不可回退或覆寫。
- review_context 沿現行 BuildContext 保存 program_revision 與 canonical configuration；
  dependencies 鎖檔 pin、portable artifact pin 與各區批次在 configuration，沒有加入私人根。
- absence 續版 evidence 列全部受審 EN 觀測與完整 JP 覆蓋的來源批次代表；實際重建及 F1 uses
  仍涵蓋 JP 全集的每張來源，不以一張代表卡證明全集。art evidence 逐一列全部成員卡頁與原圖。
- 文字／圖片建置先驗整條已支援的 transition 歷史，再做地區及來源新鮮度投影。
  importer 保留原 registry、歷史 transition 的 authored source_record（identity-transition-v1）與 raw uses；
  DB review 只投影最後有效內容。壞前件／缺來源拒絕建置；新 JP 未受審仍列 review_scope_outdated。
- 正常舊 reader／append 呼叫仍拒絕非空 transitions。後續追加可在原 registry module CLI 明示
  `--renewal-inputs /private/renewal-recipe.json`，先驗歷史投影，再只追加 original registry 分片；
  這不延伸舊 absence 批准，也不覆寫有效 after 或放寬 EN-only／reskin 的完整觀測檢查。

純查核續版與 art 擴組均不需要公開身分修復事件；其他 transition kind 仍等待完整修復能力。
