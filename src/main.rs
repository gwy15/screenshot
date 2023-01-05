#![windows_subsystem = "windows"]

#[macro_use]
extern crate tracing;

use anyhow::{bail, Context, Result};
use clap::Parser;
use ffmpeg_next as ffmpeg;

mod cli;
mod frame_extractor;
mod image_maker;
mod utils;

fn run(file: &std::path::Path, args: &cli::Args) -> Result<()> {
    assert!(file.exists());
    assert!(file.is_file());
    info!("Generating for file {}", file.display());
    let mut extractor = frame_extractor::FrameExtractor::new(
        file,
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

    let output = args.output_name(file)?;
    image_maker::merge_images(frames, args, &output)?;
    Ok(())
}

fn visit_recursive_dir(dir: &std::path::Path, args: &cli::Args) -> Result<()> {
    for entry in dir.read_dir()? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            let Some(ext) =  path.extension() else {continue};
            let Some(ext) = ext.to_str() else {continue};
            if matches!(ext, "mp4" | "mkv" | "avi" | "webm" | "mov" | "flv" | "ts") {
                run(&path, args)?;
            } else {
                debug!("skipping file: {}", path.display());
            }
        } else {
            visit_recursive_dir(&path, args)?;
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    ffmpeg::init().context("ffmpeg init failed")?;

    let args = cli::Args::parse();
    if !args.input.exists() {
        bail!("input file does not exist: {}", args.input.display());
    }
    if args.input.is_dir() {
        visit_recursive_dir(&args.input, &args)?;
    } else {
        run(&args.input, &args)?;
    }

    Ok(())
}
