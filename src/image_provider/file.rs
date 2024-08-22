pub mod file {

    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use crate::commons::config::Configuration;
    use crate::image_provider::ImageProcessingError::{
        ClientReturnedErrorStatusCode, ImageDownloadFailed, ImageDownloadTimedOut,
        InvalidResourceUriProvided,
    };
    use crate::image_provider::ImageProvider;
    use crate::routes::image::ImageProcessingError;
    use async_trait::async_trait;
    use libvips::bindings::mkdir;
    use log::*;
    use reqwest::{Client, Url};
    use tokio::fs::File;
    use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};

    pub fn create_path_for_file(filepath: &str) -> () {
        // 将路径转换为 Path 对象
        let path = Path::new(filepath);

        // 使用 components 迭代路径部分并丢弃最后一个元素（文件名）
        let mut components = path.components().collect::<Vec<_>>();
        if let Some(_) = components.pop() {
            // 重新组合路径部分以获取目录路径
            let dir_path = components
                .iter()
                .map(|component| component.as_os_str())
                .collect::<PathBuf>();

            // 检查并创建目录
            if !dir_path.exists() {
                fs::create_dir_all(&dir_path).unwrap();
            }
        }
    }

    async fn read_file(path: &str) -> Result<Vec<u8>, ImageProcessingError> {
        // 异步打开文件
        let mut file = File::open(path).await.unwrap();

        // 创建一个缓冲区来存储文件内容
        let mut buffer = Vec::new();

        // 异步读取文件到缓冲区
        file.read_to_end(&mut buffer).await.unwrap();
        Ok(buffer)
    }

    pub struct FileImageProvider {
        pub public_img_path: String,
        pub client: Client,
    }

    impl FileImageProvider {
        pub async fn new(config: &Configuration) -> FileImageProvider {
            let reqwest_client = Client::builder()
                .timeout(Duration::from_millis(u64::from(
                    config.reqwest_timeout_millis.unwrap_or(2000),
                )))
                .connect_timeout(Duration::from_millis(u64::from(
                    config.reqwest_connection_timeout_millis.unwrap_or(2000),
                )))
                .pool_max_idle_per_host(usize::from(
                    config.reqwest_pool_max_idle_per_host.unwrap_or(10),
                ))
                .pool_idle_timeout(Duration::from_millis(u64::from(
                    config.reqwest_pool_idle_timeout_millis.unwrap_or(60000),
                )))
                .build()
                .unwrap();
            if let Some(pub_path) = config.public_img_path.clone() {
                Self {
                    public_img_path: pub_path,
                    client: reqwest_client,
                }
            } else {
                Self {
                    public_img_path: "".into(),
                    client: reqwest_client,
                }
            }
        }
    }

    #[async_trait]
    impl ImageProvider for FileImageProvider {
        async fn get_file(&self, resource: &str) -> Result<Vec<u8>, ImageProcessingError> {
            if resource.starts_with("http://") || resource.starts_with("https://") {
                let url = Url::parse(resource).map_err(|_| {
                    error!(
                        "the provided resource uri is not a valid http url: '{}'",
                        resource
                    );
                    InvalidResourceUriProvided(String::from(resource))
                })?;
                let filepathstr = format!("{}{}", self.public_img_path, url.clone().path());
                let filepath = Path::new(filepathstr.as_str());
                if !url.path().is_empty() && filepath.exists() {
                    println!("file exists: {}", filepathstr);
                    return read_file(filepathstr.as_str()).await;
                }
                let response = self.client.get(url.clone()).send().await.map_err(|e| {
                    if e.is_timeout() {
                        error!(
                            "request for downloading the image '{}' timed out. error: {}",
                            resource, e
                        );
                        ImageDownloadTimedOut
                    } else {
                        error!("error downloading the image: '{}'. error: {}", resource, e);
                        ImageDownloadFailed
                    }
                })?;

                let status = response.status();
                if status.is_success() {
                    let bytes = response.bytes().await.map_err(|e| {
                        error!(
                            "failed to read the binary payload of the image '{}'. error: {}",
                            resource, e
                        );
                        ImageDownloadFailed
                    })?;
                    create_path_for_file(filepathstr.as_str());
                    let file = File::create(format!("{}{}", self.public_img_path, url.path()))
                        .await
                        .unwrap();
                    let mut writer = BufWriter::new(file);
                    let bytes_vec = bytes.to_vec();
                    writer.write_all(&bytes_vec.as_slice()).await.unwrap();
                    writer.flush().await.unwrap();
                    Ok(bytes_vec)
                } else if status.is_client_error() {
                    error!(
                        "the requested image '{}' couldn't be downloaded. received status code: {}",
                        resource, status
                    );
                    Err(ClientReturnedErrorStatusCode(
                        status.as_u16(),
                        String::from(resource),
                    ))
                } else {
                    error!(
                        "failed to download the specified resource. received status code: {}",
                        status.as_str()
                    );
                    Err(ImageDownloadFailed)
                }
            } else {
                read_file(format!("{}/{}", self.public_img_path, resource).as_str()).await
            }
        }
    }
}
