// (c) Copyright 2019-2024 OLX

use crate::commons::*;
use libvips::ops;
use libvips::Result;
use libvips::VipsImage;
use log::*;

#[derive(Clone)]
pub struct VipsOutput(Option<Vec<u8>>);

impl From<Vec<u8>> for VipsOutput {
    fn from(buf: Vec<u8>) -> Self {
        Self(Some(buf))
    }
}
impl From<VipsOutput> for Vec<u8> {
    fn from(vo: VipsOutput) -> Vec<u8> {
        Option::expect(vo.0.to_owned(), "error")
    }
}

impl Drop for VipsOutput {
    fn drop(&mut self) {
        if let Some(buf) = self.0.take() {
            let ptr = buf.as_ptr();
            std::mem::forget(buf);
            unsafe { glib_sys::g_free(ptr as *mut _) };
        }
    }
}

pub fn save_buffer_fn(
    format: ImageFormat,
    final_image: &VipsImage,
    quality: i32,
) -> Result<VipsOutput> {
    match format {
        ImageFormat::Jpeg => {
            let options = ops::JpegsaveBufferOptions {
                q: quality,
                background: vec![255.0],
                optimize_coding: true,
                interlace: true,
                ..ops::JpegsaveBufferOptions::default()
            };
            let out = ops::jpegsave_buffer_with_opts(&final_image, &options).map(|u8| u8.into());
            final_image.image_set_kill(true);
            drop(options);
            out
        }
        ImageFormat::Webp => {
            let options = ops::WebpsaveBufferOptions {
                q: quality,
                effort: 2,
                ..ops::WebpsaveBufferOptions::default()
            };
            let out = ops::webpsave_buffer_with_opts(&final_image, &options).map(|u8| u8.into());
            final_image.image_set_kill(true);
            drop(options);
            out
        }
        ImageFormat::Png => {
            let options = ops::PngsaveBufferOptions {
                q: quality,
                bitdepth: 8,
                ..ops::PngsaveBufferOptions::default()
            };
            let out = ops::pngsave_buffer_with_opts(&final_image, &options).map(|u8| u8.into());
            final_image.image_set_kill(true);
            drop(options);
            out
        }
        ImageFormat::Heic => {
            let options = ops::HeifsaveBufferOptions {
                q: quality,
                ..ops::HeifsaveBufferOptions::default()
            };
            let out = ops::heifsave_buffer_with_opts(&final_image, &options).map(|u8| u8.into());
            final_image.image_set_kill(true);
            drop(options);
            out
        }
    }
}

pub fn process_image(
    buffer: Vec<u8>,
    wm_buffers: Vec<Vec<u8>>,
    parameters: ProcessImageRequest,
) -> Result<VipsOutput> {
    let ProcessImageRequest {
        image_address: _addr,
        size,
        format,
        quality,
        watermarks,
        rotation,
        crop,
        square,
    } = parameters;
    let needs_rotation = rotation.is_some()
        || match rexif::parse_buffer_quiet(&buffer[..]).0 {
            Ok(data) => data.entries.into_iter().any(|e| {
                e.tag == rexif::ExifTag::Orientation
                    && e.value.to_i64(0).is_some()
                    && e.value.to_i64(0).unwrap() != 0
                    && e.value.to_i64(0).unwrap() != 1
            }),
            Err(_) => false,
        };
    let options = if !needs_rotation {
        "[access=VIPS_ACCESS_SEQUENTIAL]"
    } else {
        ""
    };
    let mut final_image = VipsImage::new_from_buffer(&buffer.as_slice(), options)?;

    if crop.w.is_some() && crop.h.is_some() {
        debug!("Smart crop: {}", crop);
        if let (Some(width), Some(height)) = (crop.w, crop.h) {
            let (fw, fh) = (final_image.get_height(), final_image.get_width());
            // 只在url的w和h小于原图的情况下处理
            if fw >= width && fh >= height {
                final_image = ops::smartcrop_with_opts(
                    &final_image,
                    width,
                    height,
                    &libvips::ops::SmartcropOptions {
                        interesting: ops::Interesting::Centre,
                        attention_x: 0,
                        attention_y: 0,
                        premultiplied: false,
                    },
                )?;
            }
        }
    }

    let image_width = final_image.get_width();
    let image_height = final_image.get_height();

    for (i, wm_buffer) in wm_buffers.iter().enumerate() {
        let watermark = &watermarks[i];
        debug!("Applying watermark: {:?}", watermark);
        let wm = VipsImage::new_from_buffer(&wm_buffer[..], "[access=VIPS_ACCESS_SEQUENTIAL]")?;

        let wm_width = wm.get_width();
        let wm_height = wm.get_height();

        let (wm_target_width, wm_target_height) = get_watermark_target_size(
            image_width,
            image_height,
            wm_width,
            wm_height,
            watermark.size,
        )?;

        let target_smaller = wm_width * wm_height > wm_target_width * wm_target_height;
        let wm = if target_smaller {
            ops::resize(&wm, f64::from(wm_target_width) / f64::from(wm_width))?
        } else {
            wm
        };

        let mut alpha = [1.0, 1.0, 1.0, watermark.alpha];
        let mut add = [0.0, 0.0, 0.0, 0.0];

        let wm = if !wm.image_hasalpha() {
            ops::bandjoin_const(&wm, &mut [255.0])?
        } else {
            wm
        };

        let wm = ops::linear(&wm, &mut alpha, &mut add)?;
        let (left, top, right, bottom) = get_watermark_borders(
            image_width,
            image_height,
            wm_target_width,
            wm_target_height,
            &watermark.position,
        );
        debug!(
            "Watermark position - Padding: top: {}, left: {}, bottom: {}, right: {}",
            top, left, bottom, right
        );
        let options = ops::Composite2Options {
            x: left,
            y: top,
            ..ops::Composite2Options::default()
        };
        let wm = if !target_smaller {
            ops::resize(&wm, f64::from(wm_target_width) / f64::from(wm_width))?
        } else {
            wm
        };
        final_image =
            ops::composite_2_with_opts(&final_image, &wm, ops::BlendMode::Over, &options)?;
    }

    if square {
        let (width, height) = (final_image.get_width(), final_image.get_height());
        let size = i32::max(width, height);
        final_image = ops::thumbnail_image(&final_image, size)?;
        let opts = ops::GravityOptions {
            extend: ops::Extend::White,
            background: vec![],
        };
        final_image = ops::gravity_with_opts(
            &final_image,
            ops::CompassDirection::Centre,
            size,
            size,
            &opts,
        )?;
    }

    debug!("Encoding to: {}", format);
    save_buffer_fn(format, &final_image, quality)
}

fn resize_image(img: &VipsImage, size: &Size) -> Result<VipsImage> {
    debug!("Resizing image to {:?}", size);
    let original_width = img.get_width();
    let original_height = img.get_height();

    debug!(
        "Resizing image. Original size: {}x{}. Desired: {:?}",
        original_width, original_height, size
    );

    let (target_width, target_height) = get_target_size(original_width, original_height, size)?;

    debug!("Final size: {}x{}", target_width, target_height);

    ops::resize(&img, f64::from(target_width) / f64::from(original_width))
}
