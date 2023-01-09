use crate::cli::Args;
use anyhow::Result;
use opencv::core::Mat;

#[cfg(feature = "info")]
pub const TEXT_SIZE: f32 = 36.0;
#[cfg(feature = "info")]
pub const LINE_HEIGHT: u32 = 42;
#[cfg(feature = "info")]
pub const COLOR: (u8, u8, u8) = (0, 0, 0);

#[cfg(not(feature = "info"))]
#[derive(Clone)]
pub struct Info;

#[cfg(feature = "info")]
#[derive(Clone)]
pub struct Info {
    pub file_name: String,
    pub file_size: usize,
    pub video_width: u32,
    pub video_height: u32,
    pub video_duration: ffmpeg_next::Rational,
    pub video_codec: ffmpeg_next::Codec,
}

#[cfg(not(feature = "info"))]
pub fn info_area_height(_: &Args) -> u32 {
    0
}

#[cfg(feature = "info")]
pub fn info_area_height(args: &Args) -> u32 {
    args.space + LINE_HEIGHT * 3
}

#[cfg(not(feature = "info"))]
pub fn plot_info(_image: &mut Mat, _info: Info, _args: &Args) -> Result<()> {
    Ok(())
}

#[cfg(feature = "info")]
pub fn readable_size(s: usize) -> String {
    const K: usize = 1024;
    const KF: f64 = 1024.0;
    if s <= K {
        format!("{} B", s)
    } else if s <= K * K {
        format!("{:.2} KiB", s as f64 / KF)
    } else if s <= K * K * K {
        format!("{:.2} MiB", s as f64 / KF / KF)
    } else if s <= K * K * K * K {
        format!("{:.2} GiB", s as f64 / KF / KF / KF)
    } else {
        format!("{:.2} TiB", s as f64 / KF / KF / KF / KF)
    }
}

#[cfg(feature = "info")]
pub fn plot_info(image: &mut Mat, info: Info, args: &Args) -> Result<()> {
    use crate::text::draw_text;

    let indent = args.space;
    draw_text(
        image,
        &format!(
            "文件：{} ({})",
            info.file_name,
            readable_size(info.file_size)
        ),
        indent,
        indent,
        TEXT_SIZE,
        COLOR,
    )?;
    draw_text(
        image,
        &format!(
            "视频时长：{}，分辨率: {}×{}",
            crate::utils::VideoDuration(info.video_duration),
            info.video_width,
            info.video_height
        ),
        indent,
        indent + LINE_HEIGHT,
        TEXT_SIZE,
        COLOR,
    )?;
    draw_text(
        image,
        &format!("视频编码：{}", info.video_codec.name()),
        indent,
        indent + 2 * LINE_HEIGHT,
        TEXT_SIZE,
        COLOR,
    )?;

    Ok(())
}
