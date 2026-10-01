//! The bundled fonts, and choosing and installing fallback fonts for
//! Chinese, Japanese and Korean.

#[path = "support/font.rs"]
mod font;

use eframe::egui::{self, FontFamily, FontId};
use egui_kittest::Harness;
use gitbull_app::fonts::{MEDIUM, SEMIBOLD, ScriptGroup, choose, definitions, install};

const JAPANESE: &[char] = &['日', '本', '語', 'あ', 'ア', '中', '文'];
const CHINESE_ONLY: &[char] = &['日', '本', '語', '中', '文'];
const KOREAN: &[char] = &['한', '국', '어'];

fn database(fonts: &[(&str, &[char])]) -> fontdb::Database {
    let mut db = fontdb::Database::new();
    for (family, chars) in fonts {
        db.load_font_data(font::font_with(family, chars));
    }
    db
}

fn families(db: &fontdb::Database) -> Vec<(String, Vec<ScriptGroup>)> {
    choose(db)
        .into_iter()
        .map(|fallback| (fallback.family, fallback.groups))
        .collect()
}

#[test]
fn font_with_japanese_characters_covers_chinese_and_japanese() {
    let db = database(&[("Test Latin", &['A', 'B']), ("Test Japanese", JAPANESE)]);
    assert_eq!(
        families(&db),
        [("Test Japanese".to_owned(), vec![ScriptGroup::HanKana])]
    );
}

#[test]
fn separate_fonts_are_chosen_for_separate_script_groups() {
    let db = database(&[("Test Japanese", JAPANESE), ("Test Korean", KOREAN)]);
    assert_eq!(
        families(&db),
        [
            ("Test Japanese".to_owned(), vec![ScriptGroup::HanKana]),
            ("Test Korean".to_owned(), vec![ScriptGroup::Hangul]),
        ]
    );
}

#[test]
fn a_font_covering_every_group_is_loaded_once() {
    let everything: Vec<char> = JAPANESE.iter().chain(KOREAN).copied().collect();
    let db = database(&[("Test Everything", &everything), ("Test Korean", KOREAN)]);
    assert_eq!(
        families(&db),
        [(
            "Test Everything".to_owned(),
            vec![ScriptGroup::HanKana, ScriptGroup::Hangul]
        )]
    );
}

#[test]
fn font_with_kana_is_preferred_over_one_with_chinese_characters_only() {
    let db = database(&[("Test Chinese", CHINESE_ONLY), ("Test Japanese", JAPANESE)]);
    assert_eq!(
        families(&db),
        [("Test Japanese".to_owned(), vec![ScriptGroup::HanKana])]
    );
}

#[test]
fn font_with_chinese_characters_only_is_used_when_nothing_better_exists() {
    let db = database(&[("Test Chinese", CHINESE_ONLY)]);
    assert_eq!(
        families(&db),
        [("Test Chinese".to_owned(), vec![ScriptGroup::HanKana])]
    );
}

#[test]
fn without_suitable_fonts_nothing_is_loaded() {
    let db = database(&[("Test Latin", &['A', 'B'])]);
    assert!(choose(&db).is_empty());
}

#[test]
fn installed_fallback_lets_egui_draw_japanese() {
    let db = database(&[("Test Japanese", JAPANESE)]);
    let fallbacks = choose(&db);
    let mut before = None;
    let mut after = None;
    let mut frame = 0;
    let mut harness = Harness::new_ui(|ui| {
        let ctx = ui.ctx().clone();
        let drawable =
            ctx.fonts_mut(|fonts| fonts.has_glyphs(&FontId::proportional(14.0), "日本語あア"));
        match frame {
            0 => {
                before = Some(drawable);
                install(&ctx, &fallbacks);
            }
            _ => after = Some(drawable),
        }
        frame += 1;
        ui.label(egui::RichText::new("日本語あア"));
    });
    harness.run();
    drop(harness);

    assert_eq!(before, Some(false), "egui alone lacks Japanese");
    assert_eq!(after, Some(true), "the fallback supplies it");
}

