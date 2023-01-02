#[macro_use]
extern crate tracing;

use anyhow::{Context, Result};
use clap::Parser;
use ffmpeg_next as ffmpeg;

mod cli;
mod frame_extractor;
mod image_maker;
mod utils;

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    ffmpeg::init().context("ffmpeg init failed")?;

    let args = cli::Args::parse();

    let mut extractor = frame_extractor::FrameExtractor::new(
        &args.input,
        args.num_of_frames(),
        args.scaled_frame_width(),
    )?;

    let mut frames = vec![];

    while extractor.extract_frame_to_internal_buffer()? {
        let frame = &mut extractor.extracted_bgr_frame;
        let (width, height, line_size) = (frame.width(), frame.height(), frame.stride(0));
        assert_eq!(width, args.scaled_frame_width());
        let data = frame.data_mut(0);

        let mat = image_maker::open_frame_data(width as usize, height as usize, line_size, data)?;
        let time = extractor.extracted_bgr_frame_time;
        frames.push((mat, time.to_string()));
    }

    let output = args.output_name(&args.input)?;
    image_maker::merge_images(frames, &args, &output)?;

    Ok(())
}
