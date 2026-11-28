//! Text drawn by DirectWrite (egui's glyph rasterizer hook): Smooth's natural mode and Classic's
//! GDI-classic one, for Latin, Greek and Cyrillic.

mod classic_text {
    use std::collections::HashMap;
    use std::os::windows::ffi::OsStrExt as _;
    use std::path::PathBuf;

    use egui::epaint::text::{GlyphBitmap, GlyphRasterizerRequest, RasterizedGlyph};
    use egui::epaint::{Color32, ColorImage};
    use windows::Win32::Graphics::DirectWrite::{
        DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_FACE_TYPE_TRUETYPE, DWRITE_FONT_METRICS,
        DWRITE_FONT_SIMULATIONS_NONE, DWRITE_GLYPH_METRICS, DWRITE_GLYPH_OFFSET, DWRITE_GLYPH_RUN,
        DWRITE_GRID_FIT_MODE_ENABLED, DWRITE_MEASURING_MODE_GDI_CLASSIC,
        DWRITE_MEASURING_MODE_NATURAL, DWRITE_RENDERING_MODE_GDI_CLASSIC,
        DWRITE_RENDERING_MODE_NATURAL_SYMMETRIC, DWRITE_TEXT_ANTIALIAS_MODE_GRAYSCALE,
        DWRITE_TEXTURE_ALIASED_1x1, DWriteCreateFactory, IDWriteFactory2, IDWriteFontFace,
    };

    /// How much the smooth look thins its edges: 1.0 keeps DirectWrite's coverage as it is.
    const SMOOTH_COVERAGE_GAMMA: f32 = 1.8;

    pub struct Glyphs {
        factory: IDWriteFactory2,
        dir: PathBuf,
        /// The font files opened so far; `None` for one DirectWrite can't use.
        faces: HashMap<&'static str, Option<IDWriteFontFace>>,
    }

    // SAFETY: DirectWrite's shared factory and the objects it makes are free-threaded
    // (documented thread-safe); `Glyphs` is only used behind a Mutex.
    unsafe impl Send for Glyphs {}

