# 卡圖採永久版次路徑與按需版本

查卡需要快速定位圖片，又要在更正圖片後讓已接受新快照的使用者看到新圖，且不保留公開歷史圖片。採永久版次 ID 路徑配 query revision，狀態、尺寸及 card/art 版本放按需卡包 media 分片；多取一層 metadata 並放棄跨版次圖片去重，換取固定 key 的簡單回收與圖片更新不迫使文字啟動分片重抓。URL、匯出端配號、快取及非原子覆寫的限制見[圖片契約](../schema/images/image-variants.md#20-圖片-url版本與新鮮度)與[傳輸契約 §5.4](../schema/export/snapshot-transport.md#54-format-200-卡包-media-與-id-圖片)。
