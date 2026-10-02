//! The components of the design system (design, decision 5).

mod support;

use eframe::egui::accesskit::{Role, Toggled};
use eframe::egui::os::OperatingSystem;
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, Rect};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use gitbull_app::components::{self, BannerAction, BannerKind, Button, Kind};
use gitbull_app::icons;
use gitbull_app::style::apply_style;
use gitbull_app::theme::{Appearance, DARK};
use gitbull_app::ui::color;
use gitbull_core::settings::{ColourVision, Settings};

const CLOSE_TAB: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::W);

/// A harness on `os` that draws `content` with the dark style.
fn harness_on<State>(
    os: OperatingSystem,
    state: State,
    mut content: impl FnMut(&mut egui::Ui, &mut State) + 'static,
) -> Harness<'static, State> {
    Harness::builder()
        .with_size((480.0, 360.0))
        .with_os(os)
        .build_ui_state(
            move |ui, state| {
                apply_style(ui.ctx(), Appearance::Dark, ColourVision::Standard);
                content(ui, state);
            },
            state,
        )
}

fn harness<State>(
    state: State,
    content: impl FnMut(&mut egui::Ui, &mut State) + 'static,
) -> Harness<'static, State> {
    harness_on(OperatingSystem::Windows, state, content)
}

#[test]
fn icon_button_exposes_its_name() {
    let mut harness = harness((), |ui, _| {
        components::icon_button(ui, icons::CLOSE, "Close git-bull", Some(CLOSE_TAB));
    });
    harness.run();
    harness.get_by_role_and_label(Role::Button, "Close git-bull");
}

#[test]
fn icon_button_names_its_action_and_shortcut_in_a_tooltip() {
    for (os, shortcut) in [
        (OperatingSystem::Windows, "Ctrl+W"),
        (OperatingSystem::Nix, "Ctrl+W"),
        (OperatingSystem::Mac, "Cmd+W"),
    ] {
        let mut harness = harness_on(os, (), |ui, _| {
            components::icon_button(ui, icons::CLOSE, "Close git-bull", Some(CLOSE_TAB));
        });
        harness.run();
        assert!(harness.query_by_label(shortcut).is_none(), "no tooltip yet");

        harness
            .get_by_role_and_label(Role::Button, "Close git-bull")
            .hover();
        harness.run();

        assert!(
            harness.query_by_label(shortcut).is_some(),
            "{os:?}: {shortcut}"
        );
        // The name appears twice: on the button and in the tooltip.
        assert_eq!(harness.query_all_by_label("Close git-bull").count(), 2);
    }
}

#[test]
fn icon_button_without_shortcut_names_its_action_in_a_tooltip() {
    let mut harness = harness((), |ui, _| {
        components::icon_button(ui, icons::GEAR, "Settings", None);
    });
    harness.run();
    harness
        .get_by_role_and_label(Role::Button, "Settings")
        .hover();
    harness.run();
    assert_eq!(harness.query_all_by_label("Settings").count(), 2);
}

#[test]
fn segmented_control_offers_its_choices_as_radio_buttons() {
    let mut harness = harness("light", |ui, chosen: &mut &str| {
        components::segmented(ui, chosen, &[("light", "Light"), ("dark", "Dark")]);
    });
    harness.run();
    let toggled = |harness: &Harness<'_, &str>, label: &str| {
        harness
            .get_by_role_and_label(Role::RadioButton, label)
            .accesskit_node()
            .toggled()
    };
    assert_eq!(toggled(&harness, "Light"), Some(Toggled::True));
    assert_eq!(toggled(&harness, "Dark"), Some(Toggled::False));

    harness
        .get_by_role_and_label(Role::RadioButton, "Dark")
        .click();
    harness.run();

    assert_eq!(*harness.state(), "dark");
    assert_eq!(toggled(&harness, "Dark"), Some(Toggled::True));
    assert_eq!(toggled(&harness, "Light"), Some(Toggled::False));
}

const SYSTEM_TITLE_BAR: &str = "Use the system title bar";

/// Whether the checkbox named `label` is ticked, as assistive technology
/// learns it.
fn ticked(harness: &Harness<'_, bool>, label: &str) -> Option<Toggled> {
    harness
        .get_by_role_and_label(Role::CheckBox, label)
        .accesskit_node()
        .toggled()
}

