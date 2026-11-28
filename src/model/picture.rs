//! Picture-to-keymapping: decode a still image (or the first frame of an
//! animated GIF), then sample it at each physical key so the same data drives
//! the preview and the flash-backed USERPIC upload.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use image::codecs::gif::GifDecoder;
use image::imageops::FilterType;
use image::{AnimationDecoder, ImageFormat, ImageReader, RgbaImage};

use crate::model::layout::{KeyDef, Layout};
use crate::model::lighting::ImageFit;

/// Pictures are downscaled before sampling; 68 keys cannot show more detail.
const SAMPLE_MAX_EDGE: u32 = 640;

#[derive(Clone, Debug)]
pub struct Picture {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub keys: Vec<[u8; 3]>,
}

impl Picture {
    pub fn load(path: &Path, layout: &Layout, fit: ImageFit) -> Result<Self, String> {
        let reader = ImageReader::open(path)
            .map_err(|err| format!("Could not open {}: {err}", path.display()))?
            .with_guessed_format()
            .map_err(|err| format!("Could not read {}: {err}", path.display()))?;
        let format = reader.format();

        if format == Some(ImageFormat::Gif) {
            return Self::load_gif(path, layout, fit);
        }

        let image = reader
            .decode()
            .map_err(|err| format!("Could not decode {}: {err}", path.display()))?
            .to_rgba8();
        let (width, height) = image.dimensions();
        let sampled = resize_for_sampling(image);
        Ok(Picture {
            path: path.to_path_buf(),
            width,
            height,
            keys: map_to_keys(&sampled, layout, fit),
        })
    }

    /// Animated GIFs are static on this keyboard, so only the first frame is
    /// decoded and mapped.
    fn load_gif(path: &Path, layout: &Layout, fit: ImageFit) -> Result<Self, String> {
        let file =
            File::open(path).map_err(|err| format!("Could not open {}: {err}", path.display()))?;
        let decoder = GifDecoder::new(BufReader::new(file))
            .map_err(|err| format!("Could not decode {}: {err}", path.display()))?;
        let frame = decoder
            .into_frames()
            .next()
            .ok_or_else(|| format!("{} contains no GIF frames", path.display()))?
            .map_err(|err| format!("Could not decode GIF frame: {err}"))?;
        let buffer = frame.into_buffer();
        let (width, height) = buffer.dimensions();
        let sampled = resize_for_sampling(buffer);
        Ok(Picture {
            path: path.to_path_buf(),
            width,
            height,
            keys: map_to_keys(&sampled, layout, fit),
        })
    }
}

fn resize_for_sampling(image: RgbaImage) -> RgbaImage {
    let (width, height) = image.dimensions();
    let longest = width.max(height);
    if longest <= SAMPLE_MAX_EDGE || longest == 0 {
        return image;
    }
    let scale = SAMPLE_MAX_EDGE as f32 / longest as f32;
    let width = ((width as f32 * scale).round() as u32).max(1);
    let height = ((height as f32 * scale).round() as u32).max(1);
    image::imageops::resize(&image, width, height, FilterType::Triangle)
}

/// Map each key's physical footprint onto the picture and average the pixels
/// it covers. This gives one colour per key without inventing an image grid.
fn map_to_keys(image: &RgbaImage, layout: &Layout, fit: ImageFit) -> Vec<[u8; 3]> {
    let mut keys = vec![[0, 0, 0]; layout.len()];
    let min_x = layout
        .keys
        .iter()
        .map(|key| key.x)
        .fold(f32::INFINITY, f32::min);
    let min_y = layout
        .keys
        .iter()
        .map(|key| key.y)
        .fold(f32::INFINITY, f32::min);
    let max_x = layout
        .keys
        .iter()
        .map(|key| key.x + key.w)
        .fold(f32::NEG_INFINITY, f32::max);
    let max_y = layout
        .keys
        .iter()
        .map(|key| key.y + key.h)
        .fold(f32::NEG_INFINITY, f32::max);
    let keyboard_w = (max_x - min_x).max(0.001);
    let keyboard_h = (max_y - min_y).max(0.001);
    let (image_w, image_h) = image.dimensions();
    if image_w == 0 || image_h == 0 {
        return keys;
    }
    let (rect_x, rect_y, rect_w, rect_h) =
        draw_rect(keyboard_w, keyboard_h, image_w as f32, image_h as f32, fit);
    for key in &layout.keys {
        if let Some(slot) = keys.get_mut(key.index) {
            *slot = sample_key(image, key, min_x, min_y, rect_x, rect_y, rect_w, rect_h);
        }
    }
    keys
}

