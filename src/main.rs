#[macro_use]
extern crate tracing;

use anyhow::{Context, Result};
use ffmpeg_next as ffmpeg;

mod frame_extractor;
mod utils;

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    ffmpeg::init().context("ffmpeg init failed")?;

    let input = std::env::args().nth(1).context("no input file")?;
    let input = std::path::Path::new(&input);

    let mut extractor = frame_extractor::FrameExtractor::new(input, 16)?;
    extractor.extract_frames_to_ppm()?;

    Ok(())
}
