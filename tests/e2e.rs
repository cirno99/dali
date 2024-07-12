// (c) Copyright 2019-2024 OLX

use libvips::{
    ops::{
        gravity, gravity_with_opts, resize, smartcrop, smartcrop_with_opts, thumbnail,
        GravityOptions, SmartcropOptions,
    },
    VipsImage,
};
use tokio::{fs::File, io::AsyncReadExt};

#[macro_use]
extern crate lazy_static;
mod utils;

#[tokio::test]
async fn test_get_simple() {
    let result = utils::make_request(utils::RequestParametersBuilder::new("img-test"))
        .await
        .expect("Unable to download file");
    utils::assert_result(&result[..], "raw.jpg");
}

#[tokio::test]
async fn test_gravity() {
    use libvips::ops::{CompassDirection, Extend};
    let resource = "files/2325/1146135/1221394/1215dd5adc2f0c571a3327af35c8436b_cfx.jpg";

    let filename = format!(
        "{}/{}",
        "/home/cirno99/Code/company/hyperf-admin/api/public", resource
    );
    // 异步打开文件
    let mut file = File::open(filename.clone()).await.unwrap();

    // 创建一个缓冲区来存储文件内容
    let mut buffer = Vec::new();

    // 异步读取文件到缓冲区
    file.read_to_end(&mut buffer).await.unwrap();

    let wm = VipsImage::new_from_buffer(&buffer[..], "[access=VIPS_ACCESS_SEQUENTIAL]").unwrap();
    let (width, height) = (wm.get_width(), wm.get_height());
    let size = if width > height { width } else { height };
    let x = thumbnail(&filename, size).unwrap();
    let opts = GravityOptions {
        extend: Extend::White,
        background: vec![],
    };
    let x = gravity_with_opts(&x, CompassDirection::Centre, size, size, &opts).unwrap();

    x.image_write_to_file(
        format!(
            "{}/{}",
            "/home/cirno99/Code/company/hyperf-admin/api/public", "test_vips.jpg"
        )
        .as_str(),
    );
}

#[tokio::test]
async fn test_smartcrop() {
    use libvips::ops::Interesting;
    let resource = "files/2325/1146135/1221394/1215dd5adc2f0c571a3327af35c8436b_cfx.jpg";

    let filename = format!(
        "{}/{}",
        "/home/cirno99/Code/company/hyperf-admin/api/public", resource
    );
    // 异步打开文件
    let mut file = File::open(filename.clone()).await.unwrap();

    // 创建一个缓冲区来存储文件内容
    let mut buffer = Vec::new();

    // 异步读取文件到缓冲区
    file.read_to_end(&mut buffer).await.unwrap();

    let (width, height) = (533, 533);
    let dimensions = i32::min(width, height);
    let wm = VipsImage::new_from_buffer(&buffer[..], "")
        .and_then(|image| {
            let scale = dimensions as f64 / i32::min(image.get_height(), image.get_width()) as f64;
            resize(&image, scale)
        })
        .and_then(|image_scaled| {
            smartcrop_with_opts(
                &image_scaled,
                width,
                height,
                &libvips::ops::SmartcropOptions {
                    interesting: Interesting::Centre,
                    attention_x: 0,
                    attention_y: 0,
                    premultiplied: false,
                },
            )
        })
        .unwrap();
    wm.image_write_to_file(
        format!(
            "{}/{}",
            "/home/cirno99/Code/company/hyperf-admin/api/public", "test_vips_crop.jpg"
        )
        .as_str(),
    );
}
#[tokio::test]
async fn test_get_rotated() {
    let result = utils::make_request(
        utils::RequestParametersBuilder::new("img-test").with_rotation(utils::Rotation::R270),
    )
    .await
    .expect("Unable to download file");
    utils::assert_result(&result[..], "raw_rotated.jpg");
}

#[tokio::test]
async fn test_get_resized() {
    let result =
        utils::make_request(utils::RequestParametersBuilder::new("img-test").with_size(100, 100))
            .await
            .expect("Unable to download file");
    utils::assert_result(&result[..], "resized.jpg");
}

#[tokio::test]
async fn test_get_watermarked_left() {
    let result = utils::make_request(
        utils::RequestParametersBuilder::new("img-test").add_watermark(
            "watermark",
            40,
            0.5f64,
            10,
            10,
            utils::WatermarkPosition::Point,
        ),
    )
    .await
    .expect("Unable to download file");
    utils::assert_result(&result[..], "watermarked_left.jpg");
}

#[tokio::test]
async fn test_get_watermarked_right() {
    let result = utils::make_request(
        utils::RequestParametersBuilder::new("img-test").add_watermark(
            "watermark",
            40,
            0.5f64,
            -10,
            -10,
            utils::WatermarkPosition::Point,
        ),
    )
    .await
    .expect("Unable to download file");
    utils::assert_result(&result[..], "watermarked_right.jpg");
}

