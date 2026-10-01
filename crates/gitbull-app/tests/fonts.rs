//! Choosing and installing fallback fonts for Chinese, Japanese and Korean.

#[path = "support/font.rs"]
mod font;

use eframe::egui::{self, FontId};
use egui_kittest::Harness;
use gitbull_app::fonts::{ScriptGroup, choose, install};

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
