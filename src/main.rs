#[macro_use]
extern crate tracing;

mod utils;

use anyhow::{bail, Context as _, Result};
use ffmpeg_next as ffmpeg;
use std::path::Path;

use ffmpeg::{decoder, format, frame, software::scaling, Rational};

struct Transcoder {
    time_base: Rational,
    duration_s: Rational,
    input_stream_index: usize,

    decoder: decoder::Video,

    scaler: scaling::Context,

    frame_index: u32,

    // buffer
    decoded_frame: frame::Video,
    scaled_frame: frame::Video,
}
impl Transcoder {
    fn new(ictx: &format::context::Input) -> Result<Self> {
        let ist = ictx
            .streams()
            .best(ffmpeg::media::Type::Video)
            .context("no video stream found")?;
        let input_stream_index = ist.index();
        let time_base = ist.time_base();
        debug!(
            "ist index: {}, time_base: {}",
            input_stream_index, time_base
        );
        debug!(
            "video duration: {}, video frames: {}",
            ist.duration(),
            ist.frames()
        );

        let decoder = ffmpeg::codec::context::Context::from_parameters(ist.parameters())?
            .decoder()
            .video()?;
        info!("video size: W {} x H {}", decoder.width(), decoder.height(),);

        let duration_s = Self::decide_duration(&ist)?;
        info!("video duration: {}", utils::VideoDuration(duration_s));

        let scaler = scaling::Context::get(
            decoder.format(),
            decoder.width(),
            decoder.height(),
            ffmpeg::util::format::Pixel::RGB24,
            decoder.width(),
            decoder.height(),
            scaling::Flags::BILINEAR,
        )?;

        Ok(Self {
            time_base,
            duration_s,
            input_stream_index,
            decoder,
            scaler,
            frame_index: 0,
            decoded_frame: frame::Video::empty(),
            scaled_frame: frame::Video::empty(),
        })
    }

    fn decide_duration(ist: &format::stream::Stream) -> Result<Rational> {
        let duration_ts = ist.duration();
        if 0 < duration_ts && duration_ts < i32::MAX as i64 {
            let duration_s = Rational::new(duration_ts as i32, 1) * ist.time_base();
            return Ok(duration_s);
        }
        let meta = ist.metadata();
        for (key, value) in meta.iter() {
            debug!("ist metadata: {} = {}", key, value);
            if key.starts_with("DURATION") {
                let Ok(duration_s) = utils::parse_duration(value) else { continue };
                return Ok(duration_s);
            }
        }

        anyhow::bail!(
            "I don't know the duration of input (stream #{})",
            ist.index()
        );
    }

    fn run(&mut self, mut ictx: format::context::Input) -> Result<()> {
        const COLS: i32 = 4;
        const ROWS: i32 = 4;
        'thumb_gen: for i in 0..COLS * ROWS {
            let t = self.duration_s * Rational::new(2 * i + 1, 2 * COLS * ROWS);
            debug!("seeking to {}", utils::VideoDuration(t));

            // 这里的 position 是 AV_TIME_BASE，
            // 参见文档 https://ffmpeg.org/doxygen/trunk/group__lavf__decoding.html#ga3b40fc8d2fda6992ae6ea2567d71ba30
            let position = (f64::from(t) * ffmpeg::sys::AV_TIME_BASE as f64) as i64;
            trace!(" seeking with position = {}", position);
            ictx.seek(position, position..)
                .context("Seek to timestamp failed")?;

            for (stream, packet) in ictx.packets() {
                if stream.index() == self.input_stream_index {
                    debug!(
                        "got one packet, sending to decoder... size: {}, duration: {:.3}, position: {}, pts: {}",
                        packet.size(),
                        utils::VideoDuration(self.convert_pts(packet.duration())?),
                        utils::VideoDuration(self.convert_pts(packet.position() as i64)?),
                        utils::VideoDuration(self.convert_pts(packet.pts().unwrap_or(0))?),
                    );
                    self.decoder
                        .send_packet(&packet)
                        .context("send packet to decoder failed")?;
                    self.receive_and_process_decoded_frames()
                        .context("receive and process decode frames error")?;
                    continue 'thumb_gen;
                }
            }
        }
        self.decoder
            .send_eof()
            .context("send eof to decoder failed")?;
        self.receive_and_process_decoded_frames()
            .context("receive and process decode frames after eof error")?;
        Ok(())
    }

    fn receive_and_process_decoded_frames(&mut self) -> Result<()> {
        while self.decoder.receive_frame(&mut self.decoded_frame).is_ok() {
            debug!(
                " decoder got one frame: size W {} x H {}, format {:?}, kind {:?}, pts {}",
                self.decoded_frame.width(),
                self.decoded_frame.height(),
                self.decoded_frame.format(),
                self.decoded_frame.kind(),
                utils::VideoDuration(self.convert_pts(self.decoded_frame.pts().unwrap_or(0))?),
            );
            let frame_time = self.convert_pts(self.decoded_frame.pts().unwrap_or(0))?;
            debug!(
                "   computed frame_time: {}",
                utils::VideoDuration(frame_time)
            );

            self.scaler
                .run(&self.decoded_frame, &mut self.scaled_frame)
                .context("Scale failed")?;
            if self.scaled_frame.planes() != 1 {
                bail!("scaled frame planes != 1");
            }

            let filename =
                format!("frame-{}-rgb.ppm", utils::VideoDuration(frame_time)).replace(':', "：");
            Self::save_rgb_frame(&self.scaled_frame, Path::new(&filename))
                .context("save rgb frame failed")?;
            self.frame_index += 1;
        }
        Ok(())
    }

    fn save_rgb_frame(frame: &frame::Video, filename: &Path) -> Result<()> {
        use std::io::Write;
        let mut f = std::fs::File::create(filename)?;
        let (w, h) = (
            frame.plane_width(0) as usize,
            frame.plane_height(0) as usize,
        );
        write!(f, "P6\n{w} {h}\n{}\n", 255)?;
        let data = frame.data(0);
        let linesize = data.len() / h;

        for i in 0..h {
            let mut src = &data[i * linesize..i * linesize + w * 3];
            std::io::copy(&mut src, &mut f)?;
        }
        Ok(())
    }

    fn convert_pts(&self, pts: i64) -> Result<Rational> {
        if pts > i32::MAX as i64 {
            bail!("pts too large: {}", pts);
        }
        let pts = pts as i32;
        let r = Rational::new(pts, 1).reduce() * self.time_base;
        Ok(r)
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    ffmpeg::init().context("ffmpeg init failed")?;

    let input = std::env::args().nth(1).context("no input file")?;
    let input = Path::new(&input);

    let ictx = ffmpeg::format::input(&input).context("open input failed")?;

    let mut transcoder = Transcoder::new(&ictx)?;
    transcoder.run(ictx)?;

    Ok(())
}
