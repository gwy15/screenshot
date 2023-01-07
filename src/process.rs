use anyhow::{bail, Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::time::Instant;

use crate::{cli, frame_extractor, image_maker};

pub fn start(args: cli::Args) -> Result<()> {
    if !args.input.exists() {
        bail!("input file does not exist: {}", args.input.display());
    }
    if args.input.is_dir() {
        let (error_tx, error_rx) = mpsc::channel();
        let args = Arc::new(args);
        visit_recursive_dir(args.input.clone(), args.clone(), error_tx);
        let mut errors = vec![];
        while let Ok((path, e)) = error_rx.recv() {
            if args.ignore_error {
                bail!("处理文件 {} 错误: {:#}", path.display(), e);
            }
            errors.push((path, e));
        }

        info!("处理文件完成, 有 {} 个文件处理失败：", errors.len());
        for (path, e) in errors.iter() {
            error!("处理文件 {} 错误: {:#}", path.display(), e);
        }
    } else {
        run(&args.input, &args)
            .with_context(|| format!("处理文件 {} 错误", args.input.display()))?;
    }
    Ok(())
}

fn run(file: &std::path::Path, args: &cli::Args) -> Result<()> {
    assert!(file.exists());
    assert!(file.is_file());
    debug!("Generating for file {}", file.display());
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
        .args(["/C", "start", path.to_string_lossy().as_ref()])
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

fn is_video(path: &Path) -> bool {
    let ext = path.extension().and_then(|s| s.to_str());
    let Some(ext) = ext else {return false};
    matches!(
        ext,
        "mp4" | "m4v" | "mkv" | "avi" | "webm" | "mov" | "flv" | "ts"
    )
}

fn visit_recursive_dir(
    dir: PathBuf,
    args: Arc<cli::Args>,
    error_tx: mpsc::Sender<(PathBuf, anyhow::Error)>,
) {
    for entry in dir.read_dir().unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_file() {
            let is_video = is_video(&path);
            if !is_video {
                debug!("skipping file: {}", path.display());
                continue;
            }
            info!("处理文件 {}", path.display());
            let error_tx = error_tx.clone();
            let args = Arc::clone(&args);

            let task = move || {
                let t = Instant::now();
                let run_result = run(&path, &args);
                match run_result {
                    Ok(_) => {
                        info!("处理文件成功，耗时 {:?}：{}", t.elapsed(), path.display());
                    }
                    Err(e) => {
                        error_tx.send((path, e)).unwrap();
                    }
                }
            };
            rayon::spawn(task);
        } else {
            let e_tx = error_tx.clone();
            let args = Arc::clone(&args);
            rayon::spawn(move || {
                visit_recursive_dir(path, args, e_tx);
            });
        }
    }
}
