use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, RecvTimeoutError},
        Arc, Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};
use xcap::{Frame, Monitor, VideoRecorder};

use crate::{
    dtls::HueStream,
    models::{ChannelAssignment, Credentials, Rgb, StartSyncRequest, SyncPhase, SyncStatus, Zone},
};

const PREVIEW_INTERVAL: Duration = Duration::from_millis(100);
const METRICS_INTERVAL: Duration = Duration::from_millis(500);
const TARGET_SAMPLES_LONG: u32 = 56;
const TARGET_SAMPLES_SHORT: u32 = 20;
const PREDICTION_LEAD: f32 = 0.42;
const VELOCITY_BLEND: f32 = 0.28;
const BLACK_BAR_STABLE_SKIP: u8 = 10;
const HUE_BUCKETS: usize = 24;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ContentBounds {
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
}

struct BoundsTracker {
    full: ContentBounds,
    stable: ContentBounds,
    candidate: ContentBounds,
    candidate_hits: u8,
    full_misses: u8,
}

impl BoundsTracker {
    fn new(full: ContentBounds) -> Self {
        Self {
            full,
            stable: full,
            candidate: full,
            candidate_hits: 0,
            full_misses: 0,
        }
    }

    fn update(&mut self, detected: ContentBounds) -> ContentBounds {
        if detected == self.stable {
            self.candidate_hits = 0;
            self.full_misses = 0;
            return self.stable;
        }
        if detected == self.full {
            self.full_misses = self.full_misses.saturating_add(1);
            if self.full_misses >= 12 {
                self.stable = self.full;
                self.candidate = self.full;
                self.candidate_hits = 0;
            }
            return self.stable;
        }

        self.full_misses = 0;
        if detected == self.candidate {
            self.candidate_hits = self.candidate_hits.saturating_add(1);
        } else {
            self.candidate = detected;
            self.candidate_hits = 1;
        }
        if self.candidate_hits >= 3 {
            self.stable = self.candidate;
            self.candidate_hits = 0;
        }
        self.stable
    }
}

impl ContentBounds {
    fn full(image: &image::RgbaImage) -> Self {
        Self {
            left: 0,
            top: 0,
            right: image.width(),
            bottom: image.height(),
        }
    }

    fn width(self) -> u32 {
        self.right.saturating_sub(self.left)
    }

    fn height(self) -> u32 {
        self.bottom.saturating_sub(self.top)
    }
}

#[derive(Clone, Copy)]
struct SampleRect {
    x_start: u32,
    x_end: u32,
    y_start: u32,
    y_end: u32,
    zone: Zone,
}

#[derive(Clone, Copy, Default)]
struct ChannelFilter {
    value: [f32; 3],
    velocity: [f32; 3],
    primed: bool,
    held: [f32; 3],
    hold_primed: bool,
}

/// Keeps the OS capture session alive between frames. `capture_image` and
/// `capture_region` both create a fresh Windows capture session; doing that once
/// per edge was the dominant source of latency. The recorder gives us the newest
/// coherent full-screen frame without copying its pixel buffer.
struct ScreenCapture {
    monitor: Monitor,
    recorder: Option<VideoRecorder>,
    frames: Option<Receiver<Frame>>,
}

impl ScreenCapture {
    fn new(monitor: Monitor) -> Self {
        let recorder = monitor
            .video_recorder()
            .ok()
            .and_then(|(recorder, frames)| {
                recorder.start().ok()?;
                Some((recorder, frames))
            });
        match recorder {
            Some((recorder, frames)) => Self {
                monitor,
                recorder: Some(recorder),
                frames: Some(frames),
            },
            None => Self {
                monitor,
                recorder: None,
                frames: None,
            },
        }
    }

    fn next_image(&mut self) -> Result<image::RgbaImage, String> {
        if let Some(frames) = &self.frames {
            match frames.recv_timeout(Duration::from_millis(120)) {
                Ok(mut frame) => {
                    // If rendering is slower than the monitor, discard queued
                    // stale frames and always light from the freshest one.
                    while let Ok(newer) = frames.try_recv() {
                        frame = newer;
                    }
                    return image::RgbaImage::from_raw(frame.width, frame.height, frame.raw)
                        .ok_or_else(|| "Le flux vidéo a fourni une image invalide.".to_owned());
                }
                Err(RecvTimeoutError::Timeout) => {
                    // A static desktop can occasionally stop producing recorder
                    // events. A one-shot frame keeps the stream alive.
                }
                Err(RecvTimeoutError::Disconnected) => {
                    self.frames = None;
                    self.recorder = None;
                }
            }
        }
        self.monitor
            .capture_image()
            .map_err(|error| format!("La capture de l’écran a échoué : {error}"))
    }
}

impl Drop for ScreenCapture {
    fn drop(&mut self) {
        if let Some(recorder) = &self.recorder {
            let _ = recorder.stop();
        }
    }
}

