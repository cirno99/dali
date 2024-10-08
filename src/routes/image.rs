use async_trait::async_trait;
use axum::{
    body::Body,
    extract::{FromRequest, Request, State},
    http::{self, HeaderValue, Response, StatusCode},
    response::IntoResponse,
};
use futures::future::join_all;
use log::{error, warn};
use reqwest::{
    header::{CONTENT_TYPE, LAST_MODIFIED},
    Url,
};
use serde::de::DeserializeOwned;
use serde_json::json;
use std::{error::Error, path::PathBuf};
use std::{path::Path, time::SystemTime};
use thiserror::Error;
use tokio::fs;

use crate::{
    commons::{ImageFormat, ProcessImageRequest},
    image_processor, AppState,
};

use super::metric::{FETCH_DURATION, INPUT_SIZE, OUTPUT_SIZE};

pub struct ProcessImageRequestExtractor<T> {
    pub params: T,
    pub if_modified: Option<String>,
}

#[async_trait]
impl<B, T> FromRequest<B> for ProcessImageRequestExtractor<T>
where
    B: Send,
    T: DeserializeOwned + Send,
{
    type Rejection = (StatusCode, String);

    async fn from_request(req: Request, _state: &B) -> Result<Self, Self::Rejection> {
        let query = req.uri().query();
        let if_modified = req
            .headers()
            .get(http::header::IF_MODIFIED_SINCE)
            .map(|m| m.to_str().unwrap().to_owned());
        if let Some(query) = query {
            let extracted_params = serde_qs::from_str(query);
            if extracted_params.is_ok() {
                Ok(Self {
                    params: extracted_params.unwrap(),
                    if_modified,
                })
            } else {
                Err((
                    StatusCode::BAD_REQUEST,
                    "the provided parameters within the query string aren't valid".to_string(),
                ))
            }
        } else {
            Err((
                StatusCode::BAD_REQUEST,
                "the provided parameters within the query string aren't valid".to_string(),
            ))
        }
    }
}

#[derive(Error, Debug)]
pub enum ImageProcessingError {
    #[error("the provded resource uri is not valid: `{0}`")]
    InvalidResourceUriProvided(String),
    #[error("the download of the image timed out")]
    ImageDownloadTimedOut,
    #[error("received error response `{0}` while attempting to download the image `{1}`")]
    ClientReturnedErrorStatusCode(u16, String),
    #[error("the download of the image has failed")]
    ImageDownloadFailed,
    #[error("failed to join the thread that was doing the processing")]
    ProcessingWorkerJoinError,
    #[error("the image processing with libvips has failed")]
    LibvipsProcessingFailed(libvips::error::Error),
    #[error("the image processing with libvips has failed")]
    AxumHttpError(#[from] axum::http::Error),
}

impl IntoResponse for ImageProcessingError {
    fn into_response(self) -> axum::response::Response {
        error!(
            "failed to download the image that requires processing. error: {}",
            self
        );

        let (status, message) = match self {
            ImageProcessingError::ClientReturnedErrorStatusCode(status, resource) => (
                StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_REQUEST),
                format!("Received status code '{}' while attemtping to download the image that has to be processed: '{}'", status, resource),
            ),
            ImageProcessingError::LibvipsProcessingFailed(libvips::error::Error::InitializationError(_)) => (
                StatusCode::BAD_REQUEST,
                String::from("The image that was requested to be processed cannot be opened."),
            ),
            ImageProcessingError::ImageDownloadTimedOut => (
                StatusCode::BAD_REQUEST,
                String::from("Downloading the image requested to be processed timed out."),
            ),
            ImageProcessingError::InvalidResourceUriProvided(resource_uri) => (
                StatusCode::BAD_REQUEST,
                format!("The provided resource URI is not valid: '{}'", resource_uri)
            ),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                String::from("Something went wrong on our side."),
            ),
        };
        let body = json!({ "error": message }).to_string();
        Response::builder()
            .status(status)
            .header("Content-Type", "application/json")
            .body(body.into())
            .unwrap()
    }
}

async fn get_metadata(real_filepath: &str) -> Result<HeaderValue, Box<dyn Error>> {
    let metadata = fs::metadata(PathBuf::from(real_filepath))
        .await
        .expect("failed to read file metadata");
    let last_modified = metadata.modified()?; // 获取文件最后修改时间
    let last_modified_header =
        http::HeaderValue::from_str(httpdate::fmt_http_date(last_modified).as_str())?;
    Ok(last_modified_header)
}