    impl Glyphs {
        pub fn new(dir: PathBuf) -> windows::core::Result<Glyphs> {
            // SAFETY: a plain COM call.
            let factory: IDWriteFactory2 =
                unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)? };
            Ok(Glyphs {
                factory,
                dir,
                faces: HashMap::new(),
            })
        }

        fn face(&mut self, file: &'static str) -> Option<IDWriteFontFace> {
            let (factory, dir) = (&self.factory, &self.dir);
            self.faces
                .entry(file)
                .or_insert_with(|| {
                    let path: Vec<u16> = dir
                        .join(file)
                        .as_os_str()
                        .encode_wide()
                        .chain([0])
                        .collect();
                    // SAFETY: `path` is NUL-terminated and outlives the calls; plain COM calls.
                    unsafe {
                        let font = factory
                            .CreateFontFileReference(windows::core::PCWSTR(path.as_ptr()), None)
                            .ok()?;
                        factory
                            .CreateFontFace(
                                DWRITE_FONT_FACE_TYPE_TRUETYPE,
                                &[Some(font)],
                                0,
                                DWRITE_FONT_SIMULATIONS_NONE,
                            )
                            .ok()
                    }
                })
                .clone()
        }

        pub fn glyph(
            &mut self,
            req: &GlyphRasterizerRequest<'_>,
            rtl: bool,
            smooth: bool,
        ) -> Option<RasterizedGlyph> {
            let mut chars = req.cluster.chars();
            let ch = chars.next()?;
            if chars.next().is_some() || !super::classic_char(ch, rtl) {
                return None;
            }
            if req.font_size_px.is_nan() || req.font_size_px < 1.0 {
                return None;
            }
            let face = self.face(super::classic_font_file(req.family)?)?;
            // GDI's sizes are whole pixels; the natural look keeps the size it is asked for.
            let em = if smooth {
                req.font_size_px
            } else {
                req.font_size_px.round()
            };
            let mut id = 0u16;
            let mut metrics = DWRITE_GLYPH_METRICS::default();
            let mut font = DWRITE_FONT_METRICS::default();
            // SAFETY: each pointer is to one element, the count given.
            unsafe {
                face.GetGlyphIndices(&(ch as u32), 1, &mut id).ok()?;
                if id == 0 {
                    return None;
                }
                if smooth {
                    face.GetDesignGlyphMetrics(&id, 1, &mut metrics, false)
                        .ok()?;
                } else {
                    face.GetGdiCompatibleGlyphMetrics(
                        em,
                        1.0,
                        None,
                        false,
                        &id,
                        1,
                        &mut metrics,
                        false,
                    )
                    .ok()?;
                }
                face.GetMetrics(&mut font);
            }
            let advance = metrics.advanceWidth as f32 * em / f32::from(font.designUnitsPerEm);
            // Smooth: the font's own width, as egui lays a line out by it (and places each letter
            // at a quarter pixel). Classic: the whole pixels GDI's hinting gives.
            let advance_px = if smooth { advance } else { advance.round() };
            let origin = if smooth { req.subpixel_offset_px } else { 0.0 };
            let bitmap = self
                .draw(face, id, em, smooth, origin)
                .unwrap_or(GlyphBitmap {
                    // A space: egui still wants a bitmap to carry the advance.
                    image: ColorImage::new([1, 1], vec![Color32::TRANSPARENT]),
                    offset_px: egui::vec2(0.0, 0.0),
                    is_color: false,
                });
            Some(RasterizedGlyph { bitmap, advance_px })
        }

        /// The glyph in grey, placed from its origin; `None` when it has no ink.
        fn draw(
            &self,
            face: IDWriteFontFace,
            id: u16,
            em: f32,
            smooth: bool,
            origin_x: f32,
        ) -> Option<GlyphBitmap> {
            let (ids, advances, offsets) = ([id], [0f32], [DWRITE_GLYPH_OFFSET::default()]);
            let run = DWRITE_GLYPH_RUN {
                fontFace: std::mem::ManuallyDrop::new(Some(face)),
                fontEmSize: em,
                glyphCount: 1,
                glyphIndices: ids.as_ptr(),
                glyphAdvances: advances.as_ptr(),
                glyphOffsets: offsets.as_ptr(),
                isSideways: false.into(),
                bidiLevel: 0,
            };
            // SAFETY: `run` and the arrays it points to live through the calls; the texture
            // buffer is exactly the bounds' size.
            let drawn = unsafe {
                self.factory
                    .CreateGlyphRunAnalysis(
                        &run,
                        None,
                        if smooth {
                            DWRITE_RENDERING_MODE_NATURAL_SYMMETRIC
                        } else {
                            DWRITE_RENDERING_MODE_GDI_CLASSIC
                        },
                        if smooth {
                            DWRITE_MEASURING_MODE_NATURAL
                        } else {
                            DWRITE_MEASURING_MODE_GDI_CLASSIC
                        },
                        DWRITE_GRID_FIT_MODE_ENABLED,
                        DWRITE_TEXT_ANTIALIAS_MODE_GRAYSCALE,
                        origin_x,
                        0.0,
                    )
                    .and_then(|a| {
                        let r = a.GetAlphaTextureBounds(DWRITE_TEXTURE_ALIASED_1x1)?;
                        let w = (r.right - r.left).max(0) as usize;
                        let h = (r.bottom - r.top).max(0) as usize;
                        let mut coverage = vec![0u8; w * h];
                        if w > 0 && h > 0 {
                            a.CreateAlphaTexture(DWRITE_TEXTURE_ALIASED_1x1, &r, &mut coverage)?;
                        }
                        Ok((r, w, h, coverage))
                    })
            };
            drop(std::mem::ManuallyDrop::into_inner(run.fontFace));
            let (r, w, h, coverage) = drawn.ok()?;
            if w == 0 || h == 0 {
                return None;
            }
            // White with alpha, premultiplied (`GlyphBitmap`).
            // Smooth: less ink at the edges, so the weight matches what Windows' own
            // DirectWrite text shows (WPF's grey-scale text, compared side by side): coverage
            // blended as it is reads bolder, most of all light on dark.
            let gamma = if smooth { SMOOTH_COVERAGE_GAMMA } else { 1.0 };
            let pixels = coverage
                .into_iter()
                .map(|a| {
                    let a = ((f32::from(a) / 255.0).powf(gamma) * 255.0).round() as u8;
                    Color32::from_rgba_premultiplied(a, a, a, a)
                })
                .collect();
            Some(GlyphBitmap {
                image: ColorImage::new([w, h], pixels),
                offset_px: egui::vec2(r.left as f32, r.top as f32),
                is_color: false,
            })
        }
    }
}