pub fn run_capture(
    credentials: Credentials,
    request: StartSyncRequest,
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<SyncStatus>>,
) -> Result<(), String> {
    let monitors = Monitor::all()
        .map_err(|error| format!("Les écrans ne peuvent pas être listés : {error}"))?;
    let monitor = monitors
        .into_iter()
        .nth(request.monitor_index)
        .ok_or_else(|| "Le moniteur choisi n’est plus disponible.".to_owned())?;
    let mut stream = HueStream::connect(
        &credentials.host,
        &credentials.app_key,
        &credentials.client_key,
        &request.area_id,
    )?;

    {
        let mut current = status
            .lock()
            .map_err(|_| "L’état de synchronisation est indisponible.".to_owned())?;
        current.running = true;
        current.phase = SyncPhase::Running;
        current.message = "Éclairage synchronisé".to_owned();
        current.dropped_frames = 0;
    }

    let frame_duration = Duration::from_secs_f64(1.0 / request.fps.clamp(10, 60) as f64);
    let frame_budget_ms = frame_duration.as_secs_f32() * 1_000.0;
    let monitor_width = monitor.width().unwrap_or(1920);
    let resolution_scale = if monitor_width >= 3500 {
        0.52
    } else if monitor_width >= 2500 {
        0.68
    } else {
        1.0
    };
    let mut filters = HashMap::<u8, ChannelFilter>::with_capacity(request.assignments.len());
    let mut measured_at = Instant::now();
    let mut preview_at = Instant::now();
    let mut previous_frame_at = Instant::now();
    let mut next_deadline = Instant::now();
    let mut frame_count = 0u32;
    let mut dropped_frames = 0u64;
    let mut average_frame_time_ms = 0.0f32;
    let mut capture_failures = 0u8;
    let mut bounds_tracker = None::<BoundsTracker>;
    let mut black_bar_skip = 0u8;
    let mut cached_bounds = None::<ContentBounds>;
    let mut sample_scale = resolution_scale;
    let mut outgoing = Vec::with_capacity(request.assignments.len());
    let mut preview = HashMap::with_capacity(request.assignments.len());
    let mut zone_colors = HashMap::<Zone, Rgb>::with_capacity(8);
    let unique_zones = {
        let mut zones = Vec::with_capacity(4);
        for assignment in &request.assignments {
            if !zones.contains(&assignment.zone) {
                zones.push(assignment.zone);
            }
        }
        zones
    };
    let mut capture = ScreenCapture::new(monitor);

    while !stop.load(Ordering::Relaxed) {
        let started = Instant::now();
        let image = match capture.next_image() {
            Ok(image) => {
                if capture_failures > 0 {
                    if let Ok(mut current) = status.lock() {
                        current.message = "Éclairage synchronisé".to_owned();
                    }
                }
                capture_failures = 0;
                image
            }
            Err(_) if capture_failures < 2 => {
                capture_failures += 1;
                if let Ok(mut current) = status.lock() {
                    current.message = "Nouvelle tentative de capture…".to_owned();
                }
                thread::sleep(Duration::from_millis(25));
                continue;
            }
            Err(error) => return Err(error),
        };
        let full_bounds = ContentBounds::full(&image);

        let bounds = if request.black_bar_detection && black_bar_skip == 0 {
            let tracker = bounds_tracker.get_or_insert_with(|| BoundsTracker::new(full_bounds));
            if tracker.full != full_bounds {
                *tracker = BoundsTracker::new(full_bounds);
            }
            let detected = detect_content_bounds(&image);
            let next = tracker.update(detected);
            cached_bounds = Some(next);
            black_bar_skip = if next == full_bounds {
                BLACK_BAR_STABLE_SKIP.saturating_add(6)
            } else if next == detected {
                BLACK_BAR_STABLE_SKIP
            } else {
                1
            };
            next
        } else {
            if request.black_bar_detection {
                black_bar_skip = black_bar_skip.saturating_sub(1);
            }
            cached_bounds.unwrap_or(full_bounds)
        };

        // A single coherent screen frame feeds every requested zone. This avoids
        // both repeated OS captures and edge-to-edge tearing.
        zone_colors.clear();
        for zone in &unique_zones {
            zone_colors.insert(
                *zone,
                sample_zone(
                    &image,
                    *zone,
                    bounds,
                    request.edge_depth,
                    request.brightness,
                    request.saturation,
                    sample_scale,
                ),
            );
        }

        let now = Instant::now();
        let frame_delta = now.duration_since(previous_frame_at);
        previous_frame_at = now;
        let will_publish_preview = preview_at.elapsed() >= PREVIEW_INTERVAL;
        let will_publish_metrics = measured_at.elapsed() >= METRICS_INTERVAL;
        outgoing.clear();
        if will_publish_preview {
            preview.clear();
        }
        for assignment in &request.assignments {
            let sampled = zone_colors
                .get(&assignment.zone)
                .copied()
                .unwrap_or_default();
            let filter = filters.entry(assignment.channel_id).or_default();
            let color = smooth_color(
                filter,
                sampled,
                request.reactivity,
                request.max_luminosity,
                frame_delta,
            );
            outgoing.push((assignment.channel_id, color));
            if will_publish_preview {
                preview.insert(assignment.channel_id.to_string(), color.hex());
            }
        }

        if let Err(first_error) = stream.send(&outgoing) {
            reconnect_stream(
                &mut stream,
                &credentials,
                &request.area_id,
                &outgoing,
                &stop,
                &status,
                first_error,
            )?;
        }

        frame_count += 1;
        let elapsed_work = started.elapsed();
        let frame_time_ms = elapsed_work.as_secs_f32() * 1_000.0;
        average_frame_time_ms = if average_frame_time_ms == 0.0 {
            frame_time_ms
        } else {
            average_frame_time_ms * 0.86 + frame_time_ms * 0.14
        };
        let load_scale = if average_frame_time_ms > frame_budget_ms * 0.88 {
            0.55
        } else if average_frame_time_ms > frame_budget_ms * 0.74 {
            0.72
        } else if average_frame_time_ms < frame_budget_ms * 0.55 {
            1.0
        } else {
            // Hysteresis band: keep current scale to avoid sample-pattern blink.
            sample_scale / resolution_scale.max(0.01)
        };
        let desired_scale = (resolution_scale * load_scale.clamp(0.45, 1.0)).clamp(0.45, 1.0);
        sample_scale = sample_scale * 0.9 + desired_scale * 0.1;
        if elapsed_work > frame_duration.mul_f32(1.15) {
            dropped_frames += (elapsed_work.as_secs_f64() / frame_duration.as_secs_f64())
                .floor()
                .max(1.0) as u64;
        }

        if will_publish_preview || will_publish_metrics {
            if let Ok(mut current) = status.lock() {
                if will_publish_preview {
                    current.colors.clone_from(&preview);
                    current.black_bars_detected = bounds != full_bounds;
                    preview_at = Instant::now();
                }
                if will_publish_metrics {
                    let elapsed = measured_at.elapsed().as_secs_f32();
                    current.measured_fps = frame_count as f32 / elapsed.max(0.001);
                    current.frame_time_ms = average_frame_time_ms;
                    current.dropped_frames = dropped_frames;
                    measured_at = Instant::now();
                    frame_count = 0;
                }
            }
        }

        next_deadline += frame_duration;
        let after_work = Instant::now();
        if next_deadline > after_work {
            thread::sleep(next_deadline - after_work);
        } else if after_work.duration_since(next_deadline) > frame_duration {
            next_deadline = after_work;
        }
    }
    Ok(())
}

