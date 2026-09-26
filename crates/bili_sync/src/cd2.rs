//! Upload MP4 data through CloudDrive2's gRPC-Web API. Closing a CD2 file only
//! queues its cloud transfer, so a page succeeds only after its upload task finishes.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use prost::Message;
use reqwest::{Client, Url};
use tokio::fs::File;
use tokio::io::AsyncReadExt;

use crate::config::Config;

const CHUNK_SIZE: usize = 2 * 1024 * 1024;
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(6 * 60 * 60);
const POLL_INTERVAL: Duration = Duration::from_secs(5);

pub struct Cd2Client {
    http: Client,
    url: Url,
    token: String,
    save_path: String,
    metadata_root: PathBuf,
}

impl Cd2Client {
    pub fn configured(config: &Config) -> Result<Option<Self>> {
        let url = config.cd2_url.trim();
        let token = config.cd2_token.trim();
        let save_path = config.cd2_save_path.trim();
        if url.is_empty() && token.is_empty() && save_path.is_empty() {
            return Ok(None);
        }
        ensure!(!url.is_empty() && !token.is_empty() && !save_path.is_empty(), "CD2 地址、令牌和保存路径必须同时填写");
        let url = Url::parse(url).context("CD2 地址无效")?;
        ensure!(matches!(url.scheme(), "http" | "https") && url.host_str().is_some(), "CD2 地址必须是 HTTP(S) URL");
        ensure!(url.username().is_empty() && url.password().is_none() && url.query().is_none() && url.fragment().is_none(), "CD2 地址不能包含凭据、查询或片段");
        ensure!(!token.contains('\r') && !token.contains('\n'), "CD2 令牌包含无效字符");
        validate_remote_path(save_path)?;
        let metadata_root = std::env::var_os("BILI_SYNC_METADATA_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/media"));
        ensure!(metadata_root.is_absolute(), "BILI_SYNC_METADATA_ROOT 必须是绝对路径");
        let metadata_root = dunce::canonicalize(metadata_root).context("无法访问元数据目录")?;
        let http = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .timeout(Duration::from_secs(120))
            .build()?;
        Ok(Some(Self {
            http,
            url,
            token: token.to_owned(),
            save_path: save_path.trim_end_matches('/').to_owned(),
            metadata_root,
        }))
    }

    pub fn remote_path(&self, metadata_file: &Path) -> Result<String> {
        let relative = metadata_file.strip_prefix(&self.metadata_root).context("视频路径不在元数据根目录下")?;
        ensure!(!relative.as_os_str().is_empty(), "视频路径不能为空");
        let mut output = self.save_path.clone();
        for component in relative.components() {
            match component {
                Component::Normal(part) => {
                    let part = part.to_str().context("CD2 路径必须是 UTF-8")?;
                    ensure!(!part.contains('/') && !part.contains('\\'), "CD2 路径包含无效字符");
                    output.push('/');
                    output.push_str(part);
                }
                _ => bail!("视频路径包含无效目录分隔符"),
            }
        }
        Ok(output)
    }

    pub async fn upload(&self, local_file: &Path, metadata_file: &Path) -> Result<()> {
        let remote_file = self.remote_path(metadata_file)?;
        let (remote_dir, file_name) = remote_file.rsplit_once('/').context("CD2 目标路径无效")?;
        self.ensure_directory(remote_dir).await?;
        let files = self.list_directory(remote_dir, true).await?;
        ensure!(!files.iter().any(|item| item.name == file_name), "CD2 目标文件已存在：{remote_file}；请先确认云端状态，避免覆盖");
        let size = tokio::fs::metadata(local_file).await?.len();
        ensure!(size > 0, "拒绝上传空视频");
        let previous_keys = self.upload_keys(&remote_file).await?;
        let create: CreateFileResult = self.call_one("CreateFile", &CreateFileRequest {
            parent_path: remote_dir.to_owned(),
            file_name: file_name.to_owned(),
        }).await?;
        ensure!(create.file_handle != 0, "CD2 未返回文件句柄");
        let transfer = self.write_file(local_file, create.file_handle, size).await;
        let close: Result<FileOperationResult> = self.call_one("CloseFile", &CloseFileRequest { file_handle: create.file_handle }).await;
        transfer?;
        let close = close?;
        ensure!(close.success, "CD2 关闭文件失败：{}", close.error_message);
        self.wait_for_upload(&remote_file, size, &previous_keys).await
    }

    async fn write_file(&self, local_file: &Path, file_handle: u64, size: u64) -> Result<()> {
        let mut file = File::open(local_file).await?;
        let mut buffer = vec![0; CHUNK_SIZE];
        let mut offset = 0;
        loop {
            let length = file.read(&mut buffer).await?;
            if length == 0 { break; }
            let result: WriteFileResult = self.call_one("WriteToFile", &WriteFileRequest {
                file_handle,
                start_pos: offset,
                length: length as u64,
                buffer: buffer[..length].to_vec(),
                close_file: false,
            }).await?;
            ensure!(result.bytes_written == length as u64, "CD2 写入字节数不足：{}/{}", result.bytes_written, length);
            offset += length as u64;
        }
        ensure!(offset == size, "本地视频上传期间大小改变");
        Ok(())
    }

    async fn ensure_directory(&self, path: &str) -> Result<()> {
        let mut parent = String::new();
        for name in path.split('/').filter(|part| !part.is_empty()) {
            let current = format!("{parent}/{name}");
            let items = self.list_directory(if parent.is_empty() { "/" } else { &parent }, false).await?;
            if let Some(item) = items.iter().find(|item| item.name == name) {
                ensure!(item.is_directory || item.file_type == 0, "CD2 路径不是目录：{current}");
            } else {
                let created: CreateFolderResult = self.call_one("CreateFolder", &CreateFolderRequest {
                    parent_path: if parent.is_empty() { "/".to_owned() } else { parent.clone() },
                    folder_name: name.to_owned(),
                }).await?;
                let success = created.result.is_some_and(|result| result.success) || created.folder_created.is_some();
                ensure!(success, "CD2 创建目录失败：{current}");
            }
            parent = current;
        }
        Ok(())
    }

    async fn list_directory(&self, path: &str, force_refresh: bool) -> Result<Vec<CloudDriveFile>> {
        let replies: Vec<SubFilesReply> = self.call_stream("GetSubFiles", &ListSubFileRequest {
            path: path.to_owned(),
            force_refresh,
        }).await?;
        Ok(replies.into_iter().flat_map(|reply| reply.sub_files).collect())
    }

    async fn upload_keys(&self, remote_file: &str) -> Result<HashSet<String>> {
        Ok(self.upload_list().await?.upload_files.into_iter()
            .filter(|item| item.dest_path == remote_file)
            .map(|item| item.key)
            .collect())
    }

    async fn upload_list(&self) -> Result<GetUploadFileListResult> {
        self.call_one("GetUploadFileList", &GetUploadFileListRequest { get_all: true }).await
    }

    async fn wait_for_upload(&self, remote_file: &str, size: u64, previous_keys: &HashSet<String>) -> Result<()> {
        let deadline = Instant::now() + UPLOAD_TIMEOUT;
        loop {
            let tasks = self.upload_list().await?;
            for task in tasks.upload_files.iter().filter(|task| task.dest_path == remote_file && !previous_keys.contains(&task.key)) {
                match task.status_enum {
                    5 => {
                        ensure!(task.size == size && task.transfered_bytes == size, "CD2 完成任务的文件大小不符：{remote_file}");
                        let files = self.list_directory(remote_file.rsplit_once('/').unwrap().0, true).await?;
                        ensure!(files.iter().any(|file| file.name == remote_file.rsplit_once('/').unwrap().1 && file.size == size as i64), "CD2 上传完成但目录中未找到对应视频：{remote_file}");
                        return Ok(());
                    }
                    2 | 6 | 8 | 9 | 10 => bail!("CD2 上传未成功：{remote_file}，状态 {}，{}", task.status_enum, task.error_message),
                    _ => {}
                }
            }
            ensure!(Instant::now() < deadline, "等待 CD2 上传完成超时：{remote_file}");
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    async fn call_one<Req: Message, Res: Message + Default>(&self, method: &str, request: &Req) -> Result<Res> {
        let mut responses = self.call_stream(method, request).await?;
        ensure!(responses.len() == 1, "CD2 {method} 返回了异常的响应数量");
        Ok(responses.remove(0))
    }

    async fn call_stream<Req: Message, Res: Message + Default>(&self, method: &str, request: &Req) -> Result<Vec<Res>> {
        let endpoint = self.url.join(&format!("clouddrive.CloudDriveFileSrv/{method}"))?;
        let encoded = request.encode_to_vec();
        let mut body = Vec::with_capacity(encoded.len() + 5);
        body.push(0);
        body.extend_from_slice(&(encoded.len() as u32).to_be_bytes());
        body.extend_from_slice(&encoded);
        let response = self.http.post(endpoint)
            .header("content-type", "application/grpc-web+proto")
            .header("accept", "application/grpc-web+proto")
            .header("x-grpc-web", "1")
            .bearer_auth(&self.token)
            .body(body)
            .send().await.context("CD2 API 请求失败")?;
        ensure!(response.status().is_success(), "CD2 {method} HTTP 状态 {}", response.status());
        let bytes = response.bytes().await?;
        ensure!(bytes.len() <= 32 * 1024 * 1024, "CD2 {method} 响应过大");
        decode_frames::<Res>(&bytes)
    }
}

pub fn validate(config: &Config) -> Result<()> {
    Cd2Client::configured(config).map(|_| ())
}

fn validate_remote_path(path: &str) -> Result<()> {
    ensure!(path.starts_with('/') && path != "/" && !path.ends_with('/'), "CD2 保存路径必须是绝对目录，且不能是根目录或以 / 结尾");
    ensure!(path.split('/').skip(1).all(|part| !part.is_empty() && part != "." && part != ".." && !part.contains('\\')), "CD2 保存路径包含无效目录名");
    Ok(())
}

fn decode_frames<T: Message + Default>(bytes: &[u8]) -> Result<Vec<T>> {
    let mut offset = 0;
    let mut values = Vec::new();
    let mut trailer = None;
    while offset + 5 <= bytes.len() {
        let flag = bytes[offset];
        let length = u32::from_be_bytes(bytes[offset + 1..offset + 5].try_into()?) as usize;
        offset += 5;
        ensure!(length <= bytes.len() - offset, "CD2 gRPC-Web 响应帧不完整");
        let payload = &bytes[offset..offset + length];
        offset += length;
        if flag & 0x80 != 0 {
            trailer = Some(String::from_utf8_lossy(payload).to_string());
        } else {
            ensure!(flag == 0, "CD2 返回了不支持的压缩响应");
            values.push(T::decode(payload)?);
        }
    }
    ensure!(offset == bytes.len(), "CD2 gRPC-Web 响应末尾不完整");
    let trailer = trailer.context("CD2 gRPC-Web 响应缺少状态")?;
    ensure!(trailer.lines().any(|line| line.trim().eq_ignore_ascii_case("grpc-status: 0")), "CD2 API 返回错误：{trailer}");
    Ok(values)
}

#[derive(Clone, PartialEq, Message)]
struct ListSubFileRequest {
    #[prost(string, tag = "1")]
    path: String,
    #[prost(bool, tag = "2")]
    force_refresh: bool,
}
#[derive(Clone, PartialEq, Message)]
struct SubFilesReply { #[prost(message, repeated, tag = "1")] sub_files: Vec<CloudDriveFile> }
#[derive(Clone, PartialEq, Message)]
struct CloudDriveFile {
    #[prost(string, tag = "2")] name: String,
    #[prost(int64, tag = "4")] size: i64,
    #[prost(int32, tag = "5")] file_type: i32,
    #[prost(bool, tag = "30")] is_directory: bool,
}
#[derive(Clone, PartialEq, Message)]
struct CreateFolderRequest {
    #[prost(string, tag = "1")] parent_path: String,
    #[prost(string, tag = "2")] folder_name: String,
}
#[derive(Clone, PartialEq, Message)]
struct CreateFolderResult {
    #[prost(message, optional, tag = "1")] folder_created: Option<CloudDriveFile>,
    #[prost(message, optional, tag = "2")] result: Option<FileOperationResult>,
}
#[derive(Clone, PartialEq, Message)]
struct FileOperationResult {
    #[prost(bool, tag = "1")] success: bool,
    #[prost(string, tag = "2")] error_message: String,
}
#[derive(Clone, PartialEq, Message)]
struct CreateFileRequest {
    #[prost(string, tag = "1")] parent_path: String,
    #[prost(string, tag = "2")] file_name: String,
}
#[derive(Clone, PartialEq, Message)]
struct CreateFileResult { #[prost(uint64, tag = "1")] file_handle: u64 }
#[derive(Clone, PartialEq, Message)]
struct WriteFileRequest {
    #[prost(uint64, tag = "1")] file_handle: u64,
    #[prost(uint64, tag = "2")] start_pos: u64,
    #[prost(uint64, tag = "3")] length: u64,
    #[prost(bytes, tag = "4")] buffer: Vec<u8>,
    #[prost(bool, tag = "5")] close_file: bool,
}
#[derive(Clone, PartialEq, Message)]
struct WriteFileResult { #[prost(uint64, tag = "1")] bytes_written: u64 }
#[derive(Clone, PartialEq, Message)]
struct CloseFileRequest { #[prost(uint64, tag = "1")] file_handle: u64 }
#[derive(Clone, PartialEq, Message)]
struct GetUploadFileListRequest { #[prost(bool, tag = "1")] get_all: bool }
#[derive(Clone, PartialEq, Message)]
struct GetUploadFileListResult { #[prost(message, repeated, tag = "2")] upload_files: Vec<UploadFileInfo> }
#[derive(Clone, PartialEq, Message)]
struct UploadFileInfo {
    #[prost(string, tag = "1")] key: String,
    #[prost(string, tag = "2")] dest_path: String,
    #[prost(uint64, tag = "3")] size: u64,
    #[prost(uint64, tag = "4")] transfered_bytes: u64,
    #[prost(string, tag = "6")] error_message: String,
    #[prost(int32, tag = "8")] status_enum: i32,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unsafe_cloud_paths() {
        for path in ["/", "relative", "/115/../video", "/115//video", "/115/video/"] {
            assert!(validate_remote_path(path).is_err(), "{path}");
        }
        assert!(validate_remote_path("/115/媒体库/08Bilibili").is_ok());
    }
    #[test]
    fn requires_success_trailer() {
        let mut frame = vec![0x80, 0, 0, 0, 16];
        frame.extend_from_slice(b"grpc-status: 0\r\n");
        assert!(decode_frames::<CreateFileResult>(&frame).unwrap().is_empty());
        assert!(decode_frames::<CreateFileResult>(&[]).is_err());
    }

    #[test]
    fn preserves_relative_metadata_directory() {
        let client = Cd2Client {
            http: Client::new(),
            url: Url::parse("https://cd2.example.test/").unwrap(),
            token: "test".into(),
            save_path: "/115/媒体库/08Bilibili".into(),
            metadata_root: PathBuf::from("/media"),
        };
        assert_eq!(
            client.remote_path(Path::new("/media/basketball/合集/Season 1/BV1.mp4")).unwrap(),
            "/115/媒体库/08Bilibili/basketball/合集/Season 1/BV1.mp4"
        );
        assert!(client.remote_path(Path::new("/other/BV1.mp4")).is_err());
    }

    #[test]
    fn old_config_without_cd2_fields_stays_disabled() {
        let mut saved = serde_json::to_value(Config::default()).unwrap();
        let fields = saved.as_object_mut().unwrap();
        fields.remove("cd2_url");
        fields.remove("cd2_token");
        fields.remove("cd2_save_path");
        let loaded: Config = serde_json::from_value(saved).unwrap();
        assert!(Cd2Client::configured(&loaded).unwrap().is_none());
    }
}
