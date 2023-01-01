use anyhow::{Context, Result};
use opencv::{
    core::{self as cv_core, prelude::*, Rect, Vector},
    imgcodecs, imgproc,
    prelude::*,
    Error,
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

    // imshow
    opencv::highgui::imshow("image", &mat)?;
    opencv::highgui::wait_key(0)?;

    Ok(mat)
}