fn reconnect_stream(
    stream: &mut HueStream,
    credentials: &Credentials,
    area_id: &str,
    outgoing: &[(u8, Rgb)],
    stop: &AtomicBool,
    status: &Mutex<SyncStatus>,
    first_error: String,
) -> Result<(), String> {
    for attempt in 1..=2 {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        if let Ok(mut current) = status.lock() {
            current.phase = SyncPhase::Reconnecting;
            current.message = format!("Reconnexion au pont… {attempt}/2");
        }
        thread::sleep(Duration::from_millis(150 * attempt));
        if let Ok(mut replacement) = HueStream::connect(
            &credentials.host,
            &credentials.app_key,
            &credentials.client_key,
            area_id,
        ) {
            if replacement.send(outgoing).is_ok() {
                *stream = replacement;
                if let Ok(mut current) = status.lock() {
                    current.phase = SyncPhase::Running;
                    current.message = "Éclairage synchronisé".to_owned();
                }
                return Ok(());
            }
        }
    }
    Err(format!(
        "Le flux Hue a été interrompu et la reconnexion a échoué : {first_error}"
    ))
}

fn smooth_color(
    filter: &mut ChannelFilter,
    target: Rgb,
    reactivity_percent: f32,
    max_luminosity_percent: f32,
    elapsed: Duration,
) -> Rgb {
    let mut target = [target.r as f32, target.g as f32, target.b as f32];
    apply_max_luminosity(&mut target, max_luminosity_percent);

    if !filter.primed {
        filter.value = target;
        filter.held = target;
        filter.velocity = [0.0; 3];
        filter.primed = true;
        filter.hold_primed = true;
        return rgb_from_f32(target);
    }

    // Hold nearly-identical targets so static UI / paused video cannot blink.
    let hold_distance = color_distance(target, filter.held);
    if filter.hold_primed && hold_distance < 9.0 {
        filter.velocity = [0.0; 3];
        return rgb_from_f32(filter.value);
    }
    if hold_distance > 14.0 || !filter.hold_primed {
        filter.held = target;
        filter.hold_primed = true;
    } else {
        // Soft latch: blend held target so we don't chase noise around the threshold.
        for (held, target_channel) in filter.held.iter_mut().zip(target) {
            *held = *held * 0.72 + target_channel * 0.28;
        }
        target = filter.held;
    }

    let dt = elapsed.as_secs_f32().clamp(0.001, 0.12);
    let alpha = smoothing_alpha(reactivity_percent, elapsed);
    let distance = color_distance(target, filter.value);

    let scene_cut = distance > 160.0;
    let mut predicted = [0.0f32; 3];
    if scene_cut {
        filter.velocity = [0.0; 3];
        predicted = target;
    } else if distance < 6.0 {
        filter.velocity = [0.0; 3];
        predicted = filter.value;
    } else {
        for (index, target_channel) in target.iter().copied().enumerate() {
            let measured_velocity = (target_channel - filter.value[index]) / dt;
            filter.velocity[index] = filter.velocity[index] * (1.0 - VELOCITY_BLEND)
                + measured_velocity * VELOCITY_BLEND;
            // Kill residual velocity noise on quiet scenes.
            if filter.velocity[index].abs() < 8.0 {
                filter.velocity[index] *= 0.35;
            }
            predicted[index] =
                (target_channel + filter.velocity[index] * dt * PREDICTION_LEAD).clamp(0.0, 255.0);
        }
    }

    let scene_alpha = if scene_cut {
        alpha.max(0.55)
    } else if distance > 90.0 {
        alpha.clamp(0.32, 0.48)
    } else if distance > 40.0 {
        alpha * 0.92
    } else {
        alpha * 0.72
    };
    let darkness_alpha = if target.iter().copied().fold(0.0, f32::max) < 4.0 {
        scene_alpha.max(0.58)
    } else {
        scene_alpha
    };

    for (index, predicted_channel) in predicted.iter().copied().enumerate() {
        let delta = predicted_channel - filter.value[index];
        if delta.abs() > 0.35 {
            filter.value[index] += delta * darkness_alpha;
        }
    }
    apply_max_luminosity(&mut filter.value, max_luminosity_percent);
    rgb_from_f32(filter.value)
}

