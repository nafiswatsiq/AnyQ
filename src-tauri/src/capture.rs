use base64::{Engine as _, engine::general_purpose::STANDARD};
use image::{DynamicImage, ImageFormat};
use std::io::Cursor;
use xcap::Monitor;

pub fn capture_primary_screen() -> Result<String, String> {
    let monitors = Monitor::all().map_err(|error| error.to_string())?;
    if monitors.is_empty() {
        return Err("No display monitor found".to_string());
    }

    let cursor = cursor_location();
    let monitor = cursor
        .and_then(|(x, y)| {
            monitors.iter().find(|monitor| {
                let left = monitor.x().unwrap_or_default();
                let top = monitor.y().unwrap_or_default();
                let right = left + monitor.width().unwrap_or_default() as i32;
                let bottom = top + monitor.height().unwrap_or_default() as i32;
                x >= left && x < right && y >= top && y < bottom
            })
        })
        .or_else(|| monitors.iter().find(|monitor| monitor.is_primary().unwrap_or(false)))
        .unwrap_or(&monitors[0]);

    let image = monitor.capture_image().map_err(|error| error.to_string())?;
    let image = DynamicImage::ImageRgba8(image);
    let image = resize_for_capture(image);
    let mut png = Cursor::new(Vec::new());
    image
        .write_to(&mut png, ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    Ok(STANDARD.encode(png.into_inner()))
}

#[cfg(target_os = "windows")]
fn cursor_location() -> Option<(i32, i32)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut point = POINT::default();
    unsafe { GetCursorPos(&mut point).ok()? };
    Some((point.x, point.y))
}

#[cfg(not(target_os = "windows"))]
fn cursor_location() -> Option<(i32, i32)> {
    None
}

fn resize_for_capture(image: DynamicImage) -> DynamicImage {
    let max_dimension = image.width().max(image.height());
    if max_dimension <= 1568 {
        return image;
    }
    image.resize(1568, 1568, image::imageops::FilterType::Triangle)
}

#[cfg(test)]
mod tests {
    use super::resize_for_capture;
    use image::{DynamicImage, ImageBuffer, Rgba};

    #[test]
    fn limits_longest_side() {
        let image = DynamicImage::ImageRgba8(ImageBuffer::from_pixel(
            2000,
            1000,
            Rgba([0, 0, 0, 255]),
        ));
        let resized = resize_for_capture(image);
        assert_eq!(resized.width(), 1568);
        assert_eq!(resized.height(), 784);
    }
}