#[tokio::test]
async fn test_get_watermarked_center() {
    let result = utils::make_request(
        utils::RequestParametersBuilder::new("img-test").add_watermark(
            "watermark",
            40,
            0.5f64,
            10,
            10,
            utils::WatermarkPosition::Center,
        ),
    )
    .await
    .expect("Unable to download file");
    utils::assert_result(&result[..], "watermarked_center.jpg");
}

#[tokio::test]
async fn test_get_watermarked_rotated() {
    let result = utils::make_request(
        utils::RequestParametersBuilder::new("img-test")
            .add_watermark(
                "watermark",
                40,
                0.5f64,
                10,
                10,
                utils::WatermarkPosition::Center,
            )
            .with_rotation(utils::Rotation::R90),
    )
    .await
    .expect("Unable to download file");
    utils::assert_result(&result[..], "rotated_watermarked.jpg");
}

#[tokio::test]
async fn test_get_encoded_webp() {
    let result = utils::make_request(
        utils::RequestParametersBuilder::new("img-test").with_format(utils::ImageFormat::Webp),
    )
    .await
    .expect("Unable to download file");
    utils::assert_result(&result[..], "raw.webp");
}

#[tokio::test]
async fn test_get_encoded_heic() {
    let result = utils::make_request(
        utils::RequestParametersBuilder::new("img-test").with_format(utils::ImageFormat::Heic),
    )
    .await
    .expect("Unable to download file");
    utils::assert_result(&result[..], "raw.heic");
}

#[tokio::test]
async fn test_get_encoded_webp_bad_quality() {
    let result = utils::make_request(
        utils::RequestParametersBuilder::new("img-test")
            .with_format(utils::ImageFormat::Webp)
            .with_quality(10),
    )
    .await
    .expect("Unable to download file");
    utils::assert_result(&result[..], "raw_bad_quality.webp");
}

#[tokio::test]
async fn test_get_raw_bad_quality() {
    let result = utils::make_request(
        utils::RequestParametersBuilder::new("img-test")
            .with_format(utils::ImageFormat::Jpeg)
            .with_quality(10),
    )
    .await
    .expect("Unable to download file");
    utils::assert_result(&result[..], "raw_bad_quality.jpg");
}

#[tokio::test]
async fn test_get_multiple_watermarks() {
    let result = utils::make_request(
        utils::RequestParametersBuilder::new("img-test")
            .add_watermark(
                "watermark",
                20,
                0.3f64,
                -10,
                -10,
                utils::WatermarkPosition::Point,
            )
            .add_watermark(
                "watermark",
                20,
                0.3f64,
                10,
                10,
                utils::WatermarkPosition::Center,
            )
            .add_watermark(
                "watermark",
                20,
                0.3f64,
                10,
                10,
                utils::WatermarkPosition::Point,
            ),
    )
    .await
    .expect("Unable to download file");
    utils::assert_result(&result[..], "multiple_watermarks.jpg");
}

#[tokio::test]
async fn test_get_watermark_no_alpha() {
    let result = utils::make_request(
        utils::RequestParametersBuilder::new("img-test")
            .add_watermark(
                "watermark",
                20,
                0.3f64,
                -10,
                -10,
                utils::WatermarkPosition::Point,
            )
            .add_watermark("lena", 20, 0.3f64, 10, 10, utils::WatermarkPosition::Center)
            .add_watermark("lena", 20, 0.3f64, 10, 10, utils::WatermarkPosition::Point),
    )
    .await
    .expect("Unable to download file");
    utils::assert_result(&result[..], "watermarks_no_alpha.jpg");
}

#[tokio::test]
async fn test_get_exif_watermark() {
    let result = utils::make_request(
        utils::RequestParametersBuilder::new("exif")
            .add_watermark(
                "watermark",
                20,
                0.3f64,
                -10,
                -10,
                utils::WatermarkPosition::Point,
            )
            .add_watermark("lena", 20, 0.3f64, 10, 10, utils::WatermarkPosition::Center)
            .add_watermark("exif", 20, 0.3f64, 10, 10, utils::WatermarkPosition::Point),
    )
    .await
    .expect("Unable to download file");
    utils::assert_result(&result[..], "exif_watermark.jpg");
}

#[tokio::test]
async fn test_get_all_features() {
    let result = utils::make_request(
        utils::RequestParametersBuilder::new("img-test")
            .with_format(utils::ImageFormat::Webp)
            .with_quality(50)
            .with_rotation(utils::Rotation::R180)
            .add_watermark(
                "watermark",
                33,
                0.3f64,
                -10,
                -10,
                utils::WatermarkPosition::Point,
            )
            .add_watermark(
                "watermark",
                33,
                0.3f64,
                10,
                10,
                utils::WatermarkPosition::Point,
            )
            .with_size(150, 150),
    )
    .await
    .expect("Unable to download file");
    utils::assert_result(&result[..], "all_features.webp");
}
