use anyhow::{Context as _, Result};
use opencv::{
    core::{self as cv_core, Mat},
    imgproc,
};

pub fn draw_text(img: &mut Mat, text: &str, x: u32, y: u32) -> Result<()> {
    cv_draw_text(img, text, x, y)
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
