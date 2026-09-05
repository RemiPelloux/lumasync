#[cfg(windows)]
use super::desktop::DesktopCapture;
use super::frame::ScreenFrame;
use std::time::{Duration, Instant};
use xcap::Monitor;

const CAPTURE_RETRY: Duration = Duration::from_secs(2);

pub(super) struct ScreenCapture {
    monitor: Monitor,
    pub(super) frame: ScreenFrame,
    #[cfg(windows)]
    native: Option<DesktopCapture>,
    retry_at: Instant,
}

impl ScreenCapture {
    pub(super) fn new(monitor: Monitor) -> Self {
        Self {
            monitor,
            frame: ScreenFrame::default(),
            #[cfg(windows)]
            native: None,
            retry_at: Instant::now(),
        }
    }

    pub(super) fn update(&mut self) -> Result<bool, String> {
        #[cfg(windows)]
        if let Some(fresh) = self.native_update() {
            if fresh || !self.frame.pixels.is_empty() {
                return Ok(fresh);
            }
        }
        let image = self
            .monitor
            .capture_image()
            .map_err(|error| format!("Capture indisponible : {error}"))?;
        self.frame.set_image(image);
        Ok(true)
    }

    #[cfg(windows)]
    fn native_update(&mut self) -> Option<bool> {
        if self.native.is_none() && Instant::now() >= self.retry_at {
            self.retry_at = Instant::now() + CAPTURE_RETRY;
            self.native = self
                .monitor
                .x()
                .ok()
                .zip(self.monitor.y().ok())
                .and_then(|origin| DesktopCapture::new(origin).ok());
        }
        match self.native.as_mut()?.update(&mut self.frame) {
            Ok(fresh) => Some(fresh),
            Err(_) => {
                self.native = None;
                self.retry_at = Instant::now() + CAPTURE_RETRY;
                None
            }
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Reads the local desktop for two seconds; no image is saved or transmitted"]
    fn desktop_capture_smoke() {
        let monitor = Monitor::all().unwrap().remove(0);
        let origin = (monitor.x().unwrap(), monitor.y().unwrap());
        let mut capture = DesktopCapture::new(origin).expect("native DXGI unavailable");
        let mut frame = ScreenFrame::default();
        let mut times = Vec::new();
        let mut fresh_times = Vec::new();
        let mut fresh = 0;
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(2) {
            let tick = Instant::now();
            let updated = capture.update(&mut frame).unwrap();
            let elapsed = tick.elapsed().as_secs_f64() * 1000.0;
            fresh += usize::from(updated);
            times.push(elapsed);
            if updated {
                fresh_times.push(elapsed);
            }
            std::thread::sleep(Duration::from_millis(16));
        }
        assert!(fresh > 0);
        times.sort_by(f64::total_cmp);
        println!(
            "DXGI {}x{}, {fresh} fresh frames, poll p50={:.3}ms p95={:.3}ms",
            frame.width,
            frame.height,
            times[times.len() / 2],
            times[times.len() * 95 / 100]
        );
        fresh_times.sort_by(f64::total_cmp);
        println!(
            "Fresh-frame copy: n={} p50={:.3}ms (idle polls excluded)",
            fresh_times.len(),
            fresh_times[fresh_times.len() / 2]
        );
    }
}
