pub mod file {

    use crate::commons::config::Configuration;
    use crate::image_provider::ImageProvider;
    use crate::routes::image::ImageProcessingError;
    use async_trait::async_trait;
    use tokio::fs::File;
    use tokio::io::AsyncReadExt;

    pub struct FileImageProvider {
        pub public_img_path: String,
    }

    impl FileImageProvider {
        pub async fn new(config: &Configuration) -> FileImageProvider {
            if let Some(pub_path) = config.public_img_path.clone() {
                Self {
                    public_img_path: pub_path,
                }
            } else {
                Self {
                    public_img_path: "".into(),
                }
            }
        }
    }

    #[async_trait]
    impl ImageProvider for FileImageProvider {
        async fn get_file(&self, resource: &str) -> Result<Vec<u8>, ImageProcessingError> {
            // 异步打开文件
            let mut file = File::open(format!("{}/{}", self.public_img_path, resource))
                .await
                .unwrap();

            // 创建一个缓冲区来存储文件内容
            let mut buffer = Vec::new();

            // 异步读取文件到缓冲区
            file.read_to_end(&mut buffer).await.unwrap();
            Ok(buffer)
        }
    }
}
