use anyhow::{Context as _, Result};
use opencv::{
    core::{self as cv_core, Mat},
    imgproc,
};

pub fn draw_text(img: &mut Mat, text: &str, x: u32, y: u32) -> Result<()> {
    font::draw_text(img, text, x, y)
    // cv_draw_text(img, text, x, y)
}

/// open cv put_text
fn cv_draw_text(img: &mut Mat, text: &str, x: u32, y: u32) -> Result<()> {
    const DATA: &[(u32, f64)] = &[(2, 16.), (1, 0.), (0, 255.)];

    // 先写一个黑色的背景
    for (offset, color) in DATA {
        let point = cv_core::Point::new((x + offset) as i32, (y + offset) as i32);
        imgproc::put_text(
            img,
            text,
            point,
            imgproc::FONT_HERSHEY_DUPLEX,
            0.9,
            cv_core::Scalar::all(*color),
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
        core::{self as cv_core, ElemMul, Mat, MatTrait, MatTraitConst},
        imgproc,
    };

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

    fn find_font() -> Result<Font> {
        load_font(Path::new(WINDOWS_SIMHEI_PATH)).context("Load font failed")
    }

    fn text_to_image(
        font: &Font,
        text: &str,
        font_size: f32,
        color: (u8, u8, u8),
    ) -> Result<(Mat, i32)> {
        let scale = rusttype::Scale::uniform(font_size);
        let v_metrics = font.v_metrics(scale);

        // layout the glyphs in a line with 20 pixels padding
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
            glyphs_height as i32,
            glyphs_width as i32,
            cv_core::CV_8UC4,
            cv_core::Scalar::all(0.0),
        )?;

        fn mix(a: u8, b: u8, alpha: f32) -> u8 {
            (a as f32 * alpha + b as f32 * (1.0 - alpha)) as u8
        }

        for glyph in glyphs {
            if let Some(bounding_box) = glyph.pixel_bounding_box() {
                // Draw the glyph into the image per-pixel by using the draw closure
                glyph.draw(|x, y, v| {
                    let x = (x as i32) + bounding_box.min.x - offset;
                    let y = (y as i32) + bounding_box.min.y;
                    // bgra
                    type P = cv_core::VecN<u8, 4>;
                    if let Ok(px) = image.at_2d_mut::<P>(y, x) {
                        px[0] = mix(color.0, px[0], v);
                        px[1] = mix(color.1, px[1], v);
                        px[2] = mix(color.2, px[2], v);
                        px[3] = (v * 255.0) as u8;
                    }
                });
            }
        }

        Ok((image, offset))
    }

    pub fn draw_text(img: &mut Mat, text: &str, mut x: u32, mut y: u32) -> Result<()> {
        let font = match GLOBAL_FONT.get_or_init(find_font) {
            Ok(f) => f,
            Err(e) => {
                anyhow::bail!("Load font failed: {:#?}", e);
            }
        };
        let (text_mat, offset) = text_to_image(&font, text, 24.0, (0, 0, 0))?;
        // split bgra to bgr
        let (mut text_mat_bgr, mut text_mat_alpha) = (Mat::default(), Mat::default());
        cv_core::mix_channels(
            &text_mat,
            &[&mut text_mat_bgr, &mut text_mat_alpha],
            &[0, 0],
        );

        // mix image
        let roi = cv_core::Rect::new(
            x as i32 + offset,
            y as i32,
            text_mat.cols(),
            text_mat.rows(),
        );
        let mut roi = Mat::roi(img, roi)?;

        let new_roi = (&roi).elem_mul(cv_core::Scalar::all(1.0) - &text_mat_alpha)
            + text_mat.elem_mul(text_mat_alpha);
        let new_roi = new_roi.into_result()?;
        cv_core::copy_to(&new_roi, &mut roi, &Mat::default()).context("copy to failed")?;

        // let mask = text_mat[]
        // cv_core::copy_to(&text_mat, &mut roi, &Mat::default()).context("copy to failed")?;

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
