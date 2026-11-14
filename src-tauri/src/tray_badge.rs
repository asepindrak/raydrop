use image::{Rgba, RgbaImage};
use std::io::Cursor;
#[allow(unused_imports)]
use tauri::{AppHandle, Manager};

// 3x5 bitmap glyphs for digits 0-9 and '+'
const GLYPH_WIDTH: usize = 3;
const GLYPH_HEIGHT: usize = 5;

const GLYPHS: [&[u8; 15]; 11] = [
    // '0'
    &[
        1, 1, 1,
        1, 0, 1,
        1, 0, 1,
        1, 0, 1,
        1, 1, 1,
    ],
    // '1'
    &[
        0, 1, 0,
        1, 1, 0,
        0, 1, 0,
        0, 1, 0,
        1, 1, 1,
    ],
    // '2'
    &[
        1, 1, 1,
        0, 0, 1,
        1, 1, 1,
        1, 0, 0,
        1, 1, 1,
    ],
    // '3'
    &[
        1, 1, 1,
        0, 0, 1,
        1, 1, 1,
        0, 0, 1,
        1, 1, 1,
    ],
    // '4'
    &[
        1, 0, 1,
        1, 0, 1,
        1, 1, 1,
        0, 0, 1,
        0, 0, 1,
    ],
    // '5'
    &[
        1, 1, 1,
        1, 0, 0,
        1, 1, 1,
        0, 0, 1,
        1, 1, 1,
    ],
    // '6'
    &[
        1, 1, 1,
        1, 0, 0,
        1, 1, 1,
        1, 0, 1,
        1, 1, 1,
    ],
    // '7'
    &[
        1, 1, 1,
        0, 0, 1,
        0, 1, 0,
        0, 1, 0,
        0, 1, 0,
    ],
    // '8'
    &[
        1, 1, 1,
        1, 0, 1,
        1, 1, 1,
        1, 0, 1,
        1, 1, 1,
    ],
    // '9'
    &[
        1, 1, 1,
        1, 0, 1,
        1, 1, 1,
        0, 0, 1,
        1, 1, 1,
    ],
    // '+' (index 10)
    &[
        0, 0, 0,
        0, 1, 0,
        1, 1, 1,
        0, 1, 0,
        0, 0, 0,
    ],
];

fn draw_glyph(img: &mut RgbaImage, start_x: i32, start_y: i32, glyph_idx: usize, color: Rgba<u8>) {
    if glyph_idx >= GLYPHS.len() {
        return;
    }
    let glyph = GLYPHS[glyph_idx];
    for row in 0..GLYPH_HEIGHT {
        for col in 0..GLYPH_WIDTH {
            if glyph[row * GLYPH_WIDTH + col] == 1 {
                let px = start_x + col as i32;
                let py = start_y + row as i32;
                if px >= 0 && px < img.width() as i32 && py >= 0 && py < img.height() as i32 {
                    img.put_pixel(px as u32, py as u32, color);
                }
            }
        }
    }
}

pub fn render_badged_tray_icon(count: u32) -> Option<tauri::image::Image<'static>> {
    if count == 0 {
        return tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png")).ok();
    }

    let mut img = image::load_from_memory(include_bytes!("../icons/32x32.png"))
        .ok()?
        .to_rgba8();

    let bg_red = Rgba([239, 68, 68, 255]);   // #EF4444 vivid coral red
    let border_white = Rgba([255, 255, 255, 255]);
    let text_white = Rgba([255, 255, 255, 255]);

    if count <= 9 {
        // Single digit: circular badge centered at (23.5, 8.5)
        let cx: f32 = 23.5;
        let cy: f32 = 8.5;
        let r_inner: f32 = 6.2;
        let r_outer: f32 = 7.5;

        for y in 0..32 {
            for x in 0..32 {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                let dist = (dx * dx + dy * dy).sqrt();

                if dist <= r_inner {
                    img.put_pixel(x, y, bg_red);
                } else if dist <= r_outer {
                    img.put_pixel(x, y, border_white);
                }
            }
        }

        // Draw digit centered
        let start_x = 22; // 22, 23, 24
        let start_y = 6;  // 6, 7, 8, 9, 10
        draw_glyph(&mut img, start_x, start_y, count as usize, text_white);
    } else {
        // 9+: oval badge spanning from x = 16 to x = 31, y = 1 to y = 15
        let cx_left: f32 = 20.0;
        let cx_right: f32 = 25.5;
        let cy: f32 = 8.0;
        let r_inner: f32 = 5.8;
        let r_outer: f32 = 7.0;

        for y in 0..32 {
            for x in 0..32 {
                let px = x as f32;
                let py = y as f32;
                let dx = if px < cx_left {
                    px - cx_left
                } else if px > cx_right {
                    px - cx_right
                } else {
                    0.0
                };
                let dy = py - cy;
                let dist = (dx * dx + dy * dy).sqrt();

                if dist <= r_inner {
                    img.put_pixel(x, y, bg_red);
                } else if dist <= r_outer {
                    img.put_pixel(x, y, border_white);
                }
            }
        }

        // Draw "9+"
        let start_x_9 = 18;
        let start_x_plus = 23;
        let start_y = 6;
        draw_glyph(&mut img, start_x_9, start_y, 9, text_white);
        draw_glyph(&mut img, start_x_plus, start_y, 10, text_white);
    }

    let mut buf = Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).ok()?;
    let bytes = buf.into_inner();
    tauri::image::Image::from_bytes(&bytes)
        .ok()
        .map(|img| img.to_owned())
}

#[allow(unused_variables)]
pub fn update_tray_unread_badge(app: &AppHandle, count: u32) {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        if let Some(tray) = app.tray_by_id("main_tray") {
            if let Some(icon) = render_badged_tray_icon(count) {
                let _ = tray.set_icon(Some(icon));
            }

            let tip = if count == 0 {
                "Raydrop — Universal P2P File Transfer".to_string()
            } else if count == 1 {
                "Raydrop — 1 pesan belum dibaca".to_string()
            } else {
                format!("Raydrop — {} pesan belum dibaca", count)
            };
            let _ = tray.set_tooltip(Some(tip));
        }

        // Also update Windows Taskbar badge count if supported
        if let Some(win) = app.get_webview_window("main") {
            let badge_val = if count == 0 { None } else { Some(count as i64) };
            let _ = win.set_badge_count(badge_val);
        }
    }
}