fn color_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn rgb_from_f32(value: [f32; 3]) -> Rgb {
    Rgb {
        r: value[0].round().clamp(0.0, 255.0) as u8,
        g: value[1].round().clamp(0.0, 255.0) as u8,
        b: value[2].round().clamp(0.0, 255.0) as u8,
    }
}

fn apply_max_luminosity(rgb: &mut [f32; 3], max_luminosity_percent: f32) {
    let ceiling = (max_luminosity_percent / 100.0).clamp(0.05, 1.0) * 255.0;
    let peak = rgb[0].max(rgb[1]).max(rgb[2]);
    if peak > ceiling && peak > 1e-3 {
        let scale = ceiling / peak;
        rgb[0] *= scale;
        rgb[1] *= scale;
        rgb[2] *= scale;
    }
}

fn smoothing_alpha(reactivity_percent: f32, elapsed: Duration) -> f32 {
    let response = (reactivity_percent / 100.0).clamp(0.1, 1.0).powf(0.9);
    // 35–232 ms instead of 130–455 ms: the old filter was visibly behind
    // medium transitions even when capture and transport were on time.
    let time_constant_ms = 260.0 - response * 225.0;
    1.0 - (-elapsed.as_secs_f32() * 1_000.0 / time_constant_ms).exp()
}

fn sample_depth(bounds: ContentBounds, edge_depth_percent: f32) -> (u32, u32) {
    let raw_x = ((bounds.width() as f32 * edge_depth_percent / 100.0).round() as u32)
        .clamp(1, bounds.width());
    let raw_y = ((bounds.height() as f32 * edge_depth_percent / 100.0).round() as u32)
        .clamp(1, bounds.height());
    let max_depth_x = ((bounds.width() as f32 * 0.06).round() as u32).clamp(64, 220);
    let max_depth_y = ((bounds.height() as f32 * 0.08).round() as u32).clamp(48, 170);
    (raw_x.min(max_depth_x).max(1), raw_y.min(max_depth_y).max(1))
}

fn zone_rect(zone: Zone, bounds: ContentBounds, edge_depth_percent: f32) -> SampleRect {
    let (depth_x, depth_y) = sample_depth(bounds, edge_depth_percent);
    match zone {
        Zone::Left => SampleRect {
            x_start: bounds.left,
            x_end: bounds.left + depth_x,
            y_start: bounds.top,
            y_end: bounds.bottom,
            zone,
        },
        Zone::Right => SampleRect {
            x_start: bounds.right - depth_x,
            x_end: bounds.right,
            y_start: bounds.top,
            y_end: bounds.bottom,
            zone,
        },
        Zone::Top => SampleRect {
            x_start: bounds.left,
            x_end: bounds.right,
            y_start: bounds.top,
            y_end: bounds.top + depth_y,
            zone,
        },
        Zone::Bottom => SampleRect {
            x_start: bounds.left,
            x_end: bounds.right,
            y_start: bounds.bottom - depth_y,
            y_end: bounds.bottom,
            zone,
        },
        Zone::TopLeft => SampleRect {
            x_start: bounds.left,
            x_end: bounds.left + depth_x,
            y_start: bounds.top,
            y_end: bounds.top + depth_y,
            zone,
        },
        Zone::TopRight => SampleRect {
            x_start: bounds.right - depth_x,
            x_end: bounds.right,
            y_start: bounds.top,
            y_end: bounds.top + depth_y,
            zone,
        },
        Zone::BottomLeft => SampleRect {
            x_start: bounds.left,
            x_end: bounds.left + depth_x,
            y_start: bounds.bottom - depth_y,
            y_end: bounds.bottom,
            zone,
        },
        Zone::BottomRight => SampleRect {
            x_start: bounds.right - depth_x,
            x_end: bounds.right,
            y_start: bounds.bottom - depth_y,
            y_end: bounds.bottom,
            zone,
        },
    }
}

fn sample_zone(
    image: &image::RgbaImage,
    zone: Zone,
    bounds: ContentBounds,
    edge_depth_percent: f32,
    brightness_percent: f32,
    saturation_percent: f32,
    sample_scale: f32,
) -> Rgb {
    if bounds.width() == 0 || bounds.height() == 0 {
        return Rgb::default();
    }
    sample_rect(
        image,
        zone_rect(zone, bounds, edge_depth_percent),
        brightness_percent,
        saturation_percent,
        sample_scale,
    )
}

fn inward_depth(rect: SampleRect, x: u32, y: u32, width: u32, height: u32) -> f32 {
    let from_left = (x - rect.x_start) as f32 / width as f32;
    let from_right = (rect.x_end - 1 - x) as f32 / width as f32;
    let from_top = (y - rect.y_start) as f32 / height as f32;
    let from_bottom = (rect.y_end - 1 - y) as f32 / height as f32;
    match rect.zone {
        Zone::Left => from_left,
        Zone::Right => from_right,
        Zone::Top => from_top,
        Zone::Bottom => from_bottom,
        Zone::TopLeft => (from_left + from_top) * 0.5,
        Zone::TopRight => (from_right + from_top) * 0.5,
        Zone::BottomLeft => (from_left + from_bottom) * 0.5,
        Zone::BottomRight => (from_right + from_bottom) * 0.5,
    }
}

