use anyhow::{Context as _, Result};

#[cfg(feature = "font")]
pub use font::draw_text;

#[cfg(not(feature = "font"))]
/// open cv put_text
pub fn draw_text(img: &mut opencv::core::Mat, text: &str, x: u32, y: u32) -> Result<()> {
    use opencv::{
        core::{Point, Scalar},
        imgproc,
    };
    const DATA: &[(u32, f64)] = &[(2, 16.), (1, 0.), (0, 255.)];

    // 先写一个黑色的背景
    for (offset, color) in DATA {
        let point = Point::new((x + offset) as i32, (y + offset) as i32);
        imgproc::put_text(
            img,
            text,
            point,
            imgproc::FONT_HERSHEY_DUPLEX,
            0.9,
            Scalar::all(*color),
            1,
            imgproc::LINE_AA,
            false,
        )
        .context("opencv::imgproc::put_text error")?;
    }
    Ok(())
}

#[cfg(feature = "font")]
mod font {
    use std::path::Path;

    use super::*;
    type Font = rusttype::Font<'static>;
    use once_cell::sync::OnceCell;
    use opencv::{
        core::{self as cv_core, Mat, MatTrait, MatTraitConst},
        prelude::MatExprTraitConst,
    };

    #[cfg(windows)]
    const WINDOWS_SIMHEI_PATH: &str = "C:\\Windows\\Fonts\\simhei.ttf";

    static GLOBAL_FONT: OnceCell<Result<Font>> = OnceCell::new();

    fn load_font(path: &Path) -> Result<Font> {
        debug!("loading font {}", path.display());
        let mut t = std::time::Instant::now();

        if !path.exists() {
            anyhow::bail!("Font {} does not exist", path.display());
        }
        let bytes =
            std::fs::read(path).with_context(|| format!("load font {} failed", path.display()))?;
        debug!(
            "font bytes ({} bytes) loaded, time cost: {:?}",
            bytes.len(),
            t.elapsed()
        );
        t = std::time::Instant::now();
        let font = Font::try_from_vec(bytes).context("rusttype load font from type failed")?;
        debug!(
            "load font {} success, time cost: {:?}",
            path.display(),
            t.elapsed()
        );
        Ok(font)
    }

    fn find_font(path: Option<&Path>) -> Result<Font> {
        if let Some(path) = path {
            match load_font(path) {
                Ok(f) => {
                    info!("Load font {} success", path.display());
                    return Ok(f);
                }
                Err(e) => {
                    warn!(
                        "Load font {} failed: {:?}, fallback to default",
                        path.display(),
                        e
                    );
                }
            }
        }
        #[cfg(windows)]
        {
            load_font(Path::new(WINDOWS_SIMHEI_PATH)).context("Load font failed")
        }
        #[cfg(not(windows))]
        {
            anyhow::bail!("对于非 Windows 系统，您必须手动指定字体路径")
        }
    }

    /// 返回 32FC1 的 mat，x_offset
    /// 会多留 1px 的边界
    fn text_to_single_channel_image(font: &Font, text: &str, font_size: f32) -> Result<(Mat, i32)> {
        let scale = rusttype::Scale::uniform(font_size);
        let v_metrics = font.v_metrics(scale);

        let glyphs: Vec<_> = font
            .layout(text, scale, rusttype::point(0.0, v_metrics.ascent))
            .collect();

        let glyphs_height = (v_metrics.ascent - v_metrics.descent).ceil() as u32;
        let (offset, glyphs_width) = {
            let min_x = glyphs
                .first()
                .map(|g| g.pixel_bounding_box().unwrap().min.x)
                .unwrap();
            let max_x = glyphs
                .last()
                .map(|g| g.pixel_bounding_box().unwrap().max.x)
                .unwrap();
            (min_x, (max_x - min_x) as u32)
        };
        let mut image = Mat::new_rows_cols_with_default(
            glyphs_height as i32 + 2,
            glyphs_width as i32 + 2,
            cv_core::CV_32FC1,
            cv_core::Scalar::all(0.0),
        )?;

        for glyph in glyphs {
            if let Some(bounding_box) = glyph.pixel_bounding_box() {
                // Draw the glyph into the image per-pixel by using the draw closure
                glyph.draw(|x, y, v| {
                    let x = (x as i32) + bounding_box.min.x - offset;
                    let y = (y as i32) + bounding_box.min.y;
                    // bgra
                    if let Ok(px) = image.at_2d_mut::<f32>(y + 1, x + 1) {
                        *px = v;
                    }
                });
            }
        }

        Ok((image, offset))
    }

    fn blur_fg_to_bg(fg: &Mat, ksize: i32, sigma: f64) -> Result<Mat> {
        let mut output = Mat::new_rows_cols_with_default(
            fg.rows(),
            fg.cols(),
            fg.typ(),
            cv_core::Scalar::all(0.0),
        )?;
        let dilate_kernel = opencv::imgproc::get_structuring_element(
            opencv::imgproc::MorphShapes::MORPH_RECT as i32,
            cv_core::Size_::new(3, 3),
            cv_core::Point_::new(-1, -1),
        )?;
        opencv::imgproc::dilate(
            fg,
            &mut output,
            &dilate_kernel,
            cv_core::Point_::new(-1, -1),
            1,
            cv_core::BORDER_CONSTANT,
            opencv::imgproc::morphology_default_border_value()?,
        )?;
        opencv::imgproc::gaussian_blur(
            &output.clone(),
            &mut output,
            cv_core::Size_::new(ksize, ksize),
            sigma,
            sigma,
            cv_core::BORDER_REPLICATE,
        )?;
        Ok(output)
    }

