//! Fallback fonts for scripts the fonts bundled with egui lack.
//!
//! egui keeps every registered font completely in memory, so git-bull loads
//! at most one system font per script group, and only one that really
//! contains the script's characters.

use eframe::egui::{self, FontData, FontFamily};
use eframe::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
use skrifa::MetadataProvider;

/// Groups of scripts that one font usually covers together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptGroup {
    /// Chinese characters with Japanese kana.
    HanKana,
    /// Korean.
    Hangul,
}

/// A system font chosen as fallback.
#[derive(Clone, Debug)]
pub struct Fallback {
    pub family: String,
    pub data: Vec<u8>,
    /// The face within a font collection.
    pub index: u32,
    pub groups: Vec<ScriptGroup>,
}

/// Characters a font must contain to cover Chinese and Japanese.
const HAN_KANA: &[char] = &['日', '本', '語', 'あ', 'ア', '中', '文'];
/// A weaker choice when no font has kana: Chinese characters only.
const HAN: &[char] = &['日', '本', '語', '中', '文'];
const HANGUL: &[char] = &['한', '국', '어'];

/// Families tried first: what Windows, macOS and common Linux systems use.
/// They are still checked for the characters like any other font.
const PREFERRED: &[&str] = &[
    "Yu Gothic UI",
    "Yu Gothic",
    "Microsoft YaHei UI",
    "Microsoft YaHei",
    "Meiryo",
    "MS Gothic",
    "Malgun Gothic",
    "Hiragino Sans",
    "PingFang SC",
    "Apple SD Gothic Neo",
    "Noto Sans CJK JP",
    "Noto Sans CJK SC",
    "Noto Sans CJK KR",
    "Source Han Sans",
    "WenQuanYi Micro Hei",
    "Droid Sans Fallback",
    "NanumGothic",
];

/// Chooses at most one font per script group from `db`.
pub fn choose(db: &fontdb::Database) -> Vec<Fallback> {
    let mut faces: Vec<&fontdb::FaceInfo> = db.faces().collect();
    let rank = |face: &fontdb::FaceInfo| {
        face.families
            .iter()
            .filter_map(|(family, _)| PREFERRED.iter().position(|p| p == family))
            .min()
            .unwrap_or(PREFERRED.len())
    };
    faces.sort_by_key(|face| rank(face));

    let covers = |face: &fontdb::FaceInfo, chars: &[char]| {
        db.with_face_data(face.id, |data, index| {
            let font = skrifa::FontRef::from_index(data, index).ok()?;
            let charmap = font.charmap();
            Some(chars.iter().all(|c| charmap.map(*c).is_some()))
        })
        .flatten()
        .unwrap_or(false)
    };
    let find = |chars: &[char]| faces.iter().copied().find(|face| covers(face, chars));

    let mut chosen: Vec<(&fontdb::FaceInfo, Vec<ScriptGroup>)> = Vec::new();
    if let Some(face) = find(HAN_KANA).or_else(|| find(HAN)) {
        chosen.push((face, vec![ScriptGroup::HanKana]));
    }
    match chosen.first_mut() {
        Some((face, groups)) if covers(face, HANGUL) => groups.push(ScriptGroup::Hangul),
        _ => {
            if let Some(face) = find(HANGUL) {
                chosen.push((face, vec![ScriptGroup::Hangul]));
            }
        }
    }

    chosen
        .into_iter()
        .filter_map(|(face, groups)| {
            let (data, index) = db.with_face_data(face.id, |data, index| (data.to_vec(), index))?;
            let family = face.families.first().map(|(family, _)| family.clone())?;
            Some(Fallback {
                family,
                data,
                index,
                groups,
            })
        })
        .collect()
}

/// Registers `fallbacks` behind egui's own fonts, for both proportional
/// and monospace text.
pub fn install(ctx: &egui::Context, fallbacks: &[Fallback]) {
    for fallback in fallbacks {
        let mut data = FontData::from_owned(fallback.data.clone());
        data.index = fallback.index;
        ctx.add_font(FontInsert::new(
            &format!("fallback: {}", fallback.family),
            data,
            [FontFamily::Proportional, FontFamily::Monospace]
                .into_iter()
                .map(|family| InsertFontFamily {
                    family,
                    priority: FontPriority::Lowest,
                })
                .collect(),
        ));
    }
}

/// The fallbacks among the fonts installed on this system.
pub fn system_fallbacks() -> Vec<Fallback> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    choose(&db)
}
