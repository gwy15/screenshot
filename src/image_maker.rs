use anyhow::{Context, Result};
use opencv::{
    core::{self as cv_core, prelude::*, Rect, Vector},
    imgcodecs, imgproc,
    prelude::*,
    Error,
};

/// data are in RGB24 format, read data as opencv image
pub fn open_frame_data(width: u32, height: u32, data: &mut [u8]) -> Result<Mat> {
    let mut mat = unsafe {
        Mat::new_rows_cols_with_data(
            height as i32,
            width as i32,
            cv_core::CV_8UC3,
            (data as *mut [u8]).cast(),
            cv_core::Mat_AUTO_STEP,
        )
    }
    .context("Mat::new_rows_cols failed")?;

    // imshow
    opencv::highgui::imshow("image", &mat)?;
    opencv::highgui::wait_key(0)?;

    Ok(mat)
}