/// Rectangle the picture occupies in keyboard coordinates. `Cover` may extend
/// past the keyboard bounds; `Contain` leaves black bars inside them.
fn draw_rect(
    keyboard_w: f32,
    keyboard_h: f32,
    image_w: f32,
    image_h: f32,
    fit: ImageFit,
) -> (f32, f32, f32, f32) {
    let (width, height) = match fit {
        ImageFit::Stretch => (keyboard_w, keyboard_h),
        ImageFit::Contain => {
            let scale = (keyboard_w / image_w).min(keyboard_h / image_h);
            (image_w * scale, image_h * scale)
        }
        ImageFit::Cover => {
            let scale = (keyboard_w / image_w).max(keyboard_h / image_h);
            (image_w * scale, image_h * scale)
        }
    };
    (
        (keyboard_w - width) * 0.5,
        (keyboard_h - height) * 0.5,
        width.max(0.001),
        height.max(0.001),
    )
}

fn sample_key(
    image: &RgbaImage,
    key: &KeyDef,
    min_x: f32,
    min_y: f32,
    rect_x: f32,
    rect_y: f32,
    rect_w: f32,
    rect_h: f32,
) -> [u8; 3] {
    let (image_w, image_h) = image.dimensions();
    let u0 = ((key.x - min_x - rect_x) / rect_w * image_w as f32).clamp(0.0, image_w as f32);
    let u1 =
        ((key.x + key.w - min_x - rect_x) / rect_w * image_w as f32).clamp(0.0, image_w as f32);
    let v0 = ((key.y - min_y - rect_y) / rect_h * image_h as f32).clamp(0.0, image_h as f32);
    let v1 =
        ((key.y + key.h - min_y - rect_y) / rect_h * image_h as f32).clamp(0.0, image_h as f32);
    let x0 = u0.floor() as u32;
    let x1 = u1.ceil().min(image_w as f32) as u32;
    let y0 = v0.floor() as u32;
    let y1 = v1.ceil().min(image_h as f32) as u32;
    if x1 <= x0 || y1 <= y0 {
        return [0, 0, 0];
    }

    // A fixed 6x6 sample keeps the cost independent of picture/key size.
    const STEPS: u32 = 6;
    let mut sums = [0u32; 3];
    let mut alpha = 0u32;
    for sy in 0..STEPS {
        let y = y0 + ((y1 - y0) * (sy * 2 + 1) / (STEPS * 2)).min(y1 - y0 - 1);
        for sx in 0..STEPS {
            let x = x0 + ((x1 - x0) * (sx * 2 + 1) / (STEPS * 2)).min(x1 - x0 - 1);
            let pixel = image.get_pixel(x, y);
            let a = pixel[3] as u32;
            sums[0] += pixel[0] as u32 * a;
            sums[1] += pixel[1] as u32 * a;
            sums[2] += pixel[2] as u32 * a;
            alpha += a;
        }
    }
    if alpha == 0 {
        return [0, 0, 0];
    }
    [
        (sums[0] / alpha) as u8,
        (sums[1] / alpha) as u8,
        (sums[2] / alpha) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::layout::layout;
    use image::codecs::gif::GifEncoder;
    use image::{Delay, Frame, Rgba, RgbaImage};

    #[test]
    fn solid_picture_maps_every_key() {
        let image = RgbaImage::from_pixel(32, 32, Rgba([30, 120, 210, 255]));
        let keys = map_to_keys(&image, layout(), ImageFit::Cover);
        assert_eq!(keys.len(), layout().len());
        assert!(keys.iter().all(|key| *key == [30, 120, 210]));
    }

    #[test]
    fn loads_still_png_and_first_gif_frame() {
        let dir = std::env::temp_dir();
        let stamp = std::process::id();
        let png_path = dir.join(format!("lk67-picture-{stamp}-still.png"));
        RgbaImage::from_pixel(12, 12, Rgba([10, 200, 30, 255]))
            .save_with_format(&png_path, ImageFormat::Png)
            .expect("write png");
        let png = Picture::load(&png_path, layout(), ImageFit::Stretch).expect("load png");
        assert!(
            png.keys
                .iter()
                .all(|color| color[0] < 20 && color[1] > 180 && color[2] < 50)
        );
        let _ = std::fs::remove_file(&png_path);

        let gif_path = dir.join(format!("lk67-picture-{stamp}-animated.gif"));
        let file = std::fs::File::create(&gif_path).expect("create gif");
        let mut encoder = GifEncoder::new(file);
        for color in [[255, 0, 0, 255], [0, 255, 0, 255]] {
            let frame = Frame::from_parts(
                RgbaImage::from_pixel(12, 12, Rgba(color)),
                0,
                0,
                Delay::from_numer_denom_ms(120, 1),
            );
            encoder.encode_frame(frame).expect("write gif frame");
        }
        drop(encoder);
        let gif = Picture::load(&gif_path, layout(), ImageFit::Stretch).expect("load gif");
        assert!(
            gif.keys
                .iter()
                .all(|color| color[0] > 180 && color[1] < 80 && color[2] < 80)
        );
        let _ = std::fs::remove_file(&gif_path);
    }
}
