#[macro_use]
extern crate tracing;

use anyhow::{Context, Result};
use ffmpeg_next as ffmpeg;

mod frame_extractor;
mod image_maker;
mod utils;

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    ffmpeg::init().context("ffmpeg init failed")?;

    let input = std::env::args().nth(1).context("no input file")?;
    let input = std::path::Path::new(&input);

    let num_of_frames = 16;
    let mut extractor = frame_extractor::FrameExtractor::new(input, num_of_frames)?;
    // extractor.extract_frames_to_ppm()?;
    while extractor.extract_frame_to_internal_buffer()? {
        let frame = &mut extractor.extracted_BGR_frame;
        let (width, height) = (frame.width(), frame.height());
        let data = frame.data_mut(0);
        image_maker::open_frame_data(width, height, data)?;
    }

    Ok(())
}
