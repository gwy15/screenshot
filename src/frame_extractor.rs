use crate::utils;
use anyhow::{bail, Context as _, Result};
use ffmpeg_next as ffmpeg;
use std::path::Path;

use ffmpeg::{decoder, format, frame, software::scaling, Rational};

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

pub struct FrameExtractor {
    ictx: format::context::Input,

    time_base: Rational,
    duration_s: Rational,
    input_stream_index: usize,

    decoder: decoder::Video,

    scaler: scaling::Context,

    packets_generated: u32,
    num_of_frames: u32,

    // buffer
    decoded_frame: frame::Video,
    pub extracted_bgr_frame: frame::Video,
    pub extracted_bgr_frame_time: utils::VideoDuration,

    // info
    pub info: crate::info::Info,
}
impl FrameExtractor {
    pub fn new(input_file: &Path, num_of_frames: u32, scaled_frame_size: u32) -> Result<Self> {
        let ictx = ffmpeg::format::input(&input_file).context("open input failed")?;

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
        let video_codec = decoder.codec().context("no codec found")?;
        debug!(
            "video size: W {} x H {}, codec {}",
            decoder.width(),
            decoder.height(),
            video_codec.name()
        );

        let duration_s = Self::decide_duration(&ictx, &ist)?;
        debug!("video duration: {}", utils::VideoDuration(duration_s));

        let scaler = scaling::Context::get(
            decoder.format(),
            decoder.width(),
            decoder.height(),
            ffmpeg::util::format::Pixel::BGR24,
            scaled_frame_size,
            decoder.height() * scaled_frame_size / decoder.width(),
            scaling::Flags::BILINEAR,
        )?;
        #[cfg(not(feature = "info"))]
        let info = crate::info::Info;

        #[cfg(feature = "info")]
        let info = crate::info::Info {
            file_name: input_file
                .file_name()
                .context("No filename")?
                .to_string_lossy()
                .to_string(),
            file_size: input_file.metadata().context("No metadata")?.len() as usize,
            video_width: decoder.width(),
            video_height: decoder.height(),
            video_duration: duration_s,
            video_codec,
        };

        Ok(Self {
            ictx,
            time_base,
            duration_s,
            input_stream_index,
            decoder,
            scaler,
            packets_generated: 0,
            num_of_frames,
            decoded_frame: frame::Video::empty(),
            extracted_bgr_frame: frame::Video::empty(),
            extracted_bgr_frame_time: utils::VideoDuration(Rational::new(0, 1)),
            info,
        })
    }

    fn decide_duration(
        ictx: &format::context::Input,
        ist: &format::stream::Stream,
    ) -> Result<Rational> {
        let duration_ts = ist.duration();
        if duration_ts > 0 && duration_ts != ffmpeg::sys::AV_NOPTS_VALUE {
            let time_base = ist.time_base();

            debug!("raw duration: {}, time_base: {}", duration_ts, time_base);
            let duration = Self::safe_mul(duration_ts, time_base).unwrap_or_else(|e| {
                warn!(
                    "Compute duration {} * {} failed: {e:#}",
                    duration_ts, time_base
                );
                Rational::from(((duration_ts * time_base.0 as i64) as f64) / time_base.1 as f64)
            });
            return Ok(duration);
        }

        // try from meta
        let meta = ist.metadata();
        debug!(
            "trying to find duration from {} metadata",
            meta.iter().count()
        );
        for (key, value) in meta.iter() {
            debug!("ist metadata: {} = {}", key, value);
            if key.starts_with("DURATION") {
                let Ok(duration_s) = utils::parse_duration(value) else { continue };
                return Ok(duration_s);
            }
        }

        // try from ictx
        let duration = unsafe {
            let ctx = &*(ictx.as_ptr());
            ctx.duration
        };
        if duration > 0 && duration != ffmpeg::sys::AV_NOPTS_VALUE {
            let d_f64 = duration as f64 / ffmpeg::sys::AV_TIME_BASE as f64;
            return Ok(Rational::from(d_f64));
        }

        anyhow::bail!(
            "I don't know the duration of input (stream #{})",
            ist.index()
        );
    }

