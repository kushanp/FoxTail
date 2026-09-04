use egui::IconData;

/// Tiny fox-head icon for the window chrome.
pub fn app_icon() -> IconData {
    const S: u32 = 64;
    let mut rgba = vec![0u8; (S * S * 4) as usize];

    let orange = [232, 122, 46, 255];
    let dark_orange = [168, 72, 24, 255];
    let cream = [250, 236, 214, 255];
    let ink = [32, 22, 16, 255];
    let white = [255, 255, 255, 255];

    fill_circle(&mut rgba, S, 30, 34, 18, orange);
    fill_circle(&mut rgba, S, 30, 36, 16, orange);

    fill_triangle(&mut rgba, S, 16, 28, 24, 8, 28, 28, orange);
    fill_triangle(&mut rgba, S, 32, 28, 36, 8, 44, 28, orange);
    fill_triangle(&mut rgba, S, 19, 26, 24, 12, 27, 26, dark_orange);
    fill_triangle(&mut rgba, S, 33, 26, 36, 12, 41, 26, dark_orange);

    fill_circle(&mut rgba, S, 52, 42, 11, orange);
    fill_circle(&mut rgba, S, 58, 38, 6, cream);

    fill_circle(&mut rgba, S, 30, 40, 9, cream);
    fill_circle(&mut rgba, S, 24, 30, 3, ink);
    fill_circle(&mut rgba, S, 36, 30, 3, ink);
    fill_circle(&mut rgba, S, 24, 29, 1, white);
    fill_circle(&mut rgba, S, 36, 29, 1, white);
    fill_circle(&mut rgba, S, 30, 42, 2, ink);

    IconData {
        rgba,
        width: S,
        height: S,
    }
}

fn put(rgba: &mut [u8], s: u32, x: i32, y: i32, c: [u8; 4]) {
    if x < 0 || y < 0 || x >= s as i32 || y >= s as i32 {
        return;
    }
    let i = ((y as u32 * s + x as u32) * 4) as usize;
    let a = c[3] as u32;
    if a == 0 {
        return;
    }
    if a == 255 {
        rgba[i..i + 4].copy_from_slice(&c);
        return;
    }
    let ia = 255 - a;
    rgba[i] = ((c[0] as u32 * a + rgba[i] as u32 * ia) / 255) as u8;
    rgba[i + 1] = ((c[1] as u32 * a + rgba[i + 1] as u32 * ia) / 255) as u8;
    rgba[i + 2] = ((c[2] as u32 * a + rgba[i + 2] as u32 * ia) / 255) as u8;
    rgba[i + 3] = 255;
}

fn fill_circle(rgba: &mut [u8], s: u32, cx: i32, cy: i32, r: i32, c: [u8; 4]) {
    let r2 = r * r;
    for y in (cy - r)..=(cy + r) {
        for x in (cx - r)..=(cx + r) {
            let dx = x - cx;
            let dy = y - cy;
            if dx * dx + dy * dy <= r2 {
                put(rgba, s, x, y, c);
            }
        }
    }
}

fn fill_triangle(
    rgba: &mut [u8],
    s: u32,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    x3: i32,
    y3: i32,
    c: [u8; 4],
) {
    let min_x = x1.min(x2).min(x3);
    let max_x = x1.max(x2).max(x3);
    let min_y = y1.min(y2).min(y3);
    let max_y = y1.max(y2).max(y3);
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            if point_in_triangle(x, y, x1, y1, x2, y2, x3, y3) {
                put(rgba, s, x, y, c);
            }
        }
    }
}

fn sign(x1: i32, y1: i32, x2: i32, y2: i32, x3: i32, y3: i32) -> i32 {
    (x1 - x3) * (y2 - y3) - (x2 - x3) * (y1 - y3)
}

fn point_in_triangle(x: i32, y: i32, x1: i32, y1: i32, x2: i32, y2: i32, x3: i32, y3: i32) -> bool {
    let d1 = sign(x, y, x1, y1, x2, y2);
    let d2 = sign(x, y, x2, y2, x3, y3);
    let d3 = sign(x, y, x3, y3, x1, y1);
    let has_neg = d1 < 0 || d2 < 0 || d3 < 0;
    let has_pos = d1 > 0 || d2 > 0 || d3 > 0;
    !(has_neg && has_pos)
}