fn edge_weight(rect: SampleRect, x: u32, y: u32, width: u32, height: u32) -> f32 {
    let d = inward_depth(rect, x, y, width, height).clamp(0.0, 1.0);
    0.25 + 1.75 * (1.0 - d).powf(2.2)
}

fn pixel_hsl_weights(r: f32, g: f32, b: f32) -> (f32, f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let chroma = max - min;
    let luminance = (max + min) * 0.5;
    let saturation = if max < 1e-6 { 0.0 } else { chroma / max };
    let hue = if chroma < 1e-6 {
        0.0
    } else if (max - r).abs() < 1e-6 {
        ((g - b) / chroma).rem_euclid(6.0) * 60.0
    } else if (max - g).abs() < 1e-6 {
        ((b - r) / chroma + 2.0) * 60.0
    } else {
        ((r - g) / chroma + 4.0) * 60.0
    };
    (hue.rem_euclid(360.0), saturation, luminance, chroma)
}

fn is_white_ui_pixel(saturation: f32, luminance: f32) -> bool {
    saturation < 0.08 && luminance > 0.78
}

fn sample_rect(
    image: &image::RgbaImage,
    rect: SampleRect,
    brightness_percent: f32,
    saturation_percent: f32,
    sample_scale: f32,
) -> Rgb {
    let width = rect.x_end.saturating_sub(rect.x_start);
    let height = rect.y_end.saturating_sub(rect.y_start);
    if width == 0 || height == 0 {
        return Rgb::default();
    }
    let scale = sample_scale.clamp(0.5, 1.0);
    let target_long = ((TARGET_SAMPLES_LONG as f32) * scale).round().max(24.0) as u32;
    let target_short = ((TARGET_SAMPLES_SHORT as f32) * scale).round().max(12.0) as u32;
    let horizontal = width >= height;
    let cells_x = if horizontal {
        target_long
    } else {
        target_short
    }
    .min(width)
    .max(1);
    let cells_y = if horizontal {
        target_short
    } else {
        target_long
    }
    .min(height)
    .max(1);
    let cell_w = (width as f32 / cells_x as f32).max(1.0);
    let cell_h = (height as f32 / cells_y as f32).max(1.0);
    let linear = srgb_linear_lut();
    let pixels = image.as_raw();
    let stride = image.width() as usize;

    let mut avg = [0.0f32; 3];
    let mut avg_weight = 0.0f32;
    let mut hue_weight = [0.0f32; HUE_BUCKETS];
    let mut hue_rgb = [[0.0f32; 3]; HUE_BUCKETS];
    let mut white_weight = 0.0f32;
    let mut content_weight = 0.0f32;
    let mut sat_mass = 0.0f32;
    let mut sat_sum = 0.0f32;

    for cy in 0..cells_y {
        for cx in 0..cells_x {
            // Fixed cell centers — frame jitter caused static-scene blink.
            let px = rect.x_start
                + ((cx as f32 + 0.5) * cell_w)
                    .floor()
                    .clamp(0.0, (width - 1) as f32) as u32;
            let py = rect.y_start
                + ((cy as f32 + 0.5) * cell_h)
                    .floor()
                    .clamp(0.0, (height - 1) as f32) as u32;
            let x = px.min(rect.x_end - 1);
            let y = py.min(rect.y_end - 1);

            // 2x2 block average inside the cell to reduce aliasing on thin lines.
            let mut block = [0.0f32; 3];
            let mut block_n = 0.0f32;
            for dy in 0..2u32 {
                for dx in 0..2u32 {
                    let sx = (x + dx).min(rect.x_end - 1);
                    let sy = (y + dy).min(rect.y_end - 1);
                    let offset = (sy as usize * stride + sx as usize) * 4;
                    block[0] += linear[pixels[offset] as usize];
                    block[1] += linear[pixels[offset + 1] as usize];
                    block[2] += linear[pixels[offset + 2] as usize];
                    block_n += 1.0;
                }
            }
            let lr = block[0] / block_n;
            let lg = block[1] / block_n;
            let lb = block[2] / block_n;
            let (hue, sat, lum, _chroma) = pixel_hsl_weights(lr, lg, lb);
            let edge = edge_weight(rect, x, y, width, height);
            let saturation_weight = 0.25 + sat * 0.75;
            let luminance_weight = 0.35 + lum.sqrt() * 0.65;
            let mut weight = edge * saturation_weight * luminance_weight;

            if is_white_ui_pixel(sat, lum) {
                white_weight += weight;
                // Soft-reject white UI / subtitles until we know their share of the zone.
                weight *= 0.12;
            } else {
                content_weight += weight;
            }

            avg[0] += lr * weight;
            avg[1] += lg * weight;
            avg[2] += lb * weight;
            avg_weight += weight;

            if sat > 0.05 && lum > 0.04 {
                let bucket = ((hue / 360.0) * HUE_BUCKETS as f32).floor() as usize % HUE_BUCKETS;
                let hist_w = weight * (0.35 + sat * 1.4);
                hue_weight[bucket] += hist_w;
                hue_rgb[bucket][0] += lr * hist_w;
                hue_rgb[bucket][1] += lg * hist_w;
                hue_rgb[bucket][2] += lb * hist_w;
                sat_sum += sat * weight;
                sat_mass += weight;
            }
        }
    }

    if avg_weight <= 1e-6 {
        return Rgb::default();
    }

    // If whites are a minority (subtitles/HUD), re-normalize by discarding most of them.
    let total_seen = white_weight + content_weight;
    if total_seen > 1e-6 && white_weight / total_seen < 0.28 && content_weight > 1e-6 {
        // average already down-weighted whites; keep as-is
    } else if white_weight / total_seen.max(1e-6) >= 0.55 {
        // Mostly white/UI: keep the tempered average so menus stay readable as soft light.
    }

    let average = [
        avg[0] / avg_weight,
        avg[1] / avg_weight,
        avg[2] / avg_weight,
    ];

    let (dominant, dominant_strength) = {
        let mut best_i = 0usize;
        let mut best_w = 0.0f32;
        let mut total_h = 0.0f32;
        for (i, w) in hue_weight.iter().enumerate() {
            total_h += *w;
            if *w > best_w {
                best_w = *w;
                best_i = i;
            }
        }
        if best_w < 1e-6 || total_h < 1e-6 {
            (average, 0.0)
        } else {
            // Blend peak with neighbors so near-ties don't flip hard each frame.
            let prev = (best_i + HUE_BUCKETS - 1) % HUE_BUCKETS;
            let next = (best_i + 1) % HUE_BUCKETS;
            let w_prev = hue_weight[prev] * 0.45;
            let w_next = hue_weight[next] * 0.45;
            let w_peak = best_w;
            let blend_w = w_peak + w_prev + w_next;
            (
                [
                    (hue_rgb[best_i][0] + hue_rgb[prev][0] * 0.45 + hue_rgb[next][0] * 0.45)
                        / blend_w,
                    (hue_rgb[best_i][1] + hue_rgb[prev][1] * 0.45 + hue_rgb[next][1] * 0.45)
                        / blend_w,
                    (hue_rgb[best_i][2] + hue_rgb[prev][2] * 0.45 + hue_rgb[next][2] * 0.45)
                        / blend_w,
                ],
                (best_w / total_h).clamp(0.0, 1.0),
            )
        }
    };

    let mean_sat = if sat_mass > 1e-6 {
        sat_sum / sat_mass
    } else {
        0.0
    };
    // Dynamic blend: favor dominant when a clear saturated hue wins the histogram.
    let dominant_mix = (0.22 + dominant_strength * 0.55 + mean_sat * 0.28).clamp(0.18, 0.78);

    let blended = [
        average[0] * (1.0 - dominant_mix) + dominant[0] * dominant_mix,
        average[1] * (1.0 - dominant_mix) + dominant[1] * dominant_mix,
        average[2] * (1.0 - dominant_mix) + dominant[2] * dominant_mix,
    ];

    let mut rgb = [
        linear_to_srgb(blended[0]),
        linear_to_srgb(blended[1]),
        linear_to_srgb(blended[2]),
    ];
    let luminance = rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
    let saturation = (saturation_percent / 100.0).clamp(0.0, 1.5);
    let brightness = (brightness_percent / 100.0).clamp(0.0, 1.0);
    for channel in &mut rgb {
        *channel = (luminance + (*channel - luminance) * saturation) * brightness;
    }
    Rgb {
        r: rgb[0].round().clamp(0.0, 255.0) as u8,
        g: rgb[1].round().clamp(0.0, 255.0) as u8,
        b: rgb[2].round().clamp(0.0, 255.0) as u8,
    }
}

