#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[macro_use]
extern crate tracing;

use anyhow::{bail, Context, Result};
use clap::Parser;
use ffmpeg_next as ffmpeg;
use std::io::Write;
use std::path::Path;

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
    let buf = image_maker::merge_images(frames, args)?;

    if args.show {
        // instead of using the imshow, use system default image viewer
        // make a temp file with the ext but a space-free name
        let filename = output
            .file_name()
            .context("get file_name failed")?
            .to_str()
            .context("get file_name str failed")?
            .replace(' ', "_");
        let tempfile = std::env::temp_dir().join(filename);
        debug!("tempfile: {}", tempfile.display());
        let mut f = std::fs::File::create(&tempfile)?;
        f.write_all(buf.as_slice())?;
        std::mem::drop(f);
        system_open(&tempfile)?;
    }
    if args.no_save {
        info!("image not saved");
    } else {
        let meta = std::fs::metadata(file)?;
        let mut f = std::fs::File::create(&output)?;
        f.write_all(buf.as_slice())?;
        std::mem::drop(f);
        info!("image saved to {}", output.display());
        // set time
        use filetime::FileTime;
        filetime::set_file_mtime(output, FileTime::from_last_modification_time(&meta))?;
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn system_open(path: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;

    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    Command::new("cmd")
        .args(&["/C", "start", path.to_string_lossy().as_ref()])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .context("spawn start failed")?
        .wait()
        .context("wait subprocess failed")?;

    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn system_open(path: &Path) -> Result<()> {
    compile_error!("not implemented")
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
