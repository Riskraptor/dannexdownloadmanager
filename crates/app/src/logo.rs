//! The app icon at each size (tools/icon/comet_logo.py, built by build_icon.py): the hand-tuned
//! pictures of `assets/icons/app`, picked by the size they are shown at, so the title bar, the
//! taskbar, Alt+Tab, the tray and the windows that show the icon get a drawing made for that
//! size rather than the 256-pixel one scaled down by the system (blurry at 16 px). A size with no
//! drawing of its own is made from the next larger one, scaled down smoothly. And the mark alone
//! (`mark-<size>.png`, white on nothing), the notification area's classic icon.

/// The sizes drawn on their own, with their pictures.
const PICTURES: [(u32, &[u8]); 9] = [
    (16, include_bytes!("../assets/icons/app/mdm-16.png")),
    (20, include_bytes!("../assets/icons/app/mdm-20.png")),
    (24, include_bytes!("../assets/icons/app/mdm-24.png")),
    (32, include_bytes!("../assets/icons/app/mdm-32.png")),
    (40, include_bytes!("../assets/icons/app/mdm-40.png")),
    (48, include_bytes!("../assets/icons/app/mdm-48.png")),
    (64, include_bytes!("../assets/icons/app/mdm-64.png")),
    (128, include_bytes!("../assets/icons/app/mdm-128.png")),
    (256, include_bytes!("../assets/icons/app/mdm-256.png")),
];

/// The mark alone, white on nothing, at the sizes drawn on their own.
const MARKS: [(u32, &[u8]); 6] = [
    (16, include_bytes!("../assets/icons/app/mark-16.png")),
    (20, include_bytes!("../assets/icons/app/mark-20.png")),
    (24, include_bytes!("../assets/icons/app/mark-24.png")),
    (32, include_bytes!("../assets/icons/app/mark-32.png")),
    (40, include_bytes!("../assets/icons/app/mark-40.png")),
    (48, include_bytes!("../assets/icons/app/mark-48.png")),
];

/// The drawing for `px` on-screen pixels: the smallest drawn for at least that size (a picture
/// is never scaled up), else the largest.
fn pick(set: &'static [(u32, &'static [u8])], px: f32) -> (u32, &'static [u8]) {
    let px = px.round().max(1.0) as u32;
    set.iter()
        .copied()
        .find(|(s, _)| *s >= px)
        .unwrap_or(set[set.len() - 1])
}

/// The app icon as egui draws it, about `px` pixels on screen (a window's heading, its own title
/// bar, the drop target): the picture drawn for that size, or the next larger one.
pub fn source(px: f32) -> egui::ImageSource<'static> {
    let (size, bytes) = pick(&PICTURES, px);
    // Kept for the program's life, as their textures are (one per size shown).
    let uri = match size {
        16 => "bytes://mdm/app/16.png",
        20 => "bytes://mdm/app/20.png",
        24 => "bytes://mdm/app/24.png",
        32 => "bytes://mdm/app/32.png",
        40 => "bytes://mdm/app/40.png",
        48 => "bytes://mdm/app/48.png",
        64 => "bytes://mdm/app/64.png",
        128 => "bytes://mdm/app/128.png",
        _ => "bytes://mdm/app/256.png",
    };
    egui::ImageSource::Bytes {
        uri: uri.into(),
        bytes: egui::load::Bytes::Static(bytes),
    }
}

/// `side` pixels square, unpremultiplied RGBA, from the set's drawing for that size: as drawn
/// when there is one, else the next larger scaled down (Lanczos), never scaled up but past the
/// largest.
fn pixels(set: &'static [(u32, &'static [u8])], side: u32) -> Vec<u8> {
    let side = side.clamp(1, 1024);
    let (size, bytes) = pick(set, side as f32);
    let Ok(img) = image::load_from_memory_with_format(bytes, image::ImageFormat::Png) else {
        return vec![0; (side * side * 4) as usize];
    };
    let img = img.to_rgba8();
    if size == side {
        return img.into_raw();
    }
    image::imageops::resize(&img, side, side, image::imageops::FilterType::Lanczos3).into_raw()
}

/// The app icon, `side` pixels square (unpremultiplied RGBA): the window icons Windows is given
/// for its title bars, taskbar and Alt+Tab, and the tray's 3D style.
pub fn rgba(side: u32) -> Vec<u8> {
    pixels(&PICTURES, side)
}

/// The mark alone, `side` pixels square, in `ink` (its coverage the alpha): the tray's classic
/// style.
pub fn mark_rgba(side: u32, ink: [u8; 3]) -> Vec<u8> {
    let mut px = pixels(&MARKS, side);
    for p in px.as_chunks_mut::<4>().0 {
        p[..3].copy_from_slice(&ink);
    }
    px
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each size drawn on its own is what its size asks for; the others take the next larger
    /// (never a smaller one scaled up), the largest past the end.
    #[test]
    fn a_size_takes_its_own_drawing_or_the_next_larger() {
        for (size, _) in PICTURES {
            assert_eq!(pick(&PICTURES, size as f32).0, size);
        }
        assert_eq!(pick(&PICTURES, 28.0).0, 32);
        assert_eq!(pick(&PICTURES, 36.0).0, 40);
        assert_eq!(pick(&PICTURES, 56.0).0, 64);
        assert_eq!(pick(&PICTURES, 300.0).0, 256);
        assert_eq!(pick(&MARKS, 36.0).0, 40);
        assert_eq!(pick(&MARKS, 64.0).0, 48);
    }

    /// Every picture reads, square at its size; a size between them is made at that size.
    #[test]
    fn every_size_reads_at_its_own_size() {
        for (size, bytes) in PICTURES.iter().chain(MARKS.iter()) {
            let img = image::load_from_memory(bytes).unwrap();
            assert_eq!((img.width(), img.height()), (*size, *size));
        }
        for side in [16, 20, 24, 28, 32, 36, 40, 48, 56, 64, 72, 96] {
            let px = rgba(side);
            assert_eq!(px.len(), (side * side * 4) as usize);
            assert!(px.chunks(4).any(|p| p[3] == 255), "{side}: nothing opaque");
        }
    }

    /// The mark takes the ink in every pixel, its shape in the alpha: the comet's head at the
    /// lower right and its tail from the upper left, held apart by a ring of nothing; the top
    /// right corner empty.
    #[test]
    fn the_mark_is_one_ink_and_its_shape() {
        let side = 16;
        let px = mark_rgba(side, [0x1c, 0x1c, 0x1e]);
        assert!(px.chunks(4).all(|p| p[..3] == [0x1c, 0x1c, 0x1e]));
        let opaque = px.chunks(4).filter(|p| p[3] == 255).count();
        assert!(opaque > 20 && opaque < 80, "{opaque}");
        let at = |x: u32, y: u32| px[((y * side + x) * 4 + 3) as usize];
        assert_eq!(at(10, 10), 255, "the head");
        assert_eq!(at(5, 5), 255, "the tail");
        assert!(at(8, 7) < 64 && at(7, 8) < 64, "the ring between them");
        assert_eq!(at(13, 2), 0, "the empty corner");
    }
}