fn detect_content_bounds(image: &image::RgbaImage) -> ContentBounds {
    let full = ContentBounds::full(image);
    if image.width() < 80 || image.height() < 80 {
        return full;
    }
    let top = find_content_offset(image, ScanEdge::Top);
    let bottom = find_content_offset(image, ScanEdge::Bottom);
    let left = find_content_offset(image, ScanEdge::Left);
    let right = find_content_offset(image, ScanEdge::Right);
    let mut bounds = full;

    if symmetric_bars(top, bottom) && top + bottom < image.height() / 2 {
        bounds.top = top;
        bounds.bottom = image.height() - bottom;
    }
    if symmetric_bars(left, right) && left + right < image.width() / 2 {
        bounds.left = left;
        bounds.right = image.width() - right;
    }
    bounds
}

fn symmetric_bars(first: u32, second: u32) -> bool {
    let largest = first.max(second);
    largest >= 3 && first.abs_diff(second) <= (largest / 3).max(3)
}

#[derive(Clone, Copy)]
enum ScanEdge {
    Left,
    Right,
    Top,
    Bottom,
}

fn find_content_offset(image: &image::RgbaImage, edge: ScanEdge) -> u32 {
    let perpendicular = match edge {
        ScanEdge::Top | ScanEdge::Bottom => image.height(),
        ScanEdge::Left | ScanEdge::Right => image.width(),
    };
    let step = (perpendicular / 120).max(2);
    let limit = perpendicular / 4;
    for offset in (0..limit).step_by(step as usize) {
        if line_has_content(image, edge, offset) {
            return offset;
        }
    }
    0
}

