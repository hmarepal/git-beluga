//! Encodes frames as a looping GIF or a PNG, doubling each pixel so they stay crisp.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use crate::whale::{Frame, Rgb};

const SCALE: usize = 2;

/// Each frame shows for `delay_cs` hundredths of a second.
pub fn encode_gif(
    frames: &[Frame],
    width: usize,
    height: usize,
    delay_cs: u16,
) -> Result<Vec<u8>, String> {
    let mut seen = HashSet::new();
    for frame in frames {
        seen.extend(frame.iter().flatten().copied());
    }
    // Index 0 stays transparent. A scene rarely nears 255 colors, but an account with very
    // many languages could, so colors coarsen until they fit instead of failing the run.
    let mut bits = 0;
    let coarse = |c: Rgb, bits: u32| {
        let mask = 0xffu8 << bits;
        Rgb(c.0 & mask, c.1 & mask, c.2 & mask)
    };
    let palette: Vec<Rgb> = loop {
        let mut unique: Vec<Rgb> = seen.iter().map(|&c| coarse(c, bits)).collect();
        unique.sort_by_key(|c| (c.0, c.1, c.2));
        unique.dedup();
        if unique.len() <= 255 {
            break unique;
        }
        bits += 1;
    };
    let index: HashMap<Rgb, u8> = palette
        .iter()
        .enumerate()
        .map(|(i, &c)| (c, i as u8 + 1))
        .collect();
    let mut flat = vec![0, 0, 0];
    for c in &palette {
        flat.extend([c.0, c.1, c.2]);
    }
    let (w, h) = (width * SCALE, height * SCALE);
    let mut out = Vec::new();
    {
        let mut encoder =
            gif::Encoder::new(&mut out, w as u16, h as u16, &flat).map_err(|e| e.to_string())?;
        encoder
            .set_repeat(gif::Repeat::Infinite)
            .map_err(|e| e.to_string())?;
        for frame in frames {
            let mut buffer = vec![0u8; w * h];
            for y in 0..h {
                for x in 0..w {
                    if let Some(c) = frame[(y / SCALE) * width + x / SCALE] {
                        buffer[y * w + x] = index[&coarse(c, bits)];
                    }
                }
            }
            encoder
                .write_frame(&gif::Frame {
                    width: w as u16,
                    height: h as u16,
                    delay: delay_cs,
                    dispose: gif::DisposalMethod::Background,
                    transparent: Some(0),
                    buffer: Cow::Owned(buffer),
                    ..gif::Frame::default()
                })
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(out)
}

pub fn encode_png(
    frame: &Frame,
    width: usize,
    height: usize,
    background: Option<Rgb>,
) -> Result<Vec<u8>, String> {
    let (w, h) = (width * SCALE, height * SCALE);
    let mut data = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            match frame[(y / SCALE) * width + x / SCALE].or(background) {
                Some(c) => data.extend([c.0, c.1, c.2, 255]),
                None => data.extend([0, 0, 0, 0]),
            }
        }
    }
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, w as u32, h as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(&data).map_err(|e| e.to_string())?;
    }
    Ok(out)
}