#[test]
fn a_checkbox_toggles_on_a_click_and_with_space() {
    let mut harness = harness(false, |ui, on: &mut bool| {
        components::checkbox(ui, on, SYSTEM_TITLE_BAR);
    });
    harness.run();
    assert_eq!(ticked(&harness, SYSTEM_TITLE_BAR), Some(Toggled::False));

    harness
        .get_by_role_and_label(Role::CheckBox, SYSTEM_TITLE_BAR)
        .click();
    harness.run();
    assert!(*harness.state());
    assert_eq!(ticked(&harness, SYSTEM_TITLE_BAR), Some(Toggled::True));

    // The keyboard reaches it with Tab.
    harness.key_press(Key::Tab);
    harness.run();
    harness.key_press(Key::Space);
    harness.run();
    assert!(!*harness.state());
}

#[test]
fn tab_onto_a_checkbox_shows_the_focus_ring() {
    let mut harness = harness(false, |ui, on: &mut bool| {
        components::checkbox(ui, on, SYSTEM_TITLE_BAR);
    });
    harness.run();
    let widget = harness
        .get_by_role_and_label(Role::CheckBox, SYSTEM_TITLE_BAR)
        .rect();
    harness.key_press(Key::Tab);
    harness.run();
    assert!(
        has_focus_ring(&harness, widget),
        "no ring around {widget:?}"
    );
}

#[test]
fn a_checkbox_has_a_click_target_of_at_least_24() {
    let mut harness = harness(false, |ui, on: &mut bool| {
        components::checkbox(ui, on, "On");
    });
    harness.run();
    let size = harness
        .get_by_role_and_label(Role::CheckBox, "On")
        .rect()
        .size();
    assert!(size.x >= 24.0 && size.y >= 24.0, "{size:?}");
}

#[test]
fn button_reports_its_click() {
    let mut harness = harness(0, |ui, clicks: &mut u32| {
        for kind in [Kind::Primary, Kind::Secondary, Kind::Ghost] {
            if Button::new(&format!("{kind:?}"))
                .kind(kind)
                .icon(icons::FOLDER)
                .show(ui)
                .clicked()
            {
                *clicks += 1;
            }
        }
    });
    harness.run();
    for kind in ["Primary", "Secondary", "Ghost"] {
        harness.get_by_role_and_label(Role::Button, kind).click();
        harness.run();
    }
    assert_eq!(*harness.state(), 3);
}

#[test]
fn banner_offers_its_actions_and_to_dismiss_it() {
    let mut harness = harness(Vec::new(), |ui, seen: &mut Vec<BannerAction>| {
        let action = components::banner(
            ui,
            BannerKind::Information,
            "The commit is hidden.",
            &["Show all branches"],
            "Dismiss",
        );
        seen.extend(action);
    });
    harness.run();
    harness.get_by_label("The commit is hidden.");
    harness
        .get_by_role_and_label(Role::Button, "Show all branches")
        .click();
    harness.run();
    harness
        .get_by_role_and_label(Role::Button, "Dismiss")
        .click();
    harness.run();
    assert_eq!(
        *harness.state(),
        [BannerAction::Action(0), BannerAction::Dismiss]
    );
}

/// Whether a focus ring is drawn around `widget`.
fn has_focus_ring<State>(harness: &Harness<'_, State>, widget: Rect) -> bool {
    support::focus_rings(harness.output()).iter().any(|ring| {
        ring.expand(1.0).contains_rect(widget) || widget.expand(1.0).contains_rect(*ring)
    })
}

#[test]
fn tab_onto_a_button_shows_the_focus_ring() {
    let mut harness = harness((), |ui, _| {
        Button::new("Open").show(ui);
    });
    harness.run();
    let button = harness.get_by_role_and_label(Role::Button, "Open").rect();
    assert!(!has_focus_ring(&harness, button), "a ring before the focus");

    harness.key_press(Key::Tab);
    harness.run();

    assert!(
        harness
            .get_by_role_and_label(Role::Button, "Open")
            .is_focused()
    );
    assert!(
        has_focus_ring(&harness, button),
        "no ring around {button:?}"
    );
}

