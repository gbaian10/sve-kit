# 卡片身分與卡名概念的關聯

本契約定義 SVE 永久 card／face 及 exact 名稱與 card_name 概念的關聯，
使用[翻譯當前資料格式](translation-contract.md)。保留名字功能與身分檢查，
不再要求 delegation、identity_basis、採納續版鏈或核可收據；非翻譯身分登錄規則不變。
本文件描述目標介面，不代表全部 owner／renderer 已實作。

## 1. 入口與邊界

概念及當前選詞走 glossary_term／glossary_choice，永久 `term:<concept_key>` 不隨改字重配。
context_assignment 只處理真實同字異義；構築 rules_name／face_rules_name 不當作卡名概念。
數位官名走[名字規則](digital-name-policy.md)，不因同名瀏覽連結取得同概念資格。
本次有效 registry／identity transition／source_face_map 仍按身分契約產生並驗證，
不能從 URL alias、名字或面 ordinal 猜 card／face 對應。

## 2. card_name_concept 當前格式

只記需要明示的例外，存 `translations/overrides/<filing_key>/<sequence>.yaml`。
record 欄位沿翻譯 format 2，origin／low_confidence 可省略為 project／false；record_key 不存檔，載入時計算為 `["card_name_concept",subject]` 的 canonical JSON 字串。
data 恰為 `{subject,term_id,source_ref,reason}`：

| 欄位 | 定義 |
| --- | --- |
| subject | `{card_id,face_id,source_lang,source_hash}`；永久卡／面、ja/en、完整 exact 名稱 UTF-8 hash |
| term_id | 已存在的 card_name 概念；null 表示撤回例外，回預設推導，不是永久禁止翻譯 |
| source_ref | 六欄引用，定位該 SVE 卡面完整 name；語言／hash 與 subject 相同，不用效果摘錄或數位名字代替 |
| reason | 非空的同概念／撤回理由，不記私人檔案、點擊或人審證明 |

同一 subject 只有一筆當前值，可以直接改 term_id；重複鍵、錯 category 或錯來源使建置失敗。
來源卡頁的原樣 region/card_no、parser source_index 與本次有效 source_face_map 必須共同支持 subject，
不只驗外鍵存在。default 可以唯一推導時不需要另寫關聯。

## 3. 名稱 owner 的預設綁定

owner 為 face_revision.name 或已知 printing_face.name，field=name、ordinal=null。
每次建置按有效 card／face、語言及自己的 exact 名稱解析：

1. 使用該 subject 的非 null 例外；沒有或已撤回則回預設。
2. 預設用 `(來源語言,完整 exact 名稱)` 對 card_name 概念的原文，恰一個才使用。
   不 trim、normalize、模糊搜尋或依譯名合併；目前 glossary 原文為 ja，不能當作 en。
3. 多概念列 ambiguous_name_concept，沒有列 missing_name_concept；EN 特例須有自己的來源與明示關聯，
   不從已確認日英同卡關係自動推論日英名稱為同概念。
4. 查當前 target_lang choice；缺譯或撤回列 missing_term_translation。結構錯誤仍失敗，不冒充缺譯。

有效 context_assignment 的 source_hash 等於該 owner 原文；concept_key 與選中 term.concept_key 相同。
矛盾使建置失敗；同字異義要有理由的 variant，不能由 card_id 配出假語義分支。
同字同概念共用 term／variant；新 owner 加入而產生歧義時重驗所有相關用途。
直接名字可由合格數位規則供詞，不因尚無 glossary 概念而阻擋；效果內概念引用仍依自己的資格。

printed 只用自己已知名稱；unknown／omitted 不借 current，舊印刷名保留自己的 source_hash。
表記／效果 pending 不整卡排除已知名字。選詞改字重算相依翻譯，沿 render-v2 內容鍵；
context/use/selection 是建置產物，不寫回 authored，也不逐卡重新核可。

## 4. 卡文中的卡名引用與選面

`{kind:card,id}` 的 id 是永久 card_id，預設名稱面固定為 front／ordinal=0 的有效面。
不是第一個 SQL row、數位 normal／evolved 或當次來源恰好使用的面；一般進化前後仍是不同卡。
引用須有有效卡片身分，依該區真正來源解析名稱，不能靠名字猜 card_id 或缺 JP 就猜 EN-only。
明指背面／其他名稱時用 card_name 的 `{kind:term,id}`，不得在 card 參數偷加 face_id；
schema 種類改變形成新模板 ID，重驗 raw span／引用及譯文。
缺概念、歧義、目標退役或缺必要譯詞時整個效果 context 回原文；壞外鍵／來源 hash 是建置錯誤。

## 5. 數位官方名稱不能從共用 context 借資格

每個 owner 驗自己的完整名稱與有效名字規則，或自己合法的真人同卡精確面連結。
same_character、same_name、同 context、數位前後面同名都不授予官方名稱／同概念資格。
逐名排除不能借任何 link 繞回自動官名；明示選詞覆寫另按名字規則。
owner 限定的官名直接由 FieldTranslation 引用精確結果，不能塞共用 selection 讓其他 owner 借用，
也不以假 variant 分隔資格。普通同概念譯本仍共用 selection。
未支援的 owner／印刷模式列能力缺口，不宣稱已可用；數位官名不使效果變官方。

## 6. 身分修復與轉換

merge／split／reassign 後按有效父卡／面重算，不沿修復路由搬舊例外或官方資格。
需要例外的新 subject 另寫記錄；撤回修復也要重驗，不自動復活失效指派。
只有效果改字而永久 card／face、語言、名稱 hash 未變時，card_name_concept 仍可用；
use 重建，context_assignment 不從舊 revision owner 搬到新 owner。
身分契約要求的 transition 驗證仍照常，未支援的 transition 不得跳過。

轉換保留有效概念、選詞、null 撤回及原 origin；舊歷史封套只用來取出有效值，Git 歷史不改寫。
不保留新格式的人工核可 hash／前件／不可變背景；公開 low_confidence 及 reader 版本依翻譯契約配套啟用。

## 7. 永久 key 與自動檢查

card_name 沿穩定英文 `name.<slug>`，slug 符合 `[a-z][a-z0-9]*(?:_[a-z0-9]+)*`，
完整 key 最多 96 bytes；名稱只用來配內部代號，不因此確認跨區或數位同卡。
沒有英文名可用簡短意譯／羅馬字代號；不以草稿序號、卡 ID、hash 或翻譯改字重配永久 key。
全入口驗字元、長度、唯一性；相同概念重用，真實異義用語義限定詞區分。
不要求另附協調者委託／點擊收據。

必要反例涵蓋同字異義、撤回回預設、雙面選面、printed 舊名、EN 不借 JA、
來源錯配、缺譯回原文、第三 owner 不借官方資格、身分移轉不搬例外及選詞修改傳播。
報告只列 owner／ID／hash／原因，CI 不輸出官方全文；資料 PR 不要求逐 guard 定向突變。
