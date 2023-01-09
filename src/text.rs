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

    const WINDOWS_SIMHEI_PATH: &str = "C:\\Windows\\Fonts\\simhei.ttf";
    // const WINDOWS_SIMHEI_PATH: &str = r"C:\Windows\Fonts\ITCEDSCR.TTF";

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

    fn find_font() -> Result<Font> {
        load_font(Path::new(WINDOWS_SIMHEI_PATH)).context("Load font failed")
    }

    /// 返回 8UC1 的 mat，x_offset
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
            cv_core::CV_8UC1,
            cv_core::Scalar::all(0.0),
        )?;

        for glyph in glyphs {
            if let Some(bounding_box) = glyph.pixel_bounding_box() {
                // Draw the glyph into the image per-pixel by using the draw closure
                glyph.draw(|x, y, v| {
                    let x = (x as i32) + bounding_box.min.x - offset;
                    let y = (y as i32) + bounding_box.min.y;
                    // bgra
                    if let Ok(px) = image.at_2d_mut::<u8>(y + 1, x + 1) {
                        *px = (v * 255.0).ceil() as u8;
                    }
                });
            }
        }

        Ok((image, offset))
    }

    fn mix(c1: f32, a1: f32, c2: f32, a2: f32) -> f32 {
        // (c1 * a1 + c2 * a2 * (1.0 - a1)) / a
        (c1 * a1 + c2 * a2 * (1.0 - a1)) / (a1 + a2 * (1.0 - a1))
    }

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

        let (alpha, offset) = text_to_single_channel_image(font, text, font_size)?;

        let mut image = Mat::new_rows_cols_with_default(
            alpha.rows(),
            alpha.cols(),
            cv_core::CV_8UC4,
            cv_core::Scalar::from((bg_color.0 as f64, bg_color.1 as f64, bg_color.2 as f64, 0.0)),
        )?;
        let (rows, cols) = (image.rows(), image.cols());
        // generate lambdas
        let at = |r, c| {
            if c < 0 || c >= cols {
                // full transparent
                return 0;
            }
            if r < 0 || r >= rows {
                return 0;
            }
            #[cfg(debug_assertions)]
            {
                *alpha.at_2d::<u8>(r, c).unwrap()
            }
            #[cfg(not(debug_assertions))]
            {
                use opencv::prelude::MatTraitConstManual;
                unsafe { *alpha.at_2d_unchecked::<u8>(r, c).unwrap() }
            }
        };
        let at_f32 = |r, c| at(r, c) as f32 / 255.0;

        for r in 0..rows {
            for c in 0..cols {
                type P = cv_core::VecN<u8, 4>;
                let px = image.at_2d_mut::<P>(r, c)?;

                let center_alpha = at_f32(r, c);
                // mix
                let bg_alpha_1 = at_f32(r - 1, c);
                let bg_alpha_2 = at_f32(r + 1, c);
                let bg_alpha_3 = at_f32(r, c - 1);
                let bg_alpha_4 = at_f32(r, c + 1);
                debug_assert!(bg_alpha_1 <= 1.0);
                debug_assert!(bg_alpha_2 <= 1.0);
                debug_assert!(bg_alpha_3 <= 1.0);
                debug_assert!(bg_alpha_4 <= 1.0);
                // 0.5, 0.5, 0.5, 0.5 0.5 => 1 - 0.5^5
                let bg_alpha = 1.0
                    - (1.0 - bg_alpha_1)
                        * (1.0 - bg_alpha_2)
                        * (1.0 - bg_alpha_3)
                        * (1.0 - bg_alpha_4);

                let (c1, c2, c3, a) = (
                    mix(color.0, center_alpha, bg_color.0, bg_alpha),
                    mix(color.1, center_alpha, bg_color.1, bg_alpha),
                    mix(color.2, center_alpha, bg_color.2, bg_alpha),
                    center_alpha + bg_alpha * (1.0 - center_alpha),
                );
                px[0] = (c1 * 255.0).ceil() as u8;
                px[1] = (c2 * 255.0).ceil() as u8;
                px[2] = (c3 * 255.0).ceil() as u8;
                px[3] = (a * 255.0).ceil() as u8;
            }
        }

        Ok((image, offset))
    }

    #[allow(dead_code)]
    /// return (image_with_alpha, x_offset)
    fn text_to_image(
        font: &Font,
        text: &str,
        font_size: f32,
        color: (u8, u8, u8),
    ) -> Result<(Mat, i32)> {
        let (alpha, offset) = text_to_single_channel_image(font, text, font_size)?;

        let mut image = Mat::new_rows_cols_with_default(
            alpha.rows(),
            alpha.cols(),
            cv_core::CV_8UC4,
            cv_core::Scalar::from((color.0 as f64, color.1 as f64, color.2 as f64, 0.0)),
        )?;
        cv_core::mix_channels(&alpha, &mut image, &[0, 3])?;

        Ok((image, offset))
    }

    /// 把 (BGRA) Mat 转换成 (BGR) 和 (AAA) Mat
    fn split_alpha(im: Mat) -> Result<(Mat, Mat)> {
        assert_eq!(im.typ(), cv_core::CV_8UC4);
        let front = Mat::new_rows_cols_with_default(
            im.rows(),
            im.cols(),
            cv_core::CV_8UC3,
            cv_core::Scalar::all(0.),
        )?;
        let alpha = Mat::new_rows_cols_with_default(
            im.rows(),
            im.cols(),
            cv_core::CV_8UC3,
            cv_core::Scalar::all(0.),
        )?;
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
        )?;
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
    ) -> Result<()> {
        let font = match GLOBAL_FONT.get_or_init(find_font) {
            Ok(f) => f,
            Err(e) => {
                anyhow::bail!("Load font failed: {:#?}", e);
            }
        };
        let (text_mat, offset) = text_to_image2(font, text, font_size, color, bg_color)?;

        // split bgra to bgr
        let (front, alpha) = split_alpha(text_mat)?;
        //
        let roi = cv_core::Rect::new(x as i32 + offset, y as i32, front.cols(), front.rows());
        let bg = Mat::roi(img, roi)?;

        // convert 0,255 to 0.0,1.0
        let front = convert_8u3_to_f32(front)?;
        let alpha = convert_8u3_to_f32(alpha)?;
        let bg = convert_8u3_to_f32(bg)?;

        let inv = (cv_core::Scalar::all(1.0) - &alpha).into_result()?;
        let o = bg.mul(&inv, 1.0)? + front.mul(&alpha, 1.0)?;
        let output = o.into_result()?.to_mat()?;
        let output = convert_f32_to_8u3(output)?;
        let mut roi = Mat::roi(img, roi)?;
        cv_core::copy_to(&output, &mut roi, &Mat::default()).context("copy to failed")?;

        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn text_to_image_test() {
            let font = find_font().unwrap();
            let text = "你好";
            // let font = load_font(Path::new(r"C:\Windows\Fonts\ITCEDSCR.TTF")).unwrap();
            // let text = "fig";
            // bgr
            let color = (0, 0, 255);
            let (image, offset) = text_to_image(&font, text, 800.0, color).unwrap();
            println!("offset: {}", offset);
        }
    }
}
