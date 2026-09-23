# 视频与元数据分离（可选）

此模式让视频留在 CD2 挂载的网盘目录，把 NFO、海报、fanart、弹幕和字幕写到本地。目录和文件名相对结构保持一致；数据库仍记录真实视频路径。未设置下述环境变量时，行为与上游版本相同。

## Docker 挂载与环境变量

下面的 `08bilibili` 只是示例。它必须与你在 MediaIndex 中选择的 115 网盘子目录名称完全一致。

```yaml
services:
  bili-sync-rs:
    image: ghcr.io/xudong7587/bili-sync:dev
    volumes:
      - /path/to/bili-sync-config:/app/.config/bili-sync
      - /path/to/cd2/媒体库/08bilibili:/media
      - /path/to/local/strm/08bilibili:/metadata
    environment:
      BILI_SYNC_MEDIA_ROOT: /media
      BILI_SYNC_METADATA_ROOT: /metadata
      BILI_SYNC_MEDIA_INDEX_WEBHOOK_URL: http://media-index:8000/api/webhooks/bili-sync
      BILI_SYNC_MEDIA_INDEX_WEBHOOK_TOKEN: ${BILI_SYNC_MEDIA_INDEX_WEBHOOK_TOKEN}
```

视频源在 bili-sync 中仍填写 `/media/...`。例如媒体文件 `/media/earth/视频/BV1.mp4` 对应元数据 `/metadata/earth/视频/BV1.nfo`。MediaIndex 应以 `/媒体库` 为来源根、勾选 `/媒体库/08bilibili`，输出根为 `/strm`，这样它生成的 `/strm/08bilibili/earth/视频/BV1.strm` 才会与 NFO 同目录。Emby 扫描本地 STRM 目录。

Webhook 使用独立的 Bili-sync 接收入口及 Token，避免覆盖已有 MDC-NG 接口的固定扫描目录。下载成功后，bili-sync 发送 `POST`、`X-MediaIndex-Webhook` 请求头和 `{"event":"finished"}`；失败时在配置目录保存待通知标记，下次下载轮次重试。MediaIndex 返回 2xx 只表示接受任务，实际 STRM 生成结果仍应在 MediaIndex 任务中心核对。

### 现有文件

启用分离只影响后续处理；已经下载的视频旁的 NFO、图片等不会自动移动，数据库中已完成的任务也不会因切换目录而重跑。先备份配置数据库及两个目录，再选择少量新视频验证。若要迁移历史元数据，应按媒体根目录的相对路径复制到本地目录，确认 Emby 正常读取后再考虑清理网盘上的旧副文件。

在 bili-sync 管理页执行“清空并重置”或“全量同步并删除本地文件”时，程序会尝试删除对应视频目录和本地元数据目录；请在操作前检查目标路径。MediaIndex 自己记录的 STRM 映射需另行对账。
