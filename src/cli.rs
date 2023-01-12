use anyhow::{Context, Result};
use clap::Parser;
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(author = env!("CARGO_PKG_AUTHORS"), version = env!("CARGO_PKG_VERSION"), about = "生成视频截图")]
pub struct Args {
    #[clap(short, long, default_value = "5", help = "纵向数量")]
    pub rows: u32,
    #[clap(short, long, default_value = "3", help = "横向数量")]
    pub cols: u32,
    #[clap(short, long, default_value = "2048", help = "横向尺寸")]
    pub width: u32,

    #[clap(short, long, default_value = "10", help = "图片之间的间隔")]
    pub space: u32,

    #[clap(long, default_value = "jpg", help = "输出文件扩展名")]
    pub ext: String,

    #[cfg(feature = "font")]
    #[clap(long, short, help = "手动指定使用字体的路径")]
    pub font: Option<PathBuf>,

    // flags
    #[clap(long, help = "输出文件去掉视频扩展名")]
    pub remove_ext: bool,

    #[cfg(target_os = "windows")]
    #[clap(long, help = "是否用一个窗口显示")]
    pub show: bool,

    #[clap(long, help = "是否跳过保存")]
    pub no_save: bool,

    #[clap(long, help = "在处理文件夹时，不进行报错，而是跳过")]
    pub ignore_error: bool,

    #[clap(
        long,
        help = "当图片是竖屏时，默认自动调整纵向和横向数量。使用此选项可以禁用此功能"
    )]
    pub no_auto_flip: bool,

    #[clap(long, help = "默认会覆盖已存文件，使用此选项可以禁用此功能")]
    pub no_overwrite: bool,

    #[clap(help = "视频路径")]
    pub input: PathBuf,
}

impl Args {
    pub fn num_of_frames(&self) -> u32 {
        self.rows * self.cols
    }
    pub fn scaled_frame_width(&self) -> u32 {
        (self.width - (self.cols + 1) * self.space) / self.cols
    }
    pub fn output_name(&self, input: &Path) -> Result<PathBuf> {
        if self.remove_ext {
            Ok(input.with_extension(&self.ext))
        } else {
            // append ext to filename
            let filename = input.file_name().context("input filename missing")?;
            let mut filename = filename.to_owned();
            filename.push(".");
            filename.push(&self.ext);

            Ok(input.with_file_name(filename))
        }
    }
}