fn line_has_content(image: &image::RgbaImage, edge: ScanEdge, offset: u32) -> bool {
    let along = match edge {
        ScanEdge::Top | ScanEdge::Bottom => image.width(),
        ScanEdge::Left | ScanEdge::Right => image.height(),
    };
    let step = (along / 96).max(1);
    let pixels = image.as_raw();
    let stride = image.width() as usize;
    let mut sum = 0.0f32;
    let mut sum_sq = 0.0f32;
    let mut chroma_sum = 0.0f32;
    let mut samples = 0u32;
    let mut luminances = [0u8; 96];
    let mut luma_count = 0usize;

    for position in (0..along).step_by(step as usize) {
        let (x, y) = match edge {
            ScanEdge::Top => (position, offset),
            ScanEdge::Bottom => (position, image.height() - 1 - offset),
            ScanEdge::Left => (offset, position),
            ScanEdge::Right => (image.width() - 1 - offset, position),
        };
        let index = (y as usize * stride + x as usize) * 4;
        let r = pixels[index] as f32;
        let g = pixels[index + 1] as f32;
        let b = pixels[index + 2] as f32;
        let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let chroma = r.max(g).max(b) - r.min(g).min(b);
        sum += luma;
        sum_sq += luma * luma;
        chroma_sum += chroma;
        if luma_count < luminances.len() {
            luminances[luma_count] = luma.round().clamp(0.0, 255.0) as u8;
            luma_count += 1;
        }
        samples += 1;
    }

    let n = samples.max(1) as f32;
    let mean = sum / n;
    let variance = (sum_sq / n - mean * mean).max(0.0);
    let mean_chroma = chroma_sum / n;
    // Approximate P95 from the sampled luminances.
    let mut sorted = luminances[..luma_count].to_vec();
    sorted.sort_unstable();
    let p95 = if sorted.is_empty() {
        0.0
    } else {
        sorted[((sorted.len() as f32 * 0.95).floor() as usize).min(sorted.len() - 1)] as f32
    };

    // True letterbox: near-black, flat, low chroma. Dark scenes have variance/P95.
    let looks_like_bar = mean < 6.5 && variance < 18.0 && p95 < 14.0 && mean_chroma < 8.0;
    !looks_like_bar && (mean > 9.0 || variance > 40.0 || p95 > 22.0 || mean_chroma > 14.0)
}

fn srgb_linear_lut() -> &'static [f32; 256] {
    static LUT: OnceLock<[f32; 256]> = OnceLock::new();
    LUT.get_or_init(|| {
        std::array::from_fn(|index| {
            let value = index as f32 / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        })
    })
}

fn linear_to_srgb(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    let encoded = if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    encoded * 255.0
}

