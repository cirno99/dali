use async_trait::async_trait;
use file::file::FileImageProvider;

use crate::{commons::config::Configuration, routes::image::ImageProcessingError};
pub mod file;

#[async_trait]
pub trait ImageProvider: Send + Sync {
    async fn get_file(&self, resource: &str) -> Result<Vec<u8>, ImageProcessingError>;
}

#[allow(unreachable_code)]
pub async fn create_image_provider(config: &Configuration) -> Box<dyn ImageProvider> {
    // #[cfg(feature = "reqwest")]
    // {
    //     return Box::new(ReqwestImageProvider::new(config).await);
    // }
    return Box::new(FileImageProvider::new(config).await);
}
