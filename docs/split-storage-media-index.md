# 视频与元数据分离（可选）

此模式让视频留在 CD2 挂载的网盘目录，把 NFO、海报、fanart、弹幕和字幕写到本地。目录和文件名的相对结构保持一致；订阅和数据库继续使用原有的 `/media` 路径。未设置下述环境变量时，行为与上游版本相同。

## Docker 挂载与环境变量

下面的 `08bilibili` 只是示例。它必须与你在 MediaIndex 中选择的 115 网盘子目录名称完全一致。

```yaml
services:
  bili-sync-rs:
    image: ghcr.io/xudong7587/bili-sync:dev
    volumes:
      - /path/to/bili-sync-config:/app/.config/bili-sync
      - /path/to/local/strm/08bilibili:/media
      - /path/to/cd2/媒体库/08bilibili:/video
    environment:
      BILI_SYNC_METADATA_ROOT: /media
      BILI_SYNC_VIDEO_ROOT: /video
```

视频源在 bili-sync 中仍填写 `/media/...`，不用修改旧订阅。例如数据库中的 `/media/earth/视频/BV1.mp4` 对应实际视频 `/video/earth/视频/BV1.mp4` 和本地元数据 `/media/earth/视频/BV1.nfo`。MediaIndex 应以 `/媒体库` 为来源根、勾选 `/媒体库/08bilibili`，输出根为本地 STRM 目录，这样生成的 STRM 才会与 NFO 同目录。Emby 扫描本地 STRM 目录。

在 bili-sync 的「设置 → 通知设置 → MediaIndex 入库通知」中粘贴 MediaIndex 入站连接的 Webhook 地址和令牌，保存后立即生效，无需修改 compose。下载成功后，bili-sync 发送 `POST`、`Authorization: Bearer <token>` 请求头和 `{"event":"finished"}`；失败时在配置目录保存待通知标记，下次下载轮次重试。MediaIndex 返回 2xx 只表示接受任务，实际 STRM 生成结果仍应在 MediaIndex 任务中心核对。

### 现有文件

启用分离只影响后续处理；已经下载的视频旁的 NFO、图片等不会自动移动，数据库中已完成的任务也不会因切换目录而重跑。先备份配置数据库及两个目录，再选择少量新视频验证。切换旧版实例时，应按原网盘媒体根目录的相对路径把历史 NFO、图片等复制到本地 `/media`，不移动视频，并保留旧实例的 `/upper` 映射。确认 Emby 正常读取后再考虑清理网盘上的旧副文件。

在 bili-sync 管理页执行“清空并重置”或“全量同步并删除本地文件”时，程序会尝试删除对应视频目录和本地元数据目录；请在操作前检查目标路径。MediaIndex 自己记录的 STRM 映射需另行对账。
