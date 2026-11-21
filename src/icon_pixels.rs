// Fox-head icon artwork, shared by the window icon (src/icon.rs) and the
// embedded .exe icon (build.rs, via `include!`). Keep this file free of
// crate dependencies so the build script can compile it on its own.

/// Logical canvas size; all shape coordinates below are in this space.
pub const ICON_BASE: u32 = 64;

/// Render the fox head at `ICON_BASE * scale` pixels square, as straight RGBA.
/// Each output pixel is sampled at its centre, so `scale == 1` gives the
/// original 64x64 artwork and larger scales give the same shapes, sharper.
pub fn render_fox(scale: u32) -> (u32, Vec<u8>) {
    let s = ICON_BASE * scale;
    let mut c = Canvas {
        s,
        k: scale as f32,
        rgba: vec![0u8; (s * s * 4) as usize],
    };

    let orange = [232, 122, 46, 255];
    let dark_orange = [168, 72, 24, 255];
    let cream = [250, 236, 214, 255];
    let ink = [32, 22, 16, 255];
    let white = [255, 255, 255, 255];

    c.circle(30.0, 34.0, 18.0, orange);
    c.circle(30.0, 36.0, 16.0, orange);

    c.triangle([16.0, 28.0], [24.0, 8.0], [28.0, 28.0], orange);
    c.triangle([32.0, 28.0], [36.0, 8.0], [44.0, 28.0], orange);
    c.triangle([19.0, 26.0], [24.0, 12.0], [27.0, 26.0], dark_orange);
    c.triangle([33.0, 26.0], [36.0, 12.0], [41.0, 26.0], dark_orange);

    c.circle(52.0, 42.0, 11.0, orange);
    c.circle(58.0, 38.0, 6.0, cream);

    c.circle(30.0, 40.0, 9.0, cream);
    c.circle(24.0, 30.0, 3.0, ink);
    c.circle(36.0, 30.0, 3.0, ink);
    c.circle(24.0, 29.0, 1.0, white);
    c.circle(36.0, 29.0, 1.0, white);
    c.circle(30.0, 42.0, 2.0, ink);

    (s, c.rgba)
}

struct Canvas {
    s: u32,
    k: f32,
    rgba: Vec<u8>,
}

impl Canvas {
    /// Pixel centre of output pixel `i`, in logical units.
    fn logical(&self, i: u32) -> f32 {
        (i as f32 + 0.5) / self.k - 0.5
    }

    /// Output pixel range covering logical interval [lo, hi].
    fn span(&self, lo: f32, hi: f32) -> (u32, u32) {
        let a = ((lo - 1.0) * self.k).floor().max(0.0) as u32;
        let b = (((hi + 1.0) * self.k).ceil() as u32).min(self.s);
        (a, b)
    }

    fn put(&mut self, x: u32, y: u32, c: [u8; 4]) {
        let i = ((y * self.s + x) * 4) as usize;
        self.rgba[i..i + 4].copy_from_slice(&c);
    }

    fn circle(&mut self, cx: f32, cy: f32, r: f32, c: [u8; 4]) {
        let (x0, x1) = self.span(cx - r, cx + r);
        let (y0, y1) = self.span(cy - r, cy + r);
        for y in y0..y1 {
            let dy = self.logical(y) - cy;
            for x in x0..x1 {
                let dx = self.logical(x) - cx;
                if dx * dx + dy * dy <= r * r {
                    self.put(x, y, c);
                }
            }
        }
    }

    fn triangle(&mut self, a: [f32; 2], b: [f32; 2], d: [f32; 2], c: [u8; 4]) {
        let (x0, x1) = self.span(a[0].min(b[0]).min(d[0]), a[0].max(b[0]).max(d[0]));
        let (y0, y1) = self.span(a[1].min(b[1]).min(d[1]), a[1].max(b[1]).max(d[1]));
        for y in y0..y1 {
            let py = self.logical(y);
            for x in x0..x1 {
                let p = [self.logical(x), py];
                let d1 = edge(p, a, b);
                let d2 = edge(p, b, d);
                let d3 = edge(p, d, a);
                let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
                let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
                if !(neg && pos) {
                    self.put(x, y, c);
                }
            }
        }
    }
}

fn edge(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    (p[0] - b[0]) * (a[1] - b[1]) - (a[0] - b[0]) * (p[1] - b[1])
}

/// Box-filter a square RGBA image down by an integer `factor`, averaging in
/// premultiplied alpha so edges against transparency stay clean.
#[allow(dead_code)]
pub fn downsample(src: &[u8], src_size: u32, factor: u32) -> (u32, Vec<u8>) {
    let dst_size = src_size / factor;
    let mut out = vec![0u8; (dst_size * dst_size * 4) as usize];
    let n = (factor * factor) as u64;
    for dy in 0..dst_size {
        for dx in 0..dst_size {
            let (mut r, mut g, mut b, mut a) = (0u64, 0u64, 0u64, 0u64);
            for sy in dy * factor..(dy + 1) * factor {
                for sx in dx * factor..(dx + 1) * factor {
                    let i = ((sy * src_size + sx) * 4) as usize;
                    let pa = src[i + 3] as u64;
                    r += src[i] as u64 * pa;
                    g += src[i + 1] as u64 * pa;
                    b += src[i + 2] as u64 * pa;
                    a += pa;
                }
            }
            if a == 0 {
                continue; // fully transparent; leave zeroed
            }
            let o = ((dy * dst_size + dx) * 4) as usize;
            out[o] = ((r + a / 2) / a) as u8;
            out[o + 1] = ((g + a / 2) / a) as u8;
            out[o + 2] = ((b + a / 2) / a) as u8;
            out[o + 3] = ((a + n / 2) / n) as u8;
        }
    }
    (dst_size, out)
}