    pub fn extract_frame_to_internal_buffer(&mut self) -> Result<bool> {
        'thumb_gen: while self.packets_generated < self.num_of_frames {
            let i = self.packets_generated;
            self.packets_generated += 1;
            let t =
                self.duration_s * Rational::new((2 * i + 1) as i32, 2 * self.num_of_frames as i32);
            debug!("seeking to {}", utils::VideoDuration(t));

            // 这里的 position 是 AV_TIME_BASE，
            // 参见文档 https://ffmpeg.org/doxygen/trunk/group__lavf__decoding.html#ga3b40fc8d2fda6992ae6ea2567d71ba30
            let position = (f64::from(t) * ffmpeg::sys::AV_TIME_BASE as f64) as i64;
            trace!(" seeking with position = {}", position);
            self.ictx
                .seek(position, position..)
                .or_else(|e| {
                    warn!(
                        "seek to {}.. failed: {e:#}, trying with more range",
                        utils::VideoDuration(t)
                    );
                    self.ictx.seek(position, ..)
                })
                .with_context(|| format!("Seek to {} failed", utils::VideoDuration(t)))?;

            for (stream, packet) in self.ictx.packets() {
                if stream.index() == self.input_stream_index {
                    debug!(
                        "got one packet, sending to decoder... packet size: {}, position: {}, pts: {}",
                        packet.size(),
                        utils::VideoDuration(self.convert_pts(packet.position() as i64)),
                        utils::VideoDuration(self.convert_pts(packet.pts().unwrap_or(0))),
                    );
                    self.decoder
                        .send_packet(&packet)
                        .context("send packet to decoder failed")?;
                    let frame_decoded = self
                        .receive_and_process_decoded_frame()
                        .context("receive and process decode frames error")?;
                    if frame_decoded {
                        return Ok(true);
                    }
                    continue 'thumb_gen;
                }
            }
        }
        self.decoder.send_eof().ok();
        let frame_decoded = self
            .receive_and_process_decoded_frame()
            .context("receive and process decode frames after eof error")?;
        Ok(frame_decoded)
    }

    fn receive_and_process_decoded_frame(&mut self) -> Result<bool> {
        if self.decoder.receive_frame(&mut self.decoded_frame).is_ok() {
            let pts = self.convert_pts(self.decoded_frame.pts().unwrap_or(0));
            let frame_time = pts;
            debug!(
                " decoder got one frame: frame size W {} x H {}, format {:?}, kind {:?}, pts {}",
                self.decoded_frame.width(),
                self.decoded_frame.height(),
                self.decoded_frame.format(),
                self.decoded_frame.kind(),
                utils::VideoDuration(frame_time),
            );

            self.scaler
                .run(&self.decoded_frame, &mut self.extracted_bgr_frame)
                .context("Scale failed")?;
            if self.extracted_bgr_frame.planes() != 1 {
                bail!("scaled frame planes != 1");
            }
            self.extracted_bgr_frame_time = utils::VideoDuration(frame_time);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn save_bgr_frame(frame: &frame::Video, filename: &Path) -> Result<()> {
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

    pub fn convert_pts(&self, pts: i64) -> Rational {
        Self::safe_mul(pts, self.time_base)
            .with_context(|| format!("compute {} * {} failed", pts, self.time_base))
            .unwrap_or_else(|e| {
                warn!("compute pts failed: {:#}, fallback to f64", e);
                let f = (pts * self.time_base.0 as i64) as f64 / self.time_base.1 as f64;
                Rational::from(f)
            })
    }

    #[allow(dead_code)]
    pub fn extract_frames_to_ppm(&mut self) -> Result<()> {
        for (idx, frame) in self.enumerate() {
            let frame = frame?;
            let filename = format!("frame-{}-BGR.ppm", idx);
            Self::save_bgr_frame(&frame, Path::new(&filename)).context("save BGR frame failed")?;
        }
        Ok(())
    }

    fn safe_mul(a: i64, r: Rational) -> Result<Rational> {
        let r = r.reduce();
        let mut a = a * r.0 as i64;
        let mut b = r.1 as i64;
        let gcd = gcd(a, b);
        a /= gcd;
        b /= gcd;
        if a > i32::MAX as i64 || b > i32::MAX as i64 {
            bail!("overflow");
        }

        Ok(Rational(a as i32, b as i32))
    }
}

impl Iterator for FrameExtractor {
    type Item = Result<frame::Video>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.extract_frame_to_internal_buffer() {
            Ok(extracted) => {
                if extracted {
                    let mut output = frame::Video::empty();
                    std::mem::swap(&mut output, &mut self.extracted_bgr_frame);
                    Some(Ok(output))
                } else {
                    None
                }
            }
            Err(e) => Some(Err(e)),
        }
    }
}
