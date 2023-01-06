use crate::cli::Args;
use anyhow::Result;
use opencv::{
    core::{self as cv_core, prelude::*, Rect, Vector},
    imgcodecs, imgproc,
    prelude::*,
};

/// data are in BGR24 format, read data as opencv image
pub fn open_frame_data(
    width: usize,
    height: usize,
    line_size: usize,
    data: &mut [u8],
) -> Result<Mat> {
    assert_eq!(data.len(), line_size * height);

    // 必须拷贝一遍，因为 ffmpeg 的 data 有 padding
    let mut mat = unsafe { Mat::new_rows_cols(height as i32, width as i32, cv_core::CV_8UC3)? };

    for row in 0..height {
        let row_start = row * line_size;
        let row_end = row_start + width * 3;
        let row_data = &data[row_start..row_end];

        unsafe {
            let mat_ptr: &mut u8 = mat.ptr_mut(row as i32)?;
            let mat_slice: &mut [u8] = std::slice::from_raw_parts_mut(mat_ptr, row_data.len());
            mat_slice.copy_from_slice(row_data);
        }
    }

    Ok(mat)
}

/// 返回
pub fn merge_images(images: Vec<(Mat, String)>, args: &Args) -> Result<cv_core::Vector<u8>> {
    if images.is_empty() {
        anyhow::bail!("没有截图生成");
    }
    if images.len() != args.num_of_frames() as usize {
        warn!(
            "截图数量 {} 与预期 {} 不匹配，可能有截图生成错误",
            images.len(),
            args.num_of_frames()
        );
    }
    let (im_w, im_h) = (images[0].0.cols() as u32, images[0].0.rows() as u32);
    let (mut rows, mut cols) = (args.rows, args.cols);
    if im_w < im_h && rows > cols {
        debug!("自动调整行列数，使得图片不会太高");
        std::mem::swap(&mut rows, &mut cols);
    }

    let canvas_w = im_w * cols + args.space * (cols + 1);
    let canvas_h = im_h * rows + args.space * (rows + 1);

    let mut canvas = Mat::new_rows_cols_with_default(
        canvas_h as i32,
        canvas_w as i32,
        cv_core::CV_8UC3,
        cv_core::Scalar::all(255.),
    )?;

    'row: for r in 0..rows {
        for c in 0..cols {
            let idx = r as usize * cols as usize + c as usize;
            let Some((image, text)) = images.get(idx) else { break 'row; };
            // put image to canvas
            let x = args.space + c * (args.space + im_w);
            let y = args.space + r * (args.space + im_h);
            let pos = Rect::new(x as i32, y as i32, im_w as i32, im_h as i32);
            let mut roi = Mat::roi(&canvas, pos)?;
            image.copy_to(&mut roi)?;
            // draw border, shift one pixel out
            let border_color = cv_core::Scalar::new(0., 0., 0., 0.);
            let border_pos = Rect::new(
                (x - 1) as i32,
                (y - 1) as i32,
                (im_w + 2) as i32,
                (im_h + 2) as i32,
            );
            imgproc::rectangle(&mut canvas, border_pos, border_color, 1, imgproc::LINE_8, 0)?;

            // draw text
            draw_text(&mut canvas, text, x, y, im_h)?;
        }
    }

    // encode image
    let mut buf = Vector::new();
    let flags = Vector::new();
    let ext = format!(".{}", args.ext);
    imgcodecs::imencode(&ext, &canvas, &mut buf, &flags)?;

    Ok(buf)
}

fn draw_text(img: &mut Mat, text: &str, x: u32, y: u32, im_h: u32) -> Result<()> {
    const DATA: &[(u32, f64)] = &[(2, 16.), (1, 0.), (0, 255.)];

    // 先写一个黑色的背景
    for (offset, color) in DATA {
        let point = cv_core::Point::new((x + 5 + offset) as i32, (y + im_h - 5 + offset) as i32);
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
        )?;
    }
    Ok(())
}