pub fn validate_assignments(assignments: &[ChannelAssignment]) -> Result<(), String> {
    if assignments.is_empty() {
        return Err("Aucune lumière n’est affectée à l’écran.".to_owned());
    }
    let mut channels = std::collections::HashSet::new();
    for assignment in assignments {
        if !channels.insert(assignment.channel_id) {
            return Err("Une lumière apparaît plusieurs fois dans la configuration.".to_owned());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    #[test]
    fn samples_only_requested_corner() {
        let mut image = RgbaImage::from_pixel(100, 60, Rgba([0, 0, 255, 255]));
        for y in 0..12 {
            for x in 0..20 {
                image.put_pixel(x, y, Rgba([255, 0, 0, 255]));
            }
        }
        let bounds = ContentBounds::full(&image);
        let corner = sample_zone(&image, Zone::TopLeft, bounds, 20.0, 100.0, 100.0, 1.0);
        let opposite = sample_zone(&image, Zone::BottomRight, bounds, 20.0, 100.0, 100.0, 1.0);
        assert!(corner.r > 240 && corner.b < 10);
        assert!(opposite.b > 240 && opposite.r < 10);
    }

    #[test]
    fn samples_only_requested_edge() {
        let mut image = RgbaImage::from_pixel(100, 60, Rgba([0, 0, 255, 255]));
        for y in 0..60 {
            for x in 0..20 {
                image.put_pixel(x, y, Rgba([255, 0, 0, 255]));
            }
        }
        let bounds = ContentBounds::full(&image);
        let left = sample_zone(&image, Zone::Left, bounds, 10.0, 100.0, 100.0, 1.0);
        let right = sample_zone(&image, Zone::Right, bounds, 10.0, 100.0, 100.0, 1.0);
        assert!(left.r > 240 && left.b < 10);
        assert!(right.b > 240 && right.r < 10);
    }

    #[test]
    fn brightness_scales_sample() {
        let image = RgbaImage::from_pixel(20, 20, Rgba([200, 100, 50, 255]));
        let bounds = ContentBounds::full(&image);
        let full = sample_zone(&image, Zone::Top, bounds, 20.0, 100.0, 100.0, 1.0);
        let half = sample_zone(&image, Zone::Top, bounds, 20.0, 50.0, 100.0, 1.0);
        assert!((half.r as i16 - full.r as i16 / 2).abs() <= 1);
    }

    #[test]
    fn detects_symmetric_letterbox_bars() {
        let mut image = RgbaImage::from_pixel(160, 100, Rgba([0, 0, 0, 255]));
        for y in 12..88 {
            for x in 0..160 {
                image.put_pixel(x, y, Rgba([180, 40, 20, 255]));
            }
        }
        let bounds = detect_content_bounds(&image);
        assert_eq!(bounds.top, 12);
        assert_eq!(bounds.bottom, 88);
        let top = sample_zone(&image, Zone::Top, bounds, 10.0, 100.0, 100.0, 1.0);
        assert!(top.r > 170 && top.g < 50);
    }

    #[test]
    fn smoothing_is_frame_rate_independent() {
        let alpha_16 = smoothing_alpha(70.0, Duration::from_millis(16));
        let alpha_32 = smoothing_alpha(70.0, Duration::from_millis(32));
        let two_steps = 1.0 - (1.0 - alpha_16).powi(2);
        assert!((two_steps - alpha_32).abs() < 0.0001);
    }

    #[test]
    fn predictive_filter_leads_a_rising_target() {
        let mut filter = ChannelFilter::default();
        let _ = smooth_color(
            &mut filter,
            Rgb {
                r: 10,
                g: 10,
                b: 10,
            },
            80.0,
            100.0,
            Duration::from_millis(16),
        );
        let first = smooth_color(
            &mut filter,
            Rgb {
                r: 40,
                g: 40,
                b: 40,
            },
            80.0,
            100.0,
            Duration::from_millis(16),
        );
        let second = smooth_color(
            &mut filter,
            Rgb {
                r: 70,
                g: 70,
                b: 70,
            },
            80.0,
            100.0,
            Duration::from_millis(16),
        );
        assert!(first.r > 10);
        assert!(second.r > first.r);
        assert!(second.r as i16 - 70 <= 12);
    }

    #[test]
    fn black_bar_tracking_ignores_single_dark_frame() {
        let full = ContentBounds {
            left: 0,
            top: 0,
            right: 160,
            bottom: 100,
        };
        let cropped = ContentBounds {
            left: 0,
            top: 12,
            right: 160,
            bottom: 88,
        };
        let mut tracker = BoundsTracker::new(full);
        assert_eq!(tracker.update(cropped), full);
        assert_eq!(tracker.update(cropped), full);
        assert_eq!(tracker.update(cropped), cropped);
        assert_eq!(tracker.update(full), cropped);
    }

    #[test]
    fn hybrid_sampling_prefers_dominant_hue() {
        let mut image = RgbaImage::from_pixel(80, 40, Rgba([0, 0, 255, 255]));
        for y in 0..40 {
            for x in 0..24 {
                image.put_pixel(x, y, Rgba([255, 0, 0, 255]));
            }
        }
        let bounds = ContentBounds::full(&image);
        let color = sample_zone(&image, Zone::Top, bounds, 50.0, 100.0, 100.0, 1.0);
        // Majority blue should win over a pure average toward purple/magenta.
        assert!(color.b > color.r + 20);
        assert!(color.b > 140);
    }

    #[test]
    fn white_subtitles_do_not_wash_out_scene() {
        let mut image = RgbaImage::from_pixel(120, 40, Rgba([10, 40, 180, 255]));
        for y in 32..40 {
            for x in 20..100 {
                image.put_pixel(x, y, Rgba([250, 250, 250, 255]));
            }
        }
        let bounds = ContentBounds::full(&image);
        let color = sample_zone(&image, Zone::Bottom, bounds, 40.0, 100.0, 100.0, 1.0);
        assert!(color.b > 90);
        assert!(color.r < 160);
    }

    #[test]
    fn sample_depth_scales_with_resolution() {
        let hd = ContentBounds {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let uhd = ContentBounds {
            left: 0,
            top: 0,
            right: 3840,
            bottom: 2160,
        };
        let (hd_x, hd_y) = sample_depth(hd, 10.0);
        let (uhd_x, uhd_y) = sample_depth(uhd, 10.0);
        assert!(uhd_x > hd_x);
        assert!(uhd_y > hd_y);
        assert!((140..=220).contains(&uhd_x));
        assert!((100..=170).contains(&uhd_y));
    }

    #[test]
    fn scene_cut_resets_velocity_without_overshoot() {
        let mut filter = ChannelFilter {
            value: [20.0, 20.0, 20.0],
            velocity: [2_000.0, 2_000.0, 2_000.0],
            primed: true,
            held: [20.0, 20.0, 20.0],
            hold_primed: true,
        };
        let next = smooth_color(
            &mut filter,
            Rgb {
                r: 220,
                g: 30,
                b: 30,
            },
            90.0,
            100.0,
            Duration::from_millis(16),
        );
        assert!(filter.velocity.iter().all(|v| v.abs() < 1e-3));
        assert!(next.r <= 220);
        assert!(next.r > 100);
    }

    #[test]
    fn static_noise_does_not_blink_output() {
        let mut filter = ChannelFilter::default();
        let base = Rgb {
            r: 80,
            g: 40,
            b: 120,
        };
        let _ = smooth_color(&mut filter, base, 90.0, 100.0, Duration::from_millis(16));
        let a = smooth_color(
            &mut filter,
            Rgb {
                r: 82,
                g: 38,
                b: 123,
            },
            90.0,
            100.0,
            Duration::from_millis(16),
        );
        let b = smooth_color(
            &mut filter,
            Rgb {
                r: 78,
                g: 42,
                b: 118,
            },
            90.0,
            100.0,
            Duration::from_millis(16),
        );
        assert_eq!(a.r, b.r);
        assert_eq!(a.g, b.g);
        assert_eq!(a.b, b.b);
    }

    #[test]
    fn max_luminosity_caps_peak_channel() {
        let mut filter = ChannelFilter::default();
        let capped = smooth_color(
            &mut filter,
            Rgb {
                r: 255,
                g: 200,
                b: 40,
            },
            100.0,
            50.0,
            Duration::from_millis(16),
        );
        assert!(capped.r <= 128);
        assert!(capped.g <= 128);
        assert!(capped.r >= capped.g);
    }

    #[test]
    fn dark_noisy_row_is_not_letterbox() {
        let mut image = RgbaImage::from_pixel(160, 40, Rgba([0, 0, 0, 255]));
        for x in (0..160).step_by(7) {
            image.put_pixel(x, 0, Rgba([28, 24, 18, 255]));
        }
        assert!(line_has_content(&image, ScanEdge::Top, 0));
        let black = RgbaImage::from_pixel(160, 40, Rgba([0, 0, 0, 255]));
        assert!(!line_has_content(&black, ScanEdge::Top, 0));
    }
}