fn family_names(family: FontFamily) -> Vec<String> {
    definitions()
        .families
        .get(&family)
        .cloned()
        .unwrap_or_default()
}

#[test]
fn each_family_has_the_chain_of_the_design() {
    let emoji = ["NotoEmoji-Regular", "emoji-icon-font"];
    let chain = |first: &[&str]| -> Vec<String> {
        first
            .iter()
            .chain(&emoji)
            .map(|name| name.to_string())
            .collect()
    };
    assert_eq!(family_names(FontFamily::Proportional), chain(&["Inter"]));
    assert_eq!(
        family_names(FontFamily::Name(MEDIUM.into())),
        chain(&["Inter Medium"])
    );
    assert_eq!(
        family_names(FontFamily::Name(SEMIBOLD.into())),
        chain(&["Inter Semibold"])
    );
    assert_eq!(
        family_names(FontFamily::Monospace),
        chain(&["JetBrains Mono", "Inter"])
    );
}

/// The width of `text` in `family` at 14 points, with the bundled fonts.
fn width(family: FontFamily, text: &str) -> f32 {
    let mut found = None;
    let mut frame = 0;
    let mut harness = Harness::new_ui(|ui| {
        if frame == 0 {
            ui.ctx().set_fonts(definitions());
        } else {
            let galley = ui.painter().layout_no_wrap(
                text.to_owned(),
                FontId::new(14.0, family.clone()),
                egui::Color32::WHITE,
            );
            found = Some(galley.size().x);
        }
        frame += 1;
    });
    harness.run();
    drop(harness);
    found.expect("a second frame")
}

#[test]
fn heavier_weights_set_wider_text() {
    let text = "Merge branch main";
    let regular = width(FontFamily::Proportional, text);
    let medium = width(FontFamily::Name(MEDIUM.into()), text);
    let semibold = width(FontFamily::Name(SEMIBOLD.into()), text);
    assert!(regular < medium, "{regular} {medium}");
    assert!(medium < semibold, "{medium} {semibold}");
}

#[test]
fn monospace_text_keeps_one_width_per_character() {
    assert_eq!(
        width(FontFamily::Monospace, "iiii"),
        width(FontFamily::Monospace, "MMMM")
    );
}

#[test]
fn installed_fallback_lets_the_semibold_weight_draw_japanese() {
    let db = database(&[("Test Japanese", JAPANESE)]);
    let fallbacks = choose(&db);
    let semibold = FontId::new(14.0, FontFamily::Name(SEMIBOLD.into()));
    let mut drawable = Vec::new();
    let mut frame = 0;
    let mut harness = Harness::new_ui(|ui| {
        let ctx = ui.ctx().clone();
        match frame {
            0 => ctx.set_fonts(definitions()),
            1 => {
                drawable.push(ctx.fonts_mut(|fonts| fonts.has_glyphs(&semibold, "日本語あア")));
                install(&ctx, &fallbacks);
            }
            _ => drawable.push(ctx.fonts_mut(|fonts| fonts.has_glyphs(&semibold, "日本語あア"))),
        }
        frame += 1;
    });
    harness.run();
    drop(harness);

    assert_eq!(
        drawable.first(),
        Some(&false),
        "the bundled fonts lack Japanese"
    );
    assert_eq!(drawable.last(), Some(&true), "the fallback supplies it");
}

/// A manual check: which fallbacks does this machine offer, and how much
/// memory do they take? Run with `cargo test -p gitbull-app --test fonts -- --ignored --nocapture`.
#[test]
#[ignore = "depends on the fonts installed on this machine"]
fn fallbacks_of_this_machine() {
    let started = std::time::Instant::now();
    let fallbacks = gitbull_app::fonts::system_fallbacks();
    for fallback in &fallbacks {
        eprintln!(
            "{} (face {}): {:?}, {:.1} MB",
            fallback.family,
            fallback.index,
            fallback.groups,
            fallback.data.len() as f64 / 1_048_576.0
        );
    }
    eprintln!("found in {:?}", started.elapsed());
    assert!(!fallbacks.is_empty(), "no fallback font on this machine");
}
