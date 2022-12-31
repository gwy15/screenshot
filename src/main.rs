use anyhow::{Context, Result};
use std::{path::Path, process::Command, time::Instant};

fn get_video_length(path: &Path) -> Result<f64> {
    // ffprobe -v error -show_entries format=duration -of default=noprint_wrappers=1:nokey=1
    Command::new("ffprobe")
        .args(&["-v", "error"])
        .args(&["-show_entries", "format=duration"])
        .args(&["-of", "default=noprint_wrappers=1:nokey=1"])
        .arg(path)
        .output()
        .context("Execute ffprobe failed")
        .and_then(|output| {
            String::from_utf8(output.stdout).context("Failed to parse output from ffprobe")
        })
        .and_then(|output| {
            output
                .trim()
                .parse::<f64>()
                .with_context(|| format!("Failed to parse duration from ffprobe: {}", output))
        })
}

fn generate_screenshot(path: &Path, time: f64, output: &Path) -> Result<()> {
    let mut command = Command::new("ffmpeg");
    command
        .args(&["-ss", &format!("{:.4}", time)])
        .arg("-noaccurate_seek") // https://trac.ffmpeg.org/wiki/Seeking
        .arg("-i")
        .arg(path)
        .arg("-y") // overwrite output file
        .args(&["-q:v", "2"]) // control output quality
        .args(&["-frames:v", "1"])
        .arg(output);
    println!("Running {:?}", command);
    let status = command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .context("Failed to execute ffmpeg")?;
    if !status.success() {
        anyhow::bail!("ffmpeg failed");
    }

    Ok(())
}

fn main() -> Result<()> {
    let arg = std::env::args().nth(1).context("Missing argument")?;
    let path = Path::new(&arg);
    if !path.exists() {
        anyhow::bail!("File not found");
    }

    let video_length = get_video_length(path)?;
    let output = tempfile::tempdir().context("Failed to create temp dir")?;
    let t = Instant::now();
    for i in 0..24 {
        let time = video_length * (i as f64 + 0.5) / 24.0;
        let output = output.as_ref().join(format!("{}.jpg", i));
        let t = Instant::now();
        generate_screenshot(path, time, &output)
            .with_context(|| format!("Generate {i} th screenshot failed"))?;
        println!(
            "Generate {i} th screenshot in {t:.2?}",
            i = i,
            t = t.elapsed()
        );
    }
    println!("Total time: {t:.2?}", t = t.elapsed());

    Ok(())
}
