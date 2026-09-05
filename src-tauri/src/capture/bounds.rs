use super::frame::ScreenFrame;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ContentBounds {
    pub(super) left: u32,
    pub(super) top: u32,
    pub(super) right: u32,
    pub(super) bottom: u32,
}

pub(super) struct BoundsTracker {
    pub(super) full: ContentBounds,
    pub(super) stable: ContentBounds,
    candidate: ContentBounds,
    candidate_hits: u8,
    full_misses: u8,
}

impl BoundsTracker {
    pub(super) fn new(full: ContentBounds) -> Self {
        Self {
            full,
            stable: full,
            candidate: full,
            candidate_hits: 0,
            full_misses: 0,
        }
    }

    pub(super) fn update(&mut self, detected: ContentBounds) -> ContentBounds {
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
    pub(super) fn full(image: &ScreenFrame) -> Self {
        Self {
            left: 0,
            top: 0,
            right: image.width,
            bottom: image.height,
        }
    }

    pub(super) fn width(self) -> u32 {
        self.right.saturating_sub(self.left)
    }

    pub(super) fn height(self) -> u32 {
        self.bottom.saturating_sub(self.top)
    }
}

pub(super) fn detect_content_bounds(image: &ScreenFrame) -> ContentBounds {
    let full = ContentBounds::full(image);
    if image.width < 80 || image.height < 80 {
        return full;
    }
    let top = find_content_offset(image, ScanEdge::Top);
    let bottom = find_content_offset(image, ScanEdge::Bottom);
    let left = find_content_offset(image, ScanEdge::Left);
    let right = find_content_offset(image, ScanEdge::Right);
    let mut bounds = full;

    if symmetric_bars(top, bottom) && top + bottom < image.height / 2 {
        bounds.top = top;
        bounds.bottom = image.height - bottom;
    }
    if symmetric_bars(left, right) && left + right < image.width / 2 {
        bounds.left = left;
        bounds.right = image.width - right;
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

fn find_content_offset(image: &ScreenFrame, edge: ScanEdge) -> u32 {
    let perpendicular = match edge {
        ScanEdge::Top | ScanEdge::Bottom => image.height,
        ScanEdge::Left | ScanEdge::Right => image.width,
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

fn line_has_content(image: &ScreenFrame, edge: ScanEdge, offset: u32) -> bool {
    let along = match edge {
        ScanEdge::Top | ScanEdge::Bottom => image.width,
        ScanEdge::Left | ScanEdge::Right => image.height,
    };
    let step = (along / 96).max(1);
    let mut sum = 0.0f32;
    let mut sum_sq = 0.0f32;
    let mut chroma_sum = 0.0f32;
    let mut samples = 0u32;
    let mut luminances = [0u8; 96];
    let mut luma_count = 0usize;

    for position in (0..along).step_by(step as usize) {
        let (x, y) = match edge {
            ScanEdge::Top => (position, offset),
            ScanEdge::Bottom => (position, image.height - 1 - offset),
            ScanEdge::Left => (offset, position),
            ScanEdge::Right => (image.width - 1 - offset, position),
        };
        let [r, g, b] = image.rgb(x, y).map(|v| v as f32);
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
    let sorted = &mut luminances[..luma_count];
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
