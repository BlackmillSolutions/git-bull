//! The bundled fonts, and fallback fonts for scripts they lack.
//!
//! Inter is the proportional font, registered once per weight because egui
//! chooses fonts by family, and JetBrains Mono the monospace font (design,
//! decision 3). Every family of text gets the whole chain behind its font:
//! egui's monochrome emoji fonts, then the system fonts for Chinese,
//! Japanese and Korean. The icons of Phosphor have a family of their own
//! (see [`crate::icons`]).
//!
//! egui keeps every registered font completely in memory, so git-bull loads
//! at most one system font per script group, and only one that really
//! contains the script's characters.

use eframe::egui::{self, FontData, FontDefinitions, FontFamily, FontTweak};
use eframe::epaint::text::{FontInsert, FontPriority, InsertFontFamily, VariationCoords};
use skrifa::MetadataProvider;

/// The family of text in the medium weight, 500.
pub const MEDIUM: &str = "medium";
/// The family of text in the semibold weight, 600.
pub const SEMIBOLD: &str = "semibold";

static INTER: &[u8] = include_bytes!("../assets/fonts/InterVariable.ttf");
static JETBRAINS_MONO: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");

/// Whether the bundled fonts are loaded, which at start-up happens before
/// the first frame. Until then egui knows only its own fonts. Valid from
/// the first pass of `ctx` on, when egui has fonts.
///
/// egui's fonts are asked only until they know the bundled families, which
/// locks them; from then on a record in the context's data answers, as
/// nothing takes the bundled fonts away. The record comes from egui's
/// fonts, not from the call that gives them: egui takes fonts given to
/// `set_fonts` only at the start of the next pass.
pub fn loaded(ctx: &egui::Context) -> bool {
    let record = egui::Id::new(LOADED);
    if ctx.data(|data| data.get_temp::<bool>(record)) == Some(true) {
        return true;
    }
    let semibold = FontFamily::Name(SEMIBOLD.into());
    let known = ctx.fonts(|fonts| fonts.definitions().families.contains_key(&semibold));
    if known {
        ctx.data_mut(|data| data.insert_temp(record, true));
    }
    known
}

/// The record that the bundled fonts are loaded.
const LOADED: &str = "bundled-fonts-loaded";

/// Counts the calls of [`install`] in the context's data.
const INSTALLED: &str = "fallback-fonts-installed";

/// Changes whenever text may measure differently with the same font id: when
/// the bundled fonts arrive, and when fallbacks are installed. What is
/// measured once and kept, such as the widths of badges, is measured again
/// when it changes.
pub fn generation(ctx: &egui::Context) -> u64 {
    let installed = ctx.data(|data| data.get_temp::<u64>(egui::Id::new(INSTALLED)));
    2 * installed.unwrap_or(0) + u64::from(loaded(ctx))
}

/// egui's emoji fonts, in their order.
const EMOJI: [&str; 2] = ["NotoEmoji-Regular", "emoji-icon-font"];

/// Every family of text, which the fallbacks join.
fn families() -> [FontFamily; 4] {
    [
        FontFamily::Proportional,
        FontFamily::Name(MEDIUM.into()),
        FontFamily::Name(SEMIBOLD.into()),
        FontFamily::Monospace,
    ]
}

/// The bundled fonts with the chain of each family, without the system
/// fallbacks, which [`install`] adds once they are found.
pub fn definitions() -> FontDefinitions {
    let mut definitions = FontDefinitions::default();
    // Ubuntu Light and Hack are replaced; egui's emoji fonts stay.
    definitions.font_data.remove("Ubuntu-Light");
    definitions.font_data.remove("Hack");
    let inter = |weight: f32| {
        FontData::from_static(INTER).tweak(FontTweak {
            coords: VariationCoords::new([(b"wght", weight)]),
            ..FontTweak::default()
        })
    };
    for (name, data) in [
        ("Inter", inter(400.0)),
        ("Inter Medium", inter(500.0)),
        ("Inter Semibold", inter(600.0)),
        ("JetBrains Mono", FontData::from_static(JETBRAINS_MONO)),
        (
            "Phosphor",
            FontData::from_static(egui_phosphor::Variant::Regular.font_bytes()),
        ),
    ] {
        definitions.font_data.insert(name.to_owned(), data.into());
    }
    let [proportional, medium, semibold, monospace] = families();
    for (family, first) in [
        (proportional, &["Inter"][..]),
        (medium, &["Inter Medium"]),
        (semibold, &["Inter Semibold"]),
        (monospace, &["JetBrains Mono", "Inter"]),
    ] {
        let chain = first.iter().chain(&EMOJI).map(|name| name.to_string());
        definitions.families.insert(family, chain.collect());
    }
    // Icons have a family of their own: in a chain with Inter, either font
    // hides glyphs of the other.
    definitions.families.insert(
        FontFamily::Name(crate::icons::FAMILY.into()),
        vec!["Phosphor".to_owned()],
    );
    definitions
}

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

/// Registers `fallbacks` last in the chain of every family of text.
pub fn install(ctx: &egui::Context, fallbacks: &[Fallback]) {
    ctx.data_mut(|data| *data.get_temp_mut_or_default::<u64>(egui::Id::new(INSTALLED)) += 1);
    for fallback in fallbacks {
        let mut data = FontData::from_owned(fallback.data.clone());
        data.index = fallback.index;
        ctx.add_font(FontInsert::new(
            &format!("fallback: {}", fallback.family),
            data,
            families()
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
