//! Tiny fonts made for tests, so that no font file with an unclear licence
//! has to live in the repository.

use kurbo::BezPath;
use write_fonts::FontBuilder;
use write_fonts::tables::cmap::Cmap;
use write_fonts::tables::glyf::{GlyfLocaBuilder, Glyph, SimpleGlyph};
use write_fonts::tables::head::Head;
use write_fonts::tables::hhea::Hhea;
use write_fonts::tables::hmtx::{Hmtx, LongMetric};
use write_fonts::tables::maxp::Maxp;
use write_fonts::tables::name::{Name, NameRecord};
use write_fonts::tables::os2::Os2;
use write_fonts::tables::post::Post;
use write_fonts::types::{FWord, GlyphId, NameId, UfWord};

/// A TrueType font named `family` with a square glyph for each of `chars`.
pub fn font_with(family: &str, chars: &[char]) -> Vec<u8> {
    let mut square = BezPath::new();
    square.move_to((100.0, 0.0));
    square.line_to((900.0, 0.0));
    square.line_to((900.0, 800.0));
    square.line_to((100.0, 800.0));
    square.close_path();
    let square = SimpleGlyph::from_bezpath(&square).expect("a valid outline");

    // Glyph 0 is the empty `.notdef`; glyph n + 1 draws `chars[n]`.
    let mut glyphs = GlyfLocaBuilder::new();
    glyphs.add_glyph(&Glyph::Empty).expect("empty glyph");
    for _ in chars {
        glyphs.add_glyph(&square).expect("square glyph");
    }
    let (glyf, loca, loca_format) = glyphs.build();
    let count = chars.len() as u16 + 1;

    let cmap = Cmap::from_mappings(
        chars
            .iter()
            .enumerate()
            .map(|(n, c)| (*c, GlyphId::new(n as u32 + 1))),
    )
    .expect("unique characters");
    let head = Head {
        units_per_em: 1000,
        x_min: 0,
        y_min: -200,
        x_max: 1000,
        y_max: 800,
        index_to_loc_format: loca_format as i16,
        ..Head::default()
    };
    let hhea = Hhea {
        ascender: FWord::new(800),
        descender: FWord::new(-200),
        line_gap: FWord::new(0),
        advance_width_max: UfWord::new(1000),
        x_max_extent: FWord::new(900),
        caret_slope_rise: 1,
        number_of_h_metrics: count,
        ..Hhea::default()
    };
    let maxp = Maxp {
        num_glyphs: count,
        max_points: Some(4),
        max_contours: Some(1),
        max_composite_points: Some(0),
        max_composite_contours: Some(0),
        max_zones: Some(2),
        max_twilight_points: Some(0),
        max_storage: Some(0),
        max_function_defs: Some(0),
        max_instruction_defs: Some(0),
        max_stack_elements: Some(0),
        max_size_of_instructions: Some(0),
        max_component_elements: Some(0),
        max_component_depth: Some(0),
    };
    let hmtx = Hmtx::new(vec![LongMetric::new(1000, 100); count as usize], Vec::new());
    // fontdb skips fonts without a PostScript name, which has no spaces.
    let postscript = family.replace(' ', "");
    let names = [
        (NameId::FAMILY_NAME, family.to_owned()),
        (NameId::FULL_NAME, family.to_owned()),
        (NameId::POSTSCRIPT_NAME, postscript),
    ]
    .into_iter()
    .map(|(id, text)| NameRecord::new(3, 1, 0x409, id, text.into()))
    .collect();
    let name = Name::new(names);

    FontBuilder::new()
        .add_table(&head)
        .and_then(|b| b.add_table(&hhea))
        .and_then(|b| b.add_table(&maxp))
        .and_then(|b| b.add_table(&Os2::default()))
        .and_then(|b| b.add_table(&hmtx))
        .and_then(|b| b.add_table(&cmap))
        .and_then(|b| b.add_table(&loca))
        .and_then(|b| b.add_table(&glyf))
        .and_then(|b| b.add_table(&name))
        .and_then(|b| b.add_table(&Post::default()))
        .expect("tables compile")
        .build()
}
