# 视频与元数据分离（可选）

此模式把 NFO、海报、fanart、弹幕和字幕写到本地 `/media`，MP4 则可写入 `/video` 挂载目录，或通过 CloudDrive2 API 直接上传到 115。两侧的相对目录和文件名保持一致；订阅和数据库继续使用原有的 `/media` 路径。

## Docker 挂载与环境变量

仓库根目录提供带注释的通用 [docker-compose.yaml](../docker-compose.yaml)。先将其中的 `/path/to/...` 宿主机路径改为实际目录。使用 MediaIndex 时，网盘中的目录名称应与 MediaIndex 选择的目录一致。

```yaml
services:
  bili-sync-rs:
    image: ghcr.io/xudong7587/bili-sync:bili115-2026.09.26.3
    volumes:
      - /path/to/bili-sync-config:/app/.config/bili-sync
      - /path/to/local-metadata:/media
      - /path/to/video-storage:/video
    environment:
      BILI_SYNC_METADATA_ROOT: /media
      BILI_SYNC_VIDEO_ROOT: /video
```

视频源在 bili-sync 中仍填写 `/media/...`，不用修改旧订阅。例如数据库中的 `/media/earth/视频/BV1.mp4` 对应实际视频 `/video/earth/视频/BV1.mp4` 和本地元数据 `/media/earth/视频/BV1.nfo`。使用 MediaIndex 时，将其扫描范围指向实际的视频存储目录，并让 STRM 输出到本地元数据目录中相应的位置，这样 STRM 才会与 NFO 同目录。Emby 扫描本地元数据目录。

在 bili-sync 的「设置 → 通知设置 → MediaIndex 入库通知」中粘贴 MediaIndex 入站连接的 Webhook 地址和令牌，保存后立即生效，无需修改 compose。下载成功后，bili-sync 发送 `POST`、`Authorization: Bearer <token>` 请求头和 `{"event":"finished"}`；失败时在配置目录保存待通知标记，下次下载轮次重试。MediaIndex 返回 2xx 只表示接受任务，实际 STRM 生成结果仍应在 MediaIndex 任务中心核对。

### CD2 API 直传 115

保存路径相对于 API 令牌允许访问的根目录。如果令牌已经限定到 `/115open`，应填写 `/媒体库/08Bilibili`，不要重复添加 `/115open` 前缀。

在「设置 → 通知设置 → CloudDrive2 直传 115」填写 CD2 地址、API 令牌和 CD2 内的保存根目录，例如 `/115/媒体库/08Bilibili`。三个字段必须同时填写；全部留空则继续使用 `/video` 挂载写入。直传时，bili-sync 先在容器临时目录下载、合并 MP4，再通过 CD2 API 分块写入 115，并等待 CD2 上传任务变为完成；确认文件大小后才更新分页下载状态和发送 MediaIndex 通知。CD2 上传失败或超时会保留未完成状态，供下轮重试。请给容器临时目录留出足够空间容纳正在处理的视频。

例如，本地 `/media/basketball/视频/BV1.nfo` 对应 115 中 `/115/媒体库/08Bilibili/basketball/视频/BV1.mp4`。MediaIndex 的 115 扫描根目录应指向相同的网盘位置，STRM 输出根目录对应本地 `/media`。CD2 直传不需要写入 `/video`，但旧 compose 的映射可以保留以便平滑切换。不要同时运行两个实例处理同一份订阅数据库。

### 现有文件

启用分离只影响后续处理；已经下载的视频旁的 NFO、图片等不会自动移动，数据库中已完成的任务也不会因切换目录而重跑。先备份配置数据库及两个目录，再选择少量新视频验证。切换旧版实例时，应按原网盘媒体根目录的相对路径把历史 NFO、图片等复制到本地 `/media`，不移动视频，并保留旧实例的 `/upper` 映射。确认 Emby 正常读取后再考虑清理网盘上的旧副文件。

CD2 可能在完成后立即移除上传任务。此时程序会强制刷新云端目录，确认文件拥有云端 ID、大小和 SHA1 与本地视频一致，且没有同路径上传任务，再判定完成。重试遇到同名文件也使用这项校验；校验通过则复用，否则报错并保留原文件。

在挂载写入模式执行“清空并重置”或“全量同步并删除本地文件”时，程序会尝试删除对应视频目录和本地元数据目录。CD2 直传模式下不会通过旧 `/video` 挂载删除云端视频；重置后按照上述校验决定能否复用云端文件。MediaIndex 自己记录的 STRM 映射需另行对账。
