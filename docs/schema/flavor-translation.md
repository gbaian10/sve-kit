# 風味文字的整段模板

風味功能與效果模板並存，沿[翻譯契約](translation-contract.md)的當前定義／譯文格式，
不另設採納政策、首輪抽查、核可收據、雙模型門檻或第二套 renderer。

## 1. 來源與清冊

來源恰為 printing_face 的 `(printing_id,face_id)`、field=flavor、ordinal=null。
只使用該版次面已知的原文，不從 current 效果、另一版次或同名卡借用。
文字相同可共用模板，各 owner 的 use 仍獨立；表記未定不抹去已知風味。
source_ref 定位完整 flavor 字串；來源角色、語言、exact bytes 及 owner 自動驗證。

每段完整文字是一個 sentence 模板，normalizer=flavor-exact-v1、零參數、整段單一 code-point span。
保留標點、數字、名字、換行、空白；不走 NFKC、N/X 或效果提醒文拆分。
未知、exact 空字串、純空白分別列狀態；純空白不生成模板或假缺譯。
所有風味模板採新 ID，不繼承草稿流水號或舊 10 hex 分類鍵；
六欄 payload 的 level=sentence、normalizer_version=flavor-exact-v1、semantic_variant=default、
parameter_schema={format:1,slots:[]}，normalized_text 等於 exact 原文。
content_hash 取 canonical payload，normalized_hash 取原文 UTF-8；依翻譯契約 §3 的 T＋16 hex 及碰撞加長規則。
清冊 entry 的 line_ordinal=0、role=flavor，source_span={role:flavor,segments:[{start:0,end:原文碼點數}],anchor:null}。
清冊依[當前清冊契約](template-source-replay.md)重產，不重播歷史環境。

## 2. 譯文

以 `(template_id,lang)` 保存當前整段譯文，修改直接覆寫該資料並由 Git 記錄。
譯文使用同一有限文字語法；沒有 slot 時任何未跳脫的參數都錯誤。
譯文拒絕空字串、純空白、CR、首尾空白及任一行尾空白；換行只用 LF，不在 loader 偷改文字。
origin=machine 的草稿可以通過自動檢查後直接入庫；低信心顯示「待校對」且可切回原文。
來源錯配者不渲染；缺譯回原文，不阻擋其他已有效的名稱／效果。

## 3. 選用與加粗

flavor、name、effect 即使同 bytes、同 context，也必須逐 use 驗欄位資格；風味譯文不流到效果或卡名。
若既有 selection 唯一鍵不能同時表示兩者，名稱／效果優先，衝突風味回原文並報 flavor_context_conflict_rows，
不虛造語義 variant。此降級只針對選用衝突，不掩蓋錯 owner 或壞來源。
風味中的術語引用及加粗仍走共用概念／位置資料，不把 HTML／Markdown 塞進 exact 譯文。

## 4. 轉換與必要測試

舊草稿以完整來源段落對應新模板；相同來源但不同譯文列衝突，不任取一筆。
保留段落、版次面關聯與低信心旗標；來源已變者列待處理，不以舊文字覆蓋新來源。
自動測試至少驗整段 exact、不參數化、owner／role 防串用、缺譯回原文及低信心呈現傳遞。
舊政策／approval／model_review 僅供轉換，不能成為新格式入庫前置條件。