#[test]
fn a_text_field_shows_where_typing_goes_after_a_click() {
    let mut harness = harness(String::new(), |ui, text: &mut String| {
        components::text_field(ui, text, "Search", 200.0);
    });
    harness.run();
    let field = harness.get_by_role(Role::TextInput);
    let widget = field.rect();
    field.click();
    harness.run();
    assert!(
        has_focus_ring(&harness, widget),
        "no border around {widget:?}"
    );
}

#[test]
fn a_click_on_a_button_shows_no_focus_ring() {
    let mut harness = harness((), |ui, _| {
        Button::new("Open").show(ui);
    });
    harness.run();
    harness.key_press(Key::Tab);
    harness.run();
    let button = harness.get_by_role_and_label(Role::Button, "Open");
    let widget = button.rect();
    assert!(has_focus_ring(&harness, widget), "Tab shows the ring");
    button.click();
    harness.run();
    assert!(!has_focus_ring(&harness, widget), "the click hides it");
}

#[test]
fn focused_components_show_the_focus_ring() {
    let mut harness = harness(("light", String::new()), |ui, (chosen, text)| {
        Button::new("Open").kind(Kind::Primary).show(ui);
        components::icon_button(ui, icons::GEAR, "Settings", None);
        components::segmented(ui, chosen, &[("light", "Light"), ("dark", "Dark")]);
        components::menu_item(ui, Some(icons::SUN), "Light", None);
        ui.add(egui::TextEdit::singleline(text));
    });
    harness.run();
    for (role, label) in [
        (Role::Button, Some("Open")),
        (Role::Button, Some("Settings")),
        (Role::RadioButton, Some("Light")),
        (Role::Button, Some("Light")),
        (Role::TextInput, None),
    ] {
        let node = match label {
            Some(label) => harness.get_by_role_and_label(role, label),
            None => harness.get_by_role(role),
        };
        let widget = node.rect();
        node.focus();
        harness.run();
        assert!(
            has_focus_ring(&harness, widget),
            "{label:?}: no ring around {widget:?}"
        );
    }
}

#[test]
fn focused_combo_boxes_of_the_window_show_the_focus_ring() {
    let test = support::app_with_open_repository(Settings::default());
    let mut harness = support::window(test.app);
    harness.run();
    assert!(
        support::focus_rings(harness.output()).is_empty(),
        "a ring before the focus"
    );

    // The search mode and the branch filter.
    for value in ["Message", "All branches"] {
        let combo = harness
            .get_all_by_role(Role::ComboBox)
            .find(|node| node.accesskit_node().value().as_deref() == Some(value))
            .expect("the combo box");
        let widget = combo.rect();
        combo.focus();
        harness.run();
        assert!(
            has_focus_ring(&harness, widget),
            "no ring around {value} {widget:?}"
        );
    }
}

#[test]
fn a_text_field_is_as_high_as_a_button() {
    let mut harness = harness(String::new(), |ui, text: &mut String| {
        ui.horizontal(|ui| {
            Button::new("Search").show(ui);
            components::text_field(ui, text, "Search commits", 200.0);
        });
    });
    harness.run();
    let button = harness.get_by_role_and_label(Role::Button, "Search").rect();
    let field = harness.get_by_role(Role::TextInput).rect();
    assert_eq!(field.height(), button.height(), "{field:?} {button:?}");
}

#[test]
fn menu_items_follow_each_other_without_a_gap() {
    let mut harness = harness((), |ui, _| {
        components::menu(ui, |ui| {
            components::menu_item(ui, None, "First", None);
            components::menu_item(ui, None, "Second", None);
        });
    });
    harness.run();
    let first = harness.get_by_role_and_label(Role::Button, "First").rect();
    let second = harness.get_by_role_and_label(Role::Button, "Second").rect();
    assert_eq!(second.top(), first.bottom());
}