/// Latin, Greek and Cyrillic text as Windows' classic controls draw it, in grey (#262, #323,
/// #334): each letter's shape and width come from the font's own hinting for this size, through
/// DirectWrite's GDI-classic mode. Before, egui placed letters by their unhinted widths rounded
/// to whole pixels while the letters themselves were fitted to pixels: the two disagreed by up to
/// a pixel, so letters touched in one place ("download") and stood apart in another ("Resum e").
///
/// `smooth` picks how a letter is drawn: `false`, the classic controls' look (hinted in both
/// directions at a whole pixel size, letters on whole pixels: crisp, and coarse on a large,
/// dense screen), or `true`, DirectWrite's natural look (the size as it is, hinted up and down
/// only, each letter where it falls between two pixels, with all 256 levels of grey: the text of
/// Windows' own modern apps, much easier to read at 1440p).
///
/// `None` from DirectWrite's factory or a font's file keeps egui's rasterizer. A cluster it isn't
/// for ([`classic_char`]) is left to egui too, which shapes it. The rasterizer is told the
/// interface's direction: a bracket in a right-to-left line is mirrored by the shaper, so those
/// stay with egui there.
pub fn text_rasterizer(
    font_dir: std::path::PathBuf,
    rtl: bool,
    smooth: bool,
) -> Option<egui::epaint::text::GlyphRasterizer> {
    let glyphs = classic_text::Glyphs::new(font_dir)
        .map_err(|e| tracing::warn!("text: DirectWrite unavailable, egui draws the text ({e})"))
        .ok()?;
    let glyphs = std::sync::Mutex::new(glyphs);
    Some(
        egui::epaint::text::GlyphRasterizer::new(
            "mdm::win32::classic_text",
            move |req: &egui::epaint::text::GlyphRasterizerRequest<'_>| {
                glyphs.lock().ok()?.glyph(req, rtl, smooth)
            },
        )
        .with_priority(egui::epaint::text::FontPriority::Highest),
    )
}

/// Does [`text_rasterizer`] draw this character? One code point that needs no shaping: Latin,
/// Greek and Cyrillic letters, digits and punctuation. Scripts whose letters change shape (Arabic,
/// Indic, ...) and the invisible marks are left to egui and its shaper; so are the brackets of a
/// right-to-left line, which the shaper mirrors.
pub(super) fn classic_char(c: char, rtl: bool) -> bool {
    if rtl
        && matches!(
            c,
            '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '«' | '»' | '‹' | '›'
        )
    {
        return false;
    }
    matches!(c,
        ' '..='~'
        | '\u{A0}'..='\u{AC}' // Latin-1, without the soft hyphen
        | '\u{AE}'..='\u{2AF}' // Latin Extended and IPA
        | '\u{370}'..='\u{3FF}' // Greek
        | '\u{400}'..='\u{52F}' // Cyrillic
        | '\u{1E00}'..='\u{1EFF}' // Vietnamese and other precomposed Latin
        | '\u{2010}'..='\u{2027}' // dashes, quotes, bullets, ellipsis
        | '\u{2030}'..='\u{205E}' // per mille, primes, guillemets
        | '\u{20A0}'..='\u{20BF}' // currency
    )
}

/// The font file of a family `theme.rs` sets up (Segoe UI leads the two built-in ones).
pub(super) fn classic_font_file(family: &egui::FontFamily) -> Option<&'static str> {
    match family {
        egui::FontFamily::Proportional | egui::FontFamily::Monospace => Some("segoeui.ttf"),
        egui::FontFamily::Name(name) => match &**name {
            "tahoma" => Some("tahoma.ttf"),
            "tahoma-bold" => Some("tahomabd.ttf"),
            _ => None,
        },
    }
}

#[cfg(test)]
mod classic_text_tests {
    use egui::FontFamily;
    use egui::epaint::text::GlyphRasterizerRequest;