pub async fn process_image(
    State(AppState {
        vips_app,
        image_provider,
        public_img_path,
    }): State<AppState>,
    ProcessImageRequestExtractor {
        mut params,
        if_modified,
    }: ProcessImageRequestExtractor<ProcessImageRequest>,
) -> Result<Response<Body>, ImageProcessingError> {
    let real_filepath: String;
    if params.image_address.starts_with("http://") || params.image_address.starts_with("https://") {
        let url = Url::parse(&params.image_address)
            .map_err(|_| {
                error!(
                    "the provided resource uri is not a valid http url: '{}'",
                    &params.image_address
                );
            })
            .unwrap();
        let filepathstr = format!("{}{}", public_img_path, url.clone().path());
        let filepath = Path::new(filepathstr.as_str());
        real_filepath = filepath.to_str().unwrap().to_owned();
    } else {
        real_filepath = format!("{}/{}", public_img_path, params.image_address)
            .as_str()
            .to_string();
    }
    if params.image_address.ends_with("400X400.jpg") {
        params.quality = 68;
    }

    let filepath = Path::new(real_filepath.as_str());
    let now = SystemTime::now();

    if filepath.exists() {
        let last_modified_header = get_metadata(real_filepath.as_str()).await.unwrap();

        // 检查 If-Modified-Since 请求头
        if let Some(if_modified_since) = if_modified {
            if if_modified_since == last_modified_header {
                return Ok(Response::builder()
                    .status(StatusCode::NOT_MODIFIED)
                    .body(Body::empty())?);
            }
        }
    }

    let main_img = image_provider.get_file(&params.image_address).await?;

    let last_modified_header = get_metadata(real_filepath.as_str()).await.unwrap();
    let mut total_input_size = main_img.len();

    let mut watermarks = vec![];
    if !params.watermarks.is_empty() {
        let watermarks_futures = params
            .watermarks
            .iter()
            .map(|wm| image_provider.get_file(&wm.image_address));
        watermarks = join_all(watermarks_futures)
            .await
            .into_iter()
            .filter(|r| {
                if r.is_err() {
                    warn!(
                        "failed to download watermark with error {}",
                        r.as_ref().err().unwrap()
                    );
                }
                r.is_ok()
            })
            .map(|r| {
                let watermark = r.unwrap();
                total_input_size += watermark.len();
                watermark
            })
            .collect();
    }

    if let Ok(elapsed) = now.elapsed() {
        let duration =
            (elapsed.as_secs() as f64) + f64::from(elapsed.subsec_nanos()) / 1_000_000_000_f64;
        FETCH_DURATION.success.observe(duration);
    }

    let format = params.format;

    // processing the image is a blocking operation and originally I've use the tokio::spawn_blocking option to process the image.
    // it was decently performing, but I've benchmarked rayon as well and the performance improved a lot in terms of
    // response time and memory used
    let (send, recv) = tokio::sync::oneshot::channel();
    rayon::spawn(move || {
        let image = image_processor::process_image(main_img, watermarks, params);
        let _ = send.send(image);
    });
    let processed_image = recv.await.map_err(|e| {
        error!(
            "failed to join the thread which process the image. error: {}",
            e
        );
        ImageProcessingError::ProcessingWorkerJoinError
    })?
    .map_err(|e| {
        error!(
            "the image processing has failed for the resource with the error: {}. libvips raw error is: {}",
            e, vips_app.error_buffer().unwrap_or("").replace("\n", ". ")
        );
        ImageProcessingError::LibvipsProcessingFailed(e)
    })?;

    // log_size_metrics(&format, total_input_size, processed_image.len());
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, format!("image/{}", format))
        .header(LAST_MODIFIED, last_modified_header)
        .body(Body::from(Into::<Vec<u8>>::into(processed_image)))?)
}

fn log_size_metrics(format: &ImageFormat, input_size: usize, response_length: usize) {
    match format {
        ImageFormat::Jpeg => {
            INPUT_SIZE.jpeg.observe(input_size as f64);
            OUTPUT_SIZE.jpeg.observe(response_length as f64);
        }
        ImageFormat::Heic => {
            INPUT_SIZE.heic.observe(input_size as f64);
            OUTPUT_SIZE.heic.observe(response_length as f64);
        }
        ImageFormat::Webp => {
            INPUT_SIZE.webp.observe(input_size as f64);
            OUTPUT_SIZE.webp.observe(response_length as f64);
        }
        ImageFormat::Png => {
            INPUT_SIZE.png.observe(input_size as f64);
            OUTPUT_SIZE.png.observe(response_length as f64);
        }
    }
}