    /// 返回 (32FC3, x_offset)
    fn text_to_image2(
        font: &Font,
        text: &str,
        font_size: f32,
        color: (u8, u8, u8),
        bg_color: (u8, u8, u8),
    ) -> Result<(Mat, i32)> {
        let color = (
            color.0 as f32 / 255.0,
            color.1 as f32 / 255.0,
            color.2 as f32 / 255.0,
        );
        let bg_color = (
            bg_color.0 as f32 / 255.0,
            bg_color.1 as f32 / 255.0,
            bg_color.2 as f32 / 255.0,
        );

        let (alpha_f32, offset) = text_to_single_channel_image(font, text, font_size)?;
        // blur alpha
        let sigma = (font_size / 16.0) as f64;
        let ksize = sigma.ceil() as i32 * 2 + 1;
        let blurred_f32 = blur_fg_to_bg(&alpha_f32, ksize, sigma)?;

        let mut image = Mat::new_rows_cols_with_default(
            alpha_f32.rows(),
            alpha_f32.cols(),
            cv_core::CV_32FC4,
            cv_core::Scalar::all(0.0),
        )?;
        let (rows, cols) = (image.rows(), image.cols());
        // generate lambdas

        for r in 0..rows {
            for c in 0..cols {
                // compute
                let a1 = *blurred_f32.at_2d::<f32>(r, c)?;
                let a2 = *alpha_f32.at_2d::<f32>(r, c)?;
                let alpha = a1 + a2 * (1.0 - a1);
                if alpha < 1e-6 {
                    continue;
                }
                // color: f' = a2/a' f2 + (1-a2)a1/a' f2
                let k2 = a2 / alpha;
                let k1 = a1 * (1.0 - a2) / alpha;
                let color = (
                    k2 * color.0 + k1 * bg_color.0,
                    k2 * color.1 + k1 * bg_color.1,
                    k2 * color.2 + k1 * bg_color.2,
                );
                type P = cv_core::VecN<f32, 4>;
                let px = image.at_2d_mut::<P>(r, c)?;
                *px = P::from((color.0, color.1, color.2, alpha));
            }
        }

        Ok((image, offset))
    }

    /// 把 (BGRA) Mat 转换成 (BGR) 和 (AAA) Mat
    fn split_alpha(im: Mat) -> Result<(Mat, Mat)> {
        let ty = match im.typ() {
            cv_core::CV_8UC4 => cv_core::CV_8UC3,
            cv_core::CV_32FC4 => cv_core::CV_32FC3,
            _ => {
                anyhow::bail!("Unknown Mat type to split");
            }
        };
        let front =
            Mat::new_rows_cols_with_default(im.rows(), im.cols(), ty, cv_core::Scalar::all(0.))?;
        let alpha =
            Mat::new_rows_cols_with_default(im.rows(), im.cols(), ty, cv_core::Scalar::all(0.))?;
        let mut output: cv_core::Vector<Mat> = cv_core::Vector::new();
        output.push(Mat::copy(&front)?);
        output.push(Mat::copy(&alpha)?);

        cv_core::mix_channels(
            &im,
            &mut output,
            &[
                0, 0, 1, 1, 2, 2, // rgb
                3, 3, 3, 4, 3, 5, // alpha
            ],
        )
        .context("split alpha channels failed")?;
        Ok((front, alpha))
    }

    fn convert_8u3_to_f32(im: Mat) -> Result<Mat> {
        assert_eq!(im.typ(), cv_core::CV_8UC3);
        let mut im_f32 = Mat::new_rows_cols_with_default(
            im.rows(),
            im.cols(),
            cv_core::CV_32FC3,
            cv_core::Scalar::all(0.),
        )?;
        im.convert_to(&mut im_f32, cv_core::CV_32FC3, 1.0 / 255.0, 0.0)
            .context("opencv::core::convert_to error")?;
        Ok(im_f32)
    }

    fn convert_f32_to_8u3(im: Mat) -> Result<Mat> {
        assert_eq!(im.typ(), cv_core::CV_32FC3);
        let mut im_u8 = Mat::new_rows_cols_with_default(
            im.rows(),
            im.cols(),
            cv_core::CV_8UC3,
            cv_core::Scalar::all(0.),
        )?;
        im.convert_to(&mut im_u8, cv_core::CV_8UC3, 255.0, 0.0)
            .context("opencv::core::convert_to error")?;
        Ok(im_u8)
    }

    pub fn draw_text(
        img: &mut Mat,
        text: &str,
        x: u32,
        y: u32,
        font_size: f32,
        color: (u8, u8, u8),
        bg_color: (u8, u8, u8),
        font_path: Option<&Path>,
    ) -> Result<()> {
        let font = match GLOBAL_FONT.get_or_init(move || find_font(font_path)) {
            Ok(f) => f,
            Err(e) => {
                anyhow::bail!("Load font failed: {:#?}", e);
            }
        };
        let (text_f32, offset) = text_to_image2(font, text, font_size, color, bg_color)?;

        // split bgra to bgr
        let (front_f32, alpha_f32) = split_alpha(text_f32)?;
        //
        let roi = cv_core::Rect::new(
            x as i32 + offset,
            y as i32,
            front_f32.cols(),
            front_f32.rows(),
        );
        let bg = Mat::roi(img, roi)?;

        let bg_f32 = convert_8u3_to_f32(bg)?;

        let inv = (cv_core::Scalar::all(1.0) - &alpha_f32).into_result()?;
        let o = bg_f32.mul(&inv, 1.0)? + front_f32.mul(&alpha_f32, 1.0)?;
        let output_f32 = o.into_result()?.to_mat()?;
        let output = convert_f32_to_8u3(output_f32)?;
        let mut roi = Mat::roi(img, roi)?;
        cv_core::copy_to(&output, &mut roi, &Mat::default()).context("copy to failed")?;

        Ok(())
    }
}
