use anyhow::{Context, Result};
use ffmpeg_next::Rational;
use std::fmt;

pub struct VideoDuration(pub Rational);

impl VideoDuration {
    #[allow(dead_code)]
    pub fn new(t: Rational) -> Self {
        Self(t)
    }
    #[cfg(test)]
    pub fn new_f64_test_only(f: f64) -> Self {
        let r = Rational::from(f);
        Self(r)
    }
}

impl fmt::Display for VideoDuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 < Rational::new(0, 1) {
            let new_rational = Rational::new(0, 1) - self.0;
            write!(f, "-{}", Self(new_rational))
        } else if self.0 == Rational::new(0, 1) {
            write!(f, "00:00.000")
        } else if self.0 < Rational::new(3600, 1) {
            let t = self.0.numerator() as f64 / self.0.denominator() as f64;
            let secs = t.floor() as u64;
            write!(
                f,
                "{:02}:{:02}.{:03}",
                secs / 60,
                secs % 60,
                (t * 1000.0) as u64 % 1000
            )
        } else {
            let t = self.0.numerator() as f64 / self.0.denominator() as f64;
            let secs = t.floor() as u64;
            write!(
                f,
                "{:02}:{:02}:{:02}.{:03}",
                secs / 3600,
                secs / 60 % 60,
                secs % 60,
                (t * 1000.0) as u64 % 1000
            )
        }
    }
}

/// parse a string like "00:00:00.123" or "01:00:00.123" to Rational
pub fn parse_duration(s: &str) -> Result<Rational> {
    let mut parts = s.split(':');
    let hours = parts.next().context("missing hour part")?.parse::<i64>()?;
    let minutes = parts
        .next()
        .context("missing minute part")?
        .parse::<i64>()?;
    let seconds = parts
        .next()
        .context("missing seconds part")?
        .parse::<f64>()?;
    let secs = (hours * 3600 + minutes * 60) as f64 + seconds;
    let r = Rational::from(secs);
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn video_duration() {
        let t = 120.123;
        assert_eq!(VideoDuration::new_f64_test_only(t).to_string(), "02:00.123");

        let t = 3600.123;
        assert_eq!(
            VideoDuration::new_f64_test_only(t).to_string(),
            "01:00:00.123"
        );
    }

    #[test]
    fn parse_video_durations() {
        let s = "00:00:00.123";
        assert_eq!(parse_duration(s).unwrap(), Rational::new(123, 1000));

        let s = "01:00:00.123";
        assert_eq!(parse_duration(s).unwrap(), Rational::new(3600123, 1000));
    }
}