#[test]
fn a_context_menu_is_as_wide_as_its_longest_entry() {
    let mut harness = harness((), |ui, _| {
        let commit = ui.add(egui::Label::new("Commit").sense(egui::Sense::click()));
        commit.context_menu(|ui| {
            components::menu(ui, |ui| {
                components::menu_item(ui, None, "Copy", None);
                components::menu_item(ui, None, "Copy the full hash", None);
            });
        });
    });
    harness.run();
    let at = harness.get_by_label("Commit").rect().center();
    harness.hover_at(at);
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run();

    let short = harness.get_by_role_and_label(Role::Button, "Copy").rect();
    let long = harness
        .get_by_role_and_label(Role::Button, "Copy the full hash")
        .rect();
    assert_eq!(short.width(), long.width(), "the entries fill the menu");
    assert!(long.width() < 250.0, "the menu is {} wide", long.width());
}

#[test]
fn a_long_banner_text_wraps_beside_its_buttons() {
    const TEXT: &str = "C:\\Users\\someone\\Projects\\clients\\a-long-name\\deep\\down\\the\\tree is not inside a Git repository.";
    let mut harness = harness((), |ui, _| {
        components::banner(
            ui,
            BannerKind::Warning,
            TEXT,
            &["Show all branches"],
            "Dismiss",
        );
    });
    harness.run();

    let text = harness.get_by_label(TEXT).rect();
    let width = harness.ctx.content_rect().width();
    assert!(text.right() <= width, "the text runs to {}", text.right());
    for button in ["Show all branches", "Dismiss"] {
        let rect = harness.get_by_role_and_label(Role::Button, button).rect();
        assert!(!rect.intersects(text), "{button} {rect:?} lies on {text:?}");
        assert!(rect.right() <= width, "{button} {rect:?}");
    }
}

/// Whether any node offers `text` to assistive technology.
fn offered<State>(harness: &Harness<'_, State>, text: &str) -> bool {
    harness
        .query_all_by(|node| {
            node.label().is_some_and(|label| label.contains(text))
                || node.value().is_some_and(|value| value.contains(text))
        })
        .next()
        .is_some()
}

#[test]
fn icons_are_hidden_from_assistive_technology() {
    let mut harness = harness((), |ui, _| {
        components::icon(ui, icons::TAG, 12.0, egui::Color32::WHITE);
        components::banner(ui, BannerKind::Warning, "Careful", &[], "Dismiss");
    });
    harness.run();
    harness.get_by_label("Careful");
    for icon in [icons::TAG, icons::WARNING] {
        assert!(!offered(&harness, icon), "{icon:?} is offered");
    }
}

#[test]
fn a_disabled_button_is_faded_and_reports_no_click() {
    let mut harness = harness(0, |ui, clicks: &mut u32| {
        ui.add_enabled_ui(false, |ui| {
            if Button::new("Next").show(ui).clicked() {
                *clicks += 1;
            }
        });
    });
    harness.run();
    harness.get_by_role_and_label(Role::Button, "Next").click();
    harness.run();
    assert_eq!(*harness.state(), 0);
    // egui fades whatever a disabled ui draws.
    let colours = support::text_colours(harness.output(), "Next");
    assert!(colours.iter().all(|colour| colour.a() < 255), "{colours:?}");
}

#[test]
fn an_error_text_is_drawn_in_the_error_colour() {
    let mut harness = harness((), |ui, _| {
        components::error_text(ui, "Git could not be started.");
    });
    harness.run();
    assert_eq!(
        support::text_colours(harness.output(), "Git could not be started."),
        [color(DARK.error_fg)]
    );
}

#[test]
fn every_component_has_a_click_target_of_24_by_24() {
    let mut harness = harness(("light", 0), |ui, (chosen, _)| {
        Button::new("A").kind(Kind::Ghost).show(ui);
        components::icon_button(ui, icons::CLOSE, "Close", None);
        components::segmented(ui, chosen, &[("light", "L"), ("dark", "D")]);
        components::menu_item(ui, None, "M", None);
        components::banner(ui, BannerKind::Warning, "W", &["B"], "Dismiss");
    });
    harness.run();
    for (role, label) in [
        (Role::Button, "A"),
        (Role::Button, "Close"),
        (Role::RadioButton, "L"),
        (Role::RadioButton, "D"),
        (Role::Button, "M"),
        (Role::Button, "B"),
        (Role::Button, "Dismiss"),
    ] {
        let size = harness.get_by_role_and_label(role, label).rect().size();
        assert!(size.x >= 24.0 && size.y >= 24.0, "{label} is {size:?}");
    }
}
