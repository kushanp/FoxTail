// Embeds the fox icon into foxtail.exe so Explorer, the taskbar pin, and
// shortcuts show it. The artwork comes from src/icon_pixels.rs, the same code
// that draws the runtime window icon.

use std::env;
use std::fs;
use std::path::PathBuf;

include!("src/icon_pixels.rs");

/// Icon sizes Explorer picks from (list view, details, tiles, large icons).
const SIZES: [u32; 6] = [16, 24, 32, 48, 64, 256];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/icon_pixels.rs");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let ico_path = out_dir.join("foxtail.ico");
    fs::write(&ico_path, build_ico()).expect("write foxtail.ico");

    let rc_path = out_dir.join("foxtail.rc");
    let ico_literal = ico_path.display().to_string().replace('\\', "\\\\");
    // Resource ID 1 is the lowest, so Explorer uses it as the exe's icon.
    fs::write(&rc_path, format!("1 ICON \"{ico_literal}\"\n")).expect("write foxtail.rc");

    embed_resource::compile(&rc_path, embed_resource::NONE)
        .manifest_optional()
        .expect("compile icon resource");
}

/// Render each size from one large supersampled image and pack them as a
/// .ico whose entries are 32-bit BGRA DIBs.
fn build_ico() -> Vec<u8> {
    // 768 = 64 * 12 divides evenly by every size in SIZES.
    let (big, master) = render_fox(12);
    let images: Vec<(u32, Vec<u8>)> = SIZES
        .iter()
        .map(|&size| downsample(&master, big, big / size))
        .collect();

    let dibs: Vec<Vec<u8>> = images.iter().map(|(s, rgba)| dib(*s, rgba)).collect();

    let mut ico = Vec::new();
    // ICONDIR
    ico.extend_from_slice(&0u16.to_le_bytes());
    ico.extend_from_slice(&1u16.to_le_bytes());
    ico.extend_from_slice(&(images.len() as u16).to_le_bytes());

    let mut offset = 6 + 16 * images.len() as u32;
    for ((size, _), data) in images.iter().zip(&dibs) {
        // ICONDIRENTRY; a dimension of 256 is stored as 0.
        let dim = if *size >= 256 { 0 } else { *size as u8 };
        ico.push(dim);
        ico.push(dim);
        ico.push(0); // palette colours
        ico.push(0); // reserved
        ico.extend_from_slice(&1u16.to_le_bytes()); // planes
        ico.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        ico.extend_from_slice(&(data.len() as u32).to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for data in &dibs {
        ico.extend_from_slice(data);
    }
    ico
}

/// BITMAPINFOHEADER + bottom-up BGRA pixels + empty AND mask.
fn dib(size: u32, rgba: &[u8]) -> Vec<u8> {
    let mask_row = size.div_ceil(32) * 4;
    let mut out = Vec::new();
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(size as i32).to_le_bytes());
    out.extend_from_slice(&(size as i32 * 2).to_le_bytes()); // XOR + AND masks
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    out.extend_from_slice(&(size * size * 4).to_le_bytes());
    out.extend_from_slice(&[0u8; 16]); // resolution, colour counts

    for y in (0..size).rev() {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            out.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
        }
    }
    out.resize(out.len() + (mask_row * size) as usize, 0);
    out
}
