#![cfg_attr(feature = "gui", windows_subsystem = "windows")]

#[macro_use]
extern crate tracing;

use anyhow::{Context, Result};
use clap::Parser;
use ffmpeg_next as ffmpeg;

mod cli;
mod frame_extractor;
mod image_maker;
mod info;
mod process;
mod text;
mod utils;

fn _main() -> Result<()> {
    ffmpeg::init().context("ffmpeg init failed")?;
    let args = cli::Args::parse();
    process::start(args)
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    #[allow(clippy::let_and_return)]
    let r = _main();
    // msgbox
    #[cfg(feature = "gui")]
    if let Err(e) = r.as_ref() {
        msgbox::create(
            "创建缩略图发生错误",
            &format!("{:#?}", e),
            msgbox::IconType::Error,
        )
        .map_err(|e| {
            error!("msgbox failed: {:#?}", e);
            e
        })
        .ok();
    }

    r
}
