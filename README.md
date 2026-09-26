# bili-sync：视频与元数据分离版

这是基于 [amtoaer/bili-sync](https://github.com/amtoaer/bili-sync) 的个人分支，保留原有的收藏夹订阅、追更和管理页面，并增加适合 NAS 的分离存储方式。

## 这个分支增加了什么

- **视频单独存储**：支持原有的 `/video` 分离挂载，也支持通过 CloudDrive2 API 将新 MP4 直接上传到 115；直传模式会等待 CD2 上传任务完成。
- **元数据留在本地**：NFO、海报、字幕、弹幕等写入 `/media`，与视频保持相同的相对目录结构，供 Emby 读取。
- **兼容原配置**：视频源路径仍使用 `/media/...`；复用原有配置目录后，`config.toml`、数据库和订阅设置无需重建。启用分离不会自动搬迁以前下载的文件。
- **可联动 [MediaIndex](https://github.com/xudong7587/media-index)**：下载完成后发送入库通知，由 MediaIndex 扫描网盘目录、生成 STRM，并刷新 Emby。Webhook 地址和令牌在 bili-sync 管理页的「设置 → 通知设置 → MediaIndex 入库通知」中填写。
- **CD2 直传设置**：在「设置 → 通知设置 → CloudDrive2 直传 115」填写 CD2 地址、API 令牌和 CD2 内的 115 保存根目录。三个字段同时填写后启用；原有媒体源和数据库路径仍是 `/media/...`。

## Docker Compose

仓库根目录提供带注释的通用示例 [docker-compose.yaml](./docker-compose.yaml)。部署前将其中三个 `/path/to/...` 宿主机路径替换成自己的目录，并检查端口、镜像标签和可选的用户 ID。升级已有实例时，务必将原配置目录继续映射到 `/app/.config/bili-sync`：

| 容器路径 | 用途 |
| --- | --- |
| `/app/.config/bili-sync` | 原有配置和数据库 |
| `/media` | 本地元数据目录 |
| `/video` | 仅原有挂载写入方式使用；CD2 直传时不会写入这里 |
| 原配置中的 UP 主头像路径 | 如原实例单独映射过，请沿用原映射 |

```bash
docker compose -f docker-compose.yaml up -d
```

现有视频源和追更路径不需要改为 `/video`。首次切换前请备份配置与元数据；已有视频对应的 NFO、图片等需要自行复制到新的本地 `/media` 目录。详细说明见 [视频与元数据分离](./docs/split-storage-media-index.md)。

上游使用说明和其他功能请参阅 [bili-sync 文档](https://bili-sync.amto.cc/)。本分支沿用上游 [License](./License)。