    fn glyphs() -> super::classic_text::Glyphs {
        let dir = std::env::var_os("WINDIR")
            .map(|w| std::path::PathBuf::from(w).join("Fonts"))
            .unwrap();
        super::classic_text::Glyphs::new(dir).unwrap()
    }

    fn req<'a>(cluster: &'a str, family: &'a FontFamily, size: f32) -> GlyphRasterizerRequest<'a> {
        GlyphRasterizerRequest {
            cluster,
            family,
            font_size_px: size,
            subpixel_offset_px: 0.0,
        }
    }

    /// #334: every letter gets a whole pixel width, and is drawn within it: the two can't drift
    /// apart any more. The letters of the words the report names, at the sizes of 100 % to 200 %.
    #[test]
    fn letters_have_whole_pixel_advances_and_stay_inside_them() {
        let mut g = glyphs();
        for family in [FontFamily::Proportional, FontFamily::Name("tahoma".into())] {
            for size in [11.0, 12.0, 13.2, 15.0, 16.5, 18.0, 24.0] {
                for ch in "downloadefaultakeDRsum".chars() {
                    let text = ch.to_string();
                    let glyph = g.glyph(&req(&text, &family, size), false, false).unwrap();
                    assert_eq!(glyph.advance_px.fract(), 0.0, "{ch} at {size}");
                    assert!(glyph.advance_px >= 1.0, "{ch} at {size}");
                    let right = glyph.bitmap.offset_px.x + glyph.bitmap.image.size[0] as f32;
                    assert!(
                        right <= glyph.advance_px + 1.0,
                        "{ch} at {size}: ink ends at {right}, advance {}",
                        glyph.advance_px
                    );
                }
            }
        }
    }

    /// The smooth look keeps the font's own width and draws more levels of grey than the
    /// classic one, whose letters are fitted to whole pixels.
    #[test]
    fn smooth_letters_keep_their_width_and_their_greys() {
        let mut g = glyphs();
        let f = FontFamily::Proportional;
        let levels = |glyph: &egui::epaint::text::RasterizedGlyph| {
            let mut l: Vec<u8> = glyph.bitmap.image.pixels.iter().map(|p| p.a()).collect();
            l.sort_unstable();
            l.dedup();
            l.len()
        };
        let classic = g.glyph(&req("o", &f, 13.2), false, false).unwrap();
        let smooth = g.glyph(&req("o", &f, 13.2), false, true).unwrap();
        assert_eq!(classic.advance_px.fract(), 0.0);
        assert!(smooth.advance_px.fract() != 0.0, "{}", smooth.advance_px);
        assert!(levels(&smooth) > levels(&classic));
        // A quarter pixel to the right changes the picture, not the width.
        let mut shifted = req("o", &f, 13.2);
        shifted.subpixel_offset_px = 0.5;
        let moved = g.glyph(&shifted, false, true).unwrap();
        assert_eq!(moved.advance_px, smooth.advance_px);
        assert_ne!(moved.bitmap.image.pixels, smooth.bitmap.image.pixels);
    }

    #[test]
    fn a_space_keeps_its_advance() {
        let mut g = glyphs();
        let s = g
            .glyph(&req(" ", &FontFamily::Proportional, 12.0), false, false)
            .unwrap();
        assert!(s.advance_px > 0.0);
    }

    #[test]
    fn what_needs_shaping_stays_with_egui() {
        let mut g = glyphs();
        let f = FontFamily::Proportional;
        for not in [
            "\u{62a}",
            "\u{5d0}",
            "\u{93e}",
            "\u{e14}",
            "e\u{301}",
            "\u{1F600}",
        ] {
            assert!(
                g.glyph(&req(not, &f, 12.0), false, false).is_none(),
                "{not:?}"
            );
        }
        // A bracket is mirrored by the shaper in a right-to-left line, only.
        assert!(g.glyph(&req("(", &f, 12.0), false, false).is_some());
        assert!(g.glyph(&req("(", &f, 12.0), true, false).is_none());
        // A family it isn't told about stays with egui.
        let other = FontFamily::Name("other".into());
        assert!(g.glyph(&req("a", &other, 12.0), false, false).is_none());
    }
}
