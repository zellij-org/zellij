use super::*;
use zellij_utils::data::{BareKey, KeyModifier, KeyWithModifier, Mouse};

fn key(bare_key: BareKey) -> KeyWithModifier {
    KeyWithModifier::new(bare_key)
}

fn ch(character: char) -> KeyWithModifier {
    KeyWithModifier::new(BareKey::Char(character))
}

fn shift_tab() -> KeyWithModifier {
    KeyWithModifier::new(BareKey::Tab).with_shift_modifier()
}

fn enc(text: &str) -> String {
    text.as_bytes()
        .iter()
        .map(|b| b.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn dcs(name: &str, coordinates: &str, state: &str, fields: &[String]) -> String {
    let mut expected = format!("\u{1b}Pz{};{};{}", name, coordinates, state);
    for field in fields {
        expected.push(';');
        expected.push_str(field);
    }
    expected.push_str("\u{1b}\\");
    expected
}

fn choice(index: usize, label: &str) -> UiValue {
    UiValue::Choice {
        index,
        label: label.to_owned(),
    }
}

#[test]
fn button_serializes_every_look() {
    let label = vec![enc("Save")];
    assert_eq!(
        Button::new("Save").serialize(2, 3),
        dcs("button", "2/3/8/1", "", &label)
    );
    assert_eq!(
        Button::new("Save").focused().serialize(2, 3),
        dcs("button", "2/3/8/1", "f", &label)
    );
    assert_eq!(
        Button::new("Save").hovered().serialize(2, 3),
        dcs("button", "2/3/8/1", "h", &label)
    );
    assert_eq!(
        Button::new("Save").pressed().serialize(2, 3),
        dcs("button", "2/3/8/1", "p", &label)
    );
    assert_eq!(
        Button::new("Save").disabled().pressed().serialize(2, 3),
        dcs("button", "2/3/8/1", "d", &label)
    );
}

#[test]
fn button_activates_on_keys_and_click_and_stays_pressed_until_its_timer() {
    let mut button = Button::new("Go");
    button.serialize(0, 0);
    assert_eq!(
        button.handle_key(&key(BareKey::Enter)),
        UiResponse::Activated
    );
    assert_eq!(button.handle_key(&ch(' ')), UiResponse::Activated);
    assert_eq!(button.handle_key(&ch('x')), UiResponse::NotHandled);
    assert!(!button.handle_timer());
    assert_eq!(
        button.handle_mouse(Mouse::LeftClick(0, 1)),
        UiResponse::Activated
    );
    assert!(button.is_pressed());
    button.handle_mouse(Mouse::Release(0, 1));
    assert!(button.is_pressed());
    assert_eq!(
        button.handle_mouse(Mouse::LeftClick(0, 1)),
        UiResponse::Activated
    );
    assert!(!button.handle_timer());
    assert!(button.is_pressed());
    assert!(button.handle_timer());
    assert!(!button.is_pressed());
    assert!(!button.handle_timer());
    assert_eq!(
        button.handle_mouse(Mouse::LeftClick(1, 1)),
        UiResponse::NotHandled
    );
}

#[test]
fn disabled_button_ignores_input() {
    let mut button = Button::new("Go").disabled();
    button.serialize(0, 0);
    assert_eq!(
        button.handle_key(&key(BareKey::Enter)),
        UiResponse::NotHandled
    );
    assert_eq!(
        button.handle_mouse(Mouse::LeftClick(0, 1)),
        UiResponse::Consumed
    );
    assert!(!button.is_pressed());
}

#[test]
fn toggle_serializes_on_off_focused_and_disabled() {
    let label = vec![enc("Wrap")];
    assert_eq!(
        Toggle::new("Wrap", false).serialize(0, 1),
        dcs("toggle", "0/1/10/1", "lw=5", &label)
    );
    assert_eq!(
        Toggle::new("Wrap", true).focused().serialize(0, 1),
        dcs("toggle", "0/1/10/1", "f,on,lw=5", &label)
    );
    assert_eq!(
        Toggle::new("Wrap", true)
            .disabled()
            .label_width(8)
            .serialize(0, 1),
        dcs("toggle", "0/1/13/1", "d,on,lw=8", &label)
    );
}

#[test]
fn toggle_flips_on_space_enter_and_click() {
    let mut toggle = Toggle::new("Wrap", false);
    toggle.serialize(3, 2);
    assert_eq!(
        toggle.handle_key(&ch(' ')),
        UiResponse::Changed(UiValue::Bool(true))
    );
    assert_eq!(
        toggle.handle_key(&key(BareKey::Enter)),
        UiResponse::Changed(UiValue::Bool(false))
    );
    assert_eq!(
        toggle.handle_mouse(Mouse::LeftClick(2, 9)),
        UiResponse::Changed(UiValue::Bool(true))
    );
    assert_eq!(
        toggle.handle_mouse(Mouse::LeftClick(3, 9)),
        UiResponse::NotHandled
    );
    assert!(toggle.is_on());
    let mut disabled = Toggle::new("Wrap", false).disabled();
    assert_eq!(disabled.handle_key(&ch(' ')), UiResponse::NotHandled);
    assert!(!disabled.is_on());
}

fn sample_menu() -> MenuList {
    MenuList::new(vec![
        MenuItem::new("Copy").shortcut("Ctrl c"),
        MenuItem::new("Paste"),
        MenuItem::separator(),
        MenuItem::new("Delete").disabled(),
        MenuItem::new("Rename"),
    ])
}

#[test]
fn menu_list_serializes_rows_separators_disabled_and_highlight() {
    let mut menu = sample_menu().with_border().focused();
    let serialized = menu.serialize(1, 2, 20, 10);
    let fields = vec![
        format!("{}:{}", enc("Copy"), enc("Ctrl c")),
        enc("Paste"),
        "-".to_owned(),
        format!("d{}", enc("Delete")),
        enc("Rename"),
    ];
    assert_eq!(
        serialized,
        dcs("menu", "1/2/20/7", "b,f,hl=0,above=0,below=0", &fields)
    );
    assert_eq!(menu.last_area(), Some(Rect::new(1, 2, 20, 7)));
}

#[test]
fn menu_list_serializes_matched_characters_and_marks() {
    let mut menu = MenuList::new(vec![
        MenuItem::new("main").matched_indices(vec![0, 2]).marked(),
        MenuItem::new("dev"),
    ]);
    let serialized = menu.serialize(0, 0, 10, 5);
    let fields = vec![format!("m{}/0,2", enc("main")), enc("dev")];
    assert_eq!(
        serialized,
        dcs("menu", "0/0/10/2", "hl=0,above=0,below=0", &fields)
    );
}

#[test]
fn menu_list_arrows_skip_separators_and_disabled_rows() {
    let mut menu = sample_menu();
    menu.serialize(0, 0, 20, 10);
    assert_eq!(menu.handle_key(&key(BareKey::Down)), UiResponse::Consumed);
    assert_eq!(menu.highlighted_index(), Some(1));
    menu.handle_key(&key(BareKey::Down));
    assert_eq!(menu.highlighted_index(), Some(4));
    menu.handle_key(&key(BareKey::Down));
    assert_eq!(menu.highlighted_index(), Some(0));
    menu.handle_key(&key(BareKey::Up));
    assert_eq!(menu.highlighted_index(), Some(4));
    assert_eq!(
        menu.handle_key(&key(BareKey::Enter)),
        UiResponse::Submitted(choice(4, "Rename"))
    );
    assert_eq!(menu.handle_key(&key(BareKey::Esc)), UiResponse::Cancelled);
}

#[test]
fn menu_list_letters_jump_to_the_next_match() {
    let mut menu = MenuList::new(vec![
        MenuItem::new("Alpha"),
        MenuItem::new("Beta"),
        MenuItem::new("bravo"),
        MenuItem::new("Charlie"),
    ]);
    menu.handle_key(&ch('b'));
    assert_eq!(menu.highlighted_index(), Some(1));
    menu.handle_key(&ch('b'));
    assert_eq!(menu.highlighted_index(), Some(2));
    menu.handle_key(&ch('b'));
    assert_eq!(menu.highlighted_index(), Some(1));
}

#[test]
fn menu_list_mouse_hover_highlights_and_click_chooses() {
    let mut menu = sample_menu().with_border();
    menu.serialize(0, 0, 20, 10);
    assert_eq!(menu.handle_mouse(Mouse::Hover(2, 3)), UiResponse::Consumed);
    assert_eq!(menu.highlighted_index(), Some(1));
    assert_eq!(menu.handle_mouse(Mouse::Hover(4, 3)), UiResponse::Consumed);
    assert_eq!(menu.highlighted_index(), Some(1));
    assert_eq!(
        menu.handle_mouse(Mouse::LeftClick(5, 3)),
        UiResponse::Submitted(choice(4, "Rename"))
    );
    assert_eq!(
        menu.handle_mouse(Mouse::LeftClick(4, 3)),
        UiResponse::Consumed
    );
    assert_eq!(
        menu.handle_mouse(Mouse::LeftClick(20, 3)),
        UiResponse::NotHandled
    );
}

#[test]
fn menu_list_scrolls_with_context_rows_and_shows_hidden_counts() {
    let items = (0..10)
        .map(|i| MenuItem::new(format!("Item {}", i)))
        .collect();
    let mut menu = MenuList::new(items);
    let serialized = menu.serialize(0, 0, 12, 6);
    assert!(serialized.contains("0/0/12/6;ind,hl=0,above=0,below=6;"));
    for _ in 0..4 {
        menu.handle_key(&key(BareKey::Down));
    }
    let serialized = menu.serialize(0, 0, 12, 6);
    assert!(serialized.contains(";ind,hl=2,above=2,below=4;"));
    assert!(serialized.contains(&enc("Item 5")));
    assert!(!serialized.contains(&enc("Item 1")));
    assert_eq!(menu.item_at(1, 3), Some(2));
    assert_eq!(menu.item_at(0, 3), None);
}

#[test]
fn menu_list_wheel_scrolls_only_while_hovered_and_indicators_page() {
    let items = (0..10)
        .map(|i| MenuItem::new(format!("Item {}", i)))
        .collect();
    let mut menu = MenuList::new(items).with_border();
    menu.serialize(0, 0, 14, 6);
    assert_eq!(
        menu.handle_mouse(Mouse::ScrollDown(1)),
        UiResponse::NotHandled
    );
    assert_eq!(menu.scroll_offset(), 0);
    assert_eq!(menu.handle_mouse(Mouse::Hover(2, 3)), UiResponse::Consumed);
    assert_eq!(
        menu.handle_mouse(Mouse::ScrollDown(3)),
        UiResponse::Consumed
    );
    assert_eq!(menu.scroll_offset(), 3);
    assert_eq!(menu.highlighted_index(), Some(3));
    let serialized = menu.serialize(0, 0, 14, 6);
    assert!(serialized.contains(";b,hl=0,above=3,below=3;"));
    assert_eq!(menu.handle_mouse(Mouse::Hover(5, 3)), UiResponse::Consumed);
    assert!(menu.serialize(0, 0, 14, 6).contains(",hd;"));
    assert_eq!(
        menu.handle_mouse(Mouse::LeftClick(5, 3)),
        UiResponse::Consumed
    );
    assert_eq!(menu.scroll_offset(), 6);
    assert_eq!(menu.handle_mouse(Mouse::Hover(20, 3)), UiResponse::Consumed);
    assert_eq!(
        menu.handle_mouse(Mouse::ScrollUp(1)),
        UiResponse::NotHandled
    );
}

fn sample_dropdown() -> Dropdown {
    Dropdown::new("Mode", vec!["normal", "titles", "compact"])
}

#[test]
fn dropdown_serializes_closed_focused_disabled_and_open() {
    let fields = vec![enc("Mode"), enc("normal")];
    assert_eq!(
        sample_dropdown().serialize(0, 0, 20),
        dcs("dropdown", "0/0/20/1", "lw=5", &fields)
    );
    assert_eq!(
        sample_dropdown().focused().serialize(0, 0, 20),
        dcs("dropdown", "0/0/20/1", "f,lw=5", &fields)
    );
    assert_eq!(
        sample_dropdown().disabled().serialize(0, 0, 20),
        dcs("dropdown", "0/0/20/1", "d,lw=5", &fields)
    );
    let mut open = sample_dropdown().selected(1).opened();
    let closed_field = open.serialize(0, 0, 20);
    assert!(closed_field.contains(";o,lw=5;"));
    let overlay = open.serialize_overlay(20, 40);
    assert!(overlay.starts_with("\u{1b}Pzmenu;5/1/15/5;b,f,hl=1,above=0,below=0;"));
    assert!(overlay.contains(&format!("m{}", enc("titles"))));
    assert!(!open.opens_upward());
}

#[test]
fn dropdown_opens_upward_when_there_is_no_room_below() {
    let mut dropdown = sample_dropdown().opened();
    dropdown.serialize(0, 18, 20);
    let overlay = dropdown.serialize_overlay(20, 40);
    assert!(overlay.starts_with("\u{1b}Pzmenu;5/13/15/5;"));
    assert!(dropdown.opens_upward());
    assert_eq!(dropdown.list_area(), Some(Rect::new(5, 13, 15, 5)));
}

#[test]
fn dropdown_list_is_limited_and_scrolls() {
    let options: Vec<String> = (0..30).map(|i| format!("Option {}", i)).collect();
    let mut dropdown = Dropdown::new("", options)
        .max_list_rows(5)
        .selected(20)
        .opened();
    dropdown.serialize(0, 0, 20);
    let overlay = dropdown.serialize_overlay(40, 40);
    assert!(overlay.contains("0/1/20/7;b,f,hl=2,above=18,below=7;"));
}

#[test]
fn dropdown_keys_change_open_choose_and_cancel() {
    let mut dropdown = sample_dropdown();
    dropdown.serialize(0, 0, 20);
    assert_eq!(
        dropdown.handle_key(&key(BareKey::Right)),
        UiResponse::Changed(choice(1, "titles"))
    );
    assert_eq!(
        dropdown.handle_key(&key(BareKey::Left)),
        UiResponse::Changed(choice(0, "normal"))
    );
    assert_eq!(
        dropdown.handle_key(&key(BareKey::Left)),
        UiResponse::Consumed
    );
    assert_eq!(
        dropdown.handle_key(&key(BareKey::Up)),
        UiResponse::NotHandled
    );
    assert_eq!(dropdown.handle_key(&ch(' ')), UiResponse::Consumed);
    assert!(dropdown.is_open());
    assert!(dropdown.captures_input());
    assert_eq!(
        dropdown.handle_key(&key(BareKey::Down)),
        UiResponse::Consumed
    );
    assert_eq!(dropdown.handle_key(&ch('c')), UiResponse::Consumed);
    assert_eq!(
        dropdown.handle_key(&key(BareKey::Enter)),
        UiResponse::Changed(choice(2, "compact"))
    );
    assert!(!dropdown.is_open());
    dropdown.handle_key(&key(BareKey::Enter));
    assert_eq!(
        dropdown.handle_key(&key(BareKey::Esc)),
        UiResponse::Cancelled
    );
    assert!(!dropdown.is_open());
    assert_eq!(dropdown.selected_value(), Some("compact"));
}

#[test]
fn dropdown_mouse_opens_chooses_and_cancels_on_outside_click() {
    let mut dropdown = sample_dropdown();
    dropdown.serialize(0, 0, 20);
    assert_eq!(
        dropdown.handle_mouse(Mouse::LeftClick(0, 10)),
        UiResponse::Consumed
    );
    assert!(dropdown.is_open());
    dropdown.serialize_overlay(20, 40);
    assert_eq!(
        dropdown.handle_mouse(Mouse::Hover(4, 8)),
        UiResponse::Consumed
    );
    assert!(dropdown.serialize_overlay(20, 40).contains(";b,f,hl=2,"));
    assert_eq!(
        dropdown.handle_mouse(Mouse::LeftClick(4, 8)),
        UiResponse::Changed(choice(2, "compact"))
    );
    assert!(!dropdown.is_open());
    dropdown.handle_mouse(Mouse::LeftClick(0, 10));
    dropdown.serialize_overlay(20, 40);
    assert_eq!(
        dropdown.handle_mouse(Mouse::LeftClick(15, 30)),
        UiResponse::Cancelled
    );
    assert!(!dropdown.is_open());
    assert_eq!(dropdown.selected_index(), 2);
}

#[test]
fn text_input_serializes_placeholder_focus_error_and_disabled() {
    let mut empty = TextInput::empty().label("Name").placeholder("type here");
    assert_eq!(
        empty.serialize(0, 0, 20),
        dcs(
            "text_input",
            "0/0/20/1",
            "ph,lw=5",
            &[enc("Name"), enc("type here"), String::new(), String::new()]
        )
    );
    let mut focused = TextInput::new("abc").focused();
    assert_eq!(
        focused.serialize(0, 0, 12),
        dcs(
            "text_input",
            "0/0/12/1",
            "f,lw=0,cur=3",
            &[String::new(), enc("abc"), String::new(), String::new()]
        )
    );
    let mut invalid = TextInput::new("12a").validator(|value: &str| {
        if value.chars().all(|c| c.is_ascii_digit()) {
            Ok(())
        } else {
            Err("digits only".to_owned())
        }
    });
    assert_eq!(
        invalid.serialize(0, 0, 12),
        dcs(
            "text_input",
            "0/0/12/2",
            "err,lw=0",
            &[String::new(), enc("12a"), enc("digits only"), String::new()]
        )
    );
    let disabled = TextInput::new("x").disabled().focused().serialize(0, 0, 10);
    assert!(disabled.contains(";f,d,lw=0;"));
}

#[test]
fn text_input_scrolls_horizontally_to_follow_the_cursor() {
    let mut input = TextInput::new("abcdefghijklmnop").focused();
    let serialized = input.serialize(0, 0, 10);
    assert!(serialized.contains(&format!("cur=5;;{};", enc("lmnop"))));
    input.move_to_start();
    let serialized = input.serialize(0, 0, 10);
    assert!(serialized.contains(&format!("cur=0;;{};", enc("abcdef"))));
}

#[test]
fn text_input_reports_changes_and_refuses_enter_while_invalid() {
    let mut input = TextInput::empty().validator(|value: &str| {
        if value.parse::<u32>().is_ok() {
            Ok(())
        } else {
            Err("not a number".to_owned())
        }
    });
    assert_eq!(
        input.handle_key(&ch('4')),
        UiResponse::Changed(UiValue::Text("4".to_owned()))
    );
    assert_eq!(input.handle_key(&key(BareKey::Left)), UiResponse::Consumed);
    assert_eq!(
        input.handle_key(&ch('x')),
        UiResponse::Changed(UiValue::Text("x4".to_owned()))
    );
    assert_eq!(input.validation_error(), Some("not a number".to_owned()));
    assert_eq!(input.handle_key(&key(BareKey::Enter)), UiResponse::Consumed);
    input.handle_key(&key(BareKey::Backspace));
    assert_eq!(
        input.handle_key(&key(BareKey::Enter)),
        UiResponse::Submitted(UiValue::Text("4".to_owned()))
    );
    assert_eq!(input.handle_key(&key(BareKey::Tab)), UiResponse::NotHandled);
    assert_eq!(input.handle_key(&key(BareKey::Up)), UiResponse::NotHandled);
    assert_eq!(input.handle_key(&key(BareKey::Esc)), UiResponse::Cancelled);
}

#[test]
fn text_input_keeps_undo_redo_and_word_keys() {
    let mut input = TextInput::new("hello world");
    let ctrl = |c: char| KeyWithModifier::new(BareKey::Char(c)).with_ctrl_modifier();
    input.handle_key(&KeyWithModifier::new(BareKey::Backspace).with_ctrl_modifier());
    assert_eq!(input.get_text(), "hello ");
    input.handle_key(&ctrl('z'));
    assert_eq!(input.get_text(), "hello world");
    input.handle_key(&ctrl('y'));
    assert_eq!(input.get_text(), "hello ");
    input.handle_key(&ctrl('z'));
    input.handle_key(&ctrl('a'));
    assert_eq!(input.get_cursor_position(), 0);
    input.handle_key(&KeyWithModifier::new(BareKey::Right).with_alt_modifier());
    assert_eq!(input.get_cursor_position(), 6);
    assert_eq!(input.handle_key(&ctrl('c')), UiResponse::Cancelled);
    assert_eq!(input.handle_key(&ctrl('x')), UiResponse::NotHandled);
    let redo = KeyWithModifier::new_with_modifiers(
        BareKey::Char('z'),
        [KeyModifier::Ctrl, KeyModifier::Shift]
            .into_iter()
            .collect(),
    );
    input.handle_key(&key(BareKey::Delete));
    assert_eq!(input.get_text(), "hello orld");
    input.handle_key(&ctrl('z'));
    input.handle_key(&redo);
    assert_eq!(input.get_text(), "hello orld");
}

#[test]
fn text_input_search_mode_clears_before_cancelling_and_shows_match_count() {
    let mut search = TextInput::empty().search_mode();
    assert_eq!(
        search.handle_key(&ch('a')),
        UiResponse::Changed(UiValue::Text("a".to_owned()))
    );
    search.set_match_count(Some(3));
    let serialized = search.serialize(0, 0, 30);
    assert!(serialized.ends_with(&format!(";{}\u{1b}\\", enc("3 matches"))));
    assert_eq!(
        search.handle_key(&key(BareKey::Esc)),
        UiResponse::Changed(UiValue::Text(String::new()))
    );
    assert_eq!(search.handle_key(&key(BareKey::Esc)), UiResponse::Cancelled);
}

#[test]
fn text_input_accept_filter_refuses_characters() {
    let mut digits = TextInput::empty().accept(|c| c.is_ascii_digit());
    assert_eq!(digits.handle_key(&ch('a')), UiResponse::Consumed);
    assert_eq!(
        digits.handle_key(&ch('7')),
        UiResponse::Changed(UiValue::Text("7".to_owned()))
    );
}

#[test]
fn text_input_click_moves_the_cursor() {
    let mut input = TextInput::new("hello").label("L");
    input.serialize(0, 0, 20);
    assert_eq!(
        input.handle_mouse(Mouse::LeftClick(0, 6)),
        UiResponse::Consumed
    );
    assert_eq!(input.get_cursor_position(), 2);
    assert_eq!(
        input.handle_mouse(Mouse::LeftClick(1, 6)),
        UiResponse::NotHandled
    );
}

#[test]
fn number_stepper_serializes_and_clamps_at_limits() {
    let mut stepper = NumberStepper::new("Size", 10).min(0).max(20);
    assert_eq!(
        stepper.serialize(0, 0),
        dcs("stepper", "0/0/12/1", "lw=5", &[enc("Size"), enc("10")])
    );
    let mut at_min = NumberStepper::new("", -5).min(0).focused();
    assert_eq!(
        at_min.serialize(0, 0),
        dcs(
            "stepper",
            "0/0/7/1",
            "f,lmin,lw=0",
            &[String::new(), enc("0")]
        )
    );
    let mut at_max = NumberStepper::new("", 99).max(50).disabled();
    assert!(at_max.serialize(0, 0).contains(";d,lmax,lw=0;"));
}

#[test]
fn number_stepper_steps_edits_and_clamps() {
    let mut stepper = NumberStepper::new("", 10).step(5).min(0).max(20);
    assert_eq!(
        stepper.handle_key(&key(BareKey::Right)),
        UiResponse::Changed(UiValue::Number(15))
    );
    stepper.handle_key(&key(BareKey::Right));
    assert_eq!(
        stepper.handle_key(&key(BareKey::Right)),
        UiResponse::Consumed
    );
    assert_eq!(stepper.value(), 20);
    assert_eq!(stepper.handle_key(&ch('7')), UiResponse::Consumed);
    assert!(stepper.is_editing());
    assert_eq!(stepper.handle_key(&ch('x')), UiResponse::NotHandled);
    stepper.handle_key(&ch('5'));
    assert_eq!(stepper.edit_text(), "75");
    assert_eq!(
        stepper.handle_key(&key(BareKey::Enter)),
        UiResponse::Consumed
    );
    assert_eq!(stepper.value(), 20);
    stepper.handle_key(&key(BareKey::Backspace));
    stepper.handle_key(&key(BareKey::Backspace));
    stepper.handle_key(&ch('3'));
    assert_eq!(
        stepper.handle_key(&key(BareKey::Enter)),
        UiResponse::Changed(UiValue::Number(3))
    );
    stepper.handle_key(&ch('9'));
    assert_eq!(
        stepper.handle_key(&key(BareKey::Esc)),
        UiResponse::Cancelled
    );
    assert_eq!(stepper.value(), 3);
    assert_eq!(
        stepper.handle_key(&key(BareKey::Up)),
        UiResponse::NotHandled
    );
}

#[test]
fn number_stepper_arrow_clicks_step() {
    let mut stepper = NumberStepper::new("N", 1).min(0).max(3);
    stepper.serialize(0, 0);
    assert_eq!(
        stepper.handle_mouse(Mouse::LeftClick(0, 2)),
        UiResponse::Changed(UiValue::Number(0))
    );
    assert_eq!(
        stepper.handle_mouse(Mouse::LeftClick(0, 2)),
        UiResponse::Consumed
    );
    assert_eq!(
        stepper.handle_mouse(Mouse::LeftClick(0, 8)),
        UiResponse::Changed(UiValue::Number(1))
    );
    assert_eq!(
        stepper.handle_mouse(Mouse::LeftClick(0, 5)),
        UiResponse::Consumed
    );
}

#[test]
fn scroll_view_tracks_visible_rows_bar_and_indicators() {
    let mut view = ScrollView::new(50);
    assert_eq!(view.layout(0, 2, 20, 10), 0..10);
    assert!(view.needs_indicators());
    assert_eq!(view.gutter_width(), 9);
    assert_eq!(view.content_width(), 11);
    assert_eq!(view.content_y(), 2);
    assert_eq!(
        view.serialize_indicators(),
        dcs(
            "scroll_indicator",
            "11/2/9/10",
            "bar,tot=50,off=0,above=0,below=40",
            &[]
        )
    );
    view.ensure_visible(25);
    assert_eq!(view.visible_range(), 18..28);
    assert_eq!(view.screen_row(18), Some(2));
    assert_eq!(view.screen_row(17), None);
    assert_eq!(view.row_at(4), Some(20));
    view.ensure_visible(3);
    assert_eq!(view.offset(), 1);
    assert_eq!(
        view.handle_key(&key(BareKey::PageDown)),
        UiResponse::Changed(UiValue::Index(11))
    );
    assert_eq!(
        view.handle_mouse(Mouse::ScrollDown(1)),
        UiResponse::NotHandled
    );
    assert_eq!(view.handle_mouse(Mouse::Hover(5, 5)), UiResponse::Consumed);
    assert_eq!(
        view.handle_mouse(Mouse::ScrollDown(100)),
        UiResponse::Changed(UiValue::Index(40))
    );
    assert_eq!(
        view.handle_mouse(Mouse::ScrollDown(1)),
        UiResponse::Consumed
    );
    assert_eq!(view.handle_mouse(Mouse::Hover(2, 15)), UiResponse::Consumed);
    assert!(view.serialize_indicators().contains(",hu\u{1b}"));
    assert_eq!(
        view.handle_mouse(Mouse::LeftClick(2, 15)),
        UiResponse::Changed(UiValue::Index(30))
    );
    assert_eq!(
        view.handle_mouse(Mouse::LeftClick(5, 5)),
        UiResponse::NotHandled
    );
    assert_eq!(
        view.handle_mouse(Mouse::LeftClick(11, 11)),
        UiResponse::Changed(UiValue::Index(40))
    );
    let mut short = ScrollView::new(3);
    short.layout(0, 0, 10, 10);
    assert!(!short.needs_indicators());
    assert_eq!(short.content_width(), 10);
    assert_eq!(short.serialize_indicators(), "");
}

#[test]
fn side_menu_serializes_selection_and_handles_keys_and_clicks() {
    let mut menu = SideMenu::new(vec!["One", "Two", "Three"])
        .selected(1)
        .focused();
    assert_eq!(
        menu.serialize(0, 0, 10, 5),
        dcs(
            "side_menu",
            "0/0/10/5",
            "f,sel=1",
            &[enc("One"), enc("Two"), enc("Three")]
        )
    );
    assert_eq!(
        menu.handle_key(&key(BareKey::Down)),
        UiResponse::Changed(choice(2, "Three"))
    );
    assert_eq!(
        menu.handle_key(&key(BareKey::Down)),
        UiResponse::Changed(choice(0, "One"))
    );
    assert_eq!(
        menu.handle_mouse(Mouse::LeftClick(1, 3)),
        UiResponse::Changed(choice(1, "Two"))
    );
    assert_eq!(
        menu.handle_mouse(Mouse::LeftClick(4, 3)),
        UiResponse::Consumed
    );
    assert_eq!(
        menu.handle_key(&key(BareKey::Enter)),
        UiResponse::Submitted(choice(1, "Two"))
    );
    let mut unfocused = SideMenu::new(vec!["A", "B", "C", "D"]).selected(3);
    let serialized = unfocused.serialize(0, 0, 6, 2);
    assert!(serialized.contains(";sel=1;"));
}

#[test]
fn side_menu_keeps_context_rows_and_scrolls_on_hover_wheel() {
    let items: Vec<String> = (0..10).map(|i| format!("Item {}", i)).collect();
    let mut menu = SideMenu::new(items);
    assert!(menu
        .serialize(0, 0, 10, 6)
        .contains(";ind,sel=0,above=0,below=6;"));
    for _ in 0..3 {
        menu.handle_key(&key(BareKey::Down));
    }
    let serialized = menu.serialize(0, 0, 10, 6);
    assert!(serialized.contains(";ind,sel=2,above=1,below=5;"));
    assert_eq!(
        menu.handle_mouse(Mouse::ScrollDown(2)),
        UiResponse::NotHandled
    );
    assert_eq!(menu.handle_mouse(Mouse::Hover(2, 2)), UiResponse::Consumed);
    assert!(menu.serialize(0, 0, 10, 6).contains(",hov=1,"));
    assert_eq!(
        menu.handle_mouse(Mouse::ScrollDown(2)),
        UiResponse::Consumed
    );
    assert_eq!(menu.scroll_offset(), 3);
    assert_eq!(menu.selected_index(), 3);
    assert_eq!(
        menu.handle_mouse(Mouse::LeftClick(2, 2)),
        UiResponse::Changed(choice(4, "Item 4"))
    );
    assert_eq!(
        menu.handle_mouse(Mouse::LeftClick(0, 2)),
        UiResponse::Consumed
    );
    assert_eq!(menu.scroll_offset(), 0);
}

#[test]
fn confirm_dialog_is_centered_and_handles_buttons() {
    let mut dialog = ConfirmDialog::new("Quit", "Really quit?")
        .buttons(vec!["Yes", "No", "Maybe"])
        .width(30);
    assert_eq!(dialog.serialize_centered(20, 40), "");
    dialog.open();
    let serialized = dialog.serialize_centered(20, 40);
    assert_eq!(
        serialized,
        dcs(
            "dialog",
            "5/7/30/6",
            "sel=0,nb=3,bx=2",
            &[
                enc("Quit"),
                enc("Yes"),
                enc("No"),
                enc("Maybe"),
                enc("Really quit?")
            ]
        )
    );
    assert_eq!(dialog.button_areas()[1], Rect::new(16, 11, 6, 1));
    assert_eq!(dialog.handle_key(&key(BareKey::Tab)), UiResponse::Consumed);
    assert_eq!(
        dialog.handle_key(&key(BareKey::Right)),
        UiResponse::Consumed
    );
    assert_eq!(dialog.selected_index(), 2);
    assert_eq!(dialog.handle_key(&shift_tab()), UiResponse::Consumed);
    assert_eq!(dialog.handle_key(&ch('x')), UiResponse::Consumed);
    assert_eq!(
        dialog.handle_key(&key(BareKey::Enter)),
        UiResponse::Submitted(choice(1, "No"))
    );
    assert!(!dialog.is_open());
    assert_eq!(
        dialog.handle_key(&key(BareKey::Enter)),
        UiResponse::NotHandled
    );
    dialog.open();
    dialog.serialize_centered(20, 40);
    assert_eq!(
        dialog.handle_mouse(Mouse::LeftClick(0, 0)),
        UiResponse::Consumed
    );
    assert_eq!(
        dialog.handle_mouse(Mouse::LeftClick(11, 8)),
        UiResponse::Submitted(choice(0, "Yes"))
    );
    dialog.open();
    assert_eq!(dialog.handle_key(&key(BareKey::Esc)), UiResponse::Cancelled);
    assert!(!dialog.is_open());
}

#[test]
fn confirm_dialog_wraps_its_message() {
    let mut dialog = ConfirmDialog::new("T", "one two three four five six").width(16);
    let serialized = dialog.serialize(0, 0, 16, 20);
    assert!(serialized.contains(&format!(
        ";{};{};{}\u{1b}\\",
        enc("one two"),
        enc("three four"),
        enc("five six")
    )));
    assert!(serialized.contains("0/0/16/8;"));
}

fn sample_group() -> FocusGroup<&'static str> {
    FocusGroup::new()
        .with("name", TextInput::empty().label("Name"))
        .with("skip", Button::new("Skip").disabled())
        .with("wrap", Toggle::new("Wrap", false))
        .with("mode", sample_dropdown())
}

#[test]
fn focus_group_moves_focus_with_tab_and_skips_disabled() {
    let mut group = sample_group();
    assert_eq!(
        group.handle_key(&key(BareKey::Tab)),
        FocusEvent::FocusChanged("name")
    );
    assert!(group.text_input(&"name").unwrap().is_focused());
    assert_eq!(
        group.handle_key(&key(BareKey::Tab)),
        FocusEvent::FocusChanged("wrap")
    );
    assert!(!group.text_input(&"name").unwrap().is_focused());
    assert_eq!(
        group.handle_key(&shift_tab()),
        FocusEvent::FocusChanged("name")
    );
    assert_eq!(
        group.handle_key(&shift_tab()),
        FocusEvent::FocusChanged("mode")
    );
    assert_eq!(
        group.handle_key(&key(BareKey::Tab)),
        FocusEvent::FocusChanged("name")
    );
    let mut no_wrap = sample_group().wrap(false);
    no_wrap.focus(&"mode");
    assert_eq!(
        no_wrap.handle_key(&key(BareKey::Tab)),
        FocusEvent::NotHandled
    );
}

#[test]
fn focus_group_routes_keys_to_the_focused_element() {
    let mut group = sample_group();
    group.focus(&"wrap");
    assert_eq!(
        group.handle_key(&ch(' ')),
        FocusEvent::Element {
            key: "wrap",
            response: UiResponse::Changed(UiValue::Bool(true))
        }
    );
    group.focus(&"name");
    assert_eq!(
        group.handle_key(&ch('q')),
        FocusEvent::Element {
            key: "name",
            response: UiResponse::Changed(UiValue::Text("q".to_owned()))
        }
    );
    assert_eq!(group.handle_key(&key(BareKey::Up)), FocusEvent::NotHandled);
    assert!(!group.focus(&"skip"));
}

#[test]
fn focus_group_routes_clicks_by_position_and_gives_overlays_priority() {
    let mut group = sample_group();
    group.text_input_mut(&"name").unwrap().serialize(0, 0, 20);
    group.button_mut(&"skip").unwrap().serialize(0, 1);
    group.toggle_mut(&"wrap").unwrap().serialize(0, 2);
    group.dropdown_mut(&"mode").unwrap().serialize(0, 3, 20);
    assert_eq!(
        group.handle_mouse(Mouse::LeftClick(2, 7)),
        FocusEvent::Element {
            key: "wrap",
            response: UiResponse::Changed(UiValue::Bool(true))
        }
    );
    assert_eq!(group.focused_key(), Some(&"wrap"));
    assert_eq!(
        group.handle_mouse(Mouse::LeftClick(3, 10)),
        FocusEvent::Element {
            key: "mode",
            response: UiResponse::Consumed
        }
    );
    assert!(group.has_open_overlay());
    let overlay = group.serialize_overlays(20, 40);
    assert!(overlay.starts_with("\u{1b}Pzmenu;5/4/15/5;"));
    assert_eq!(
        group.handle_mouse(Mouse::LeftClick(6, 8)),
        FocusEvent::Element {
            key: "mode",
            response: UiResponse::Changed(choice(1, "titles"))
        }
    );
    assert!(!group.has_open_overlay());
    assert_eq!(
        group.handle_mouse(Mouse::LeftClick(1, 2)),
        FocusEvent::Element {
            key: "skip",
            response: UiResponse::Consumed
        }
    );
    assert_eq!(group.focused_key(), Some(&"mode"));
    assert_eq!(
        group.handle_mouse(Mouse::LeftClick(15, 2)),
        FocusEvent::NotHandled
    );
    group.clear_areas();
    assert_eq!(
        group.handle_mouse(Mouse::LeftClick(2, 7)),
        FocusEvent::NotHandled
    );
}

#[test]
fn focus_group_closes_an_open_dropdown_on_tab() {
    let mut group = sample_group();
    group.focus(&"mode");
    group.dropdown_mut(&"mode").unwrap().serialize(0, 0, 20);
    group.handle_key(&key(BareKey::Enter));
    assert!(group.dropdown(&"mode").unwrap().is_open());
    assert_eq!(
        group.handle_key(&key(BareKey::Tab)),
        FocusEvent::FocusChanged("name")
    );
    assert!(!group.dropdown(&"mode").unwrap().is_open());
}

#[test]
fn ui_response_display_matches_the_status_line_format() {
    assert_eq!(
        UiResponse::Changed(choice(0, "titles")).to_string(),
        "Changed(\"titles\")"
    );
    assert_eq!(
        UiResponse::Changed(UiValue::Number(3)).to_string(),
        "Changed(3)"
    );
    assert_eq!(UiResponse::Activated.to_string(), "Activated");
}

#[test]
fn hovering_sets_and_clears_the_hover_look() {
    let mut button = Button::new("Go");
    button.serialize(0, 0);
    assert_eq!(
        button.handle_mouse(Mouse::Hover(0, 2)),
        UiResponse::Consumed
    );
    assert!(button.serialize(0, 0).contains(";h;"));
    assert_eq!(
        button.handle_mouse(Mouse::Hover(3, 2)),
        UiResponse::Consumed
    );
    assert_eq!(
        button.handle_mouse(Mouse::Hover(3, 2)),
        UiResponse::NotHandled
    );
    assert!(button.serialize(0, 0).contains(";;"));
    let mut toggle = Toggle::new("", true);
    toggle.serialize(0, 0);
    toggle.handle_mouse(Mouse::Hover(0, 1));
    assert!(toggle.serialize(0, 0).contains(";on,h,lw=0;"));
    let mut stepper = NumberStepper::new("", 5);
    stepper.serialize(0, 0);
    stepper.handle_mouse(Mouse::Hover(0, 6));
    assert!(stepper.serialize(0, 0).contains(";h,hinc,lw=0;"));
    let mut dialog = ConfirmDialog::new("T", "m").opened();
    dialog.serialize_centered(20, 40);
    let area = dialog.button_areas()[1];
    dialog.handle_mouse(Mouse::Hover(area.y as isize, area.x));
    assert!(dialog.serialize_centered(20, 40).contains(",hb=1;"));
    assert_eq!(dialog.selected_index(), 0);
}

#[test]
fn focus_group_sends_the_wheel_to_the_hovered_element() {
    let items: Vec<MenuItem> = (0..10)
        .map(|i| MenuItem::new(format!("Item {}", i)))
        .collect();
    let mut group = FocusGroup::new()
        .with("first", MenuList::new(items.clone()))
        .with("second", MenuList::new(items));
    group
        .menu_list_mut(&"first")
        .unwrap()
        .serialize(0, 0, 10, 5);
    group
        .menu_list_mut(&"second")
        .unwrap()
        .serialize(20, 0, 10, 5);
    group.focus(&"first");
    group.handle_mouse(Mouse::Hover(2, 22));
    assert_eq!(
        group.handle_mouse(Mouse::ScrollDown(1)),
        FocusEvent::Element {
            key: "second",
            response: UiResponse::Consumed
        }
    );
    assert_eq!(group.menu_list(&"first").unwrap().scroll_offset(), 0);
    assert_eq!(group.menu_list(&"second").unwrap().scroll_offset(), 1);
    group.handle_mouse(Mouse::Hover(20, 50));
    assert_eq!(
        group.handle_mouse(Mouse::ScrollDown(1)),
        FocusEvent::NotHandled
    );
}

#[test]
fn fuzzy_match_indices_finds_letters_in_order() {
    assert_eq!(fuzzy_match_indices("ape", "grape"), Some(vec![2, 3, 4]));
    assert_eq!(fuzzy_match_indices("gpe", "grape"), Some(vec![0, 3, 4]));
    assert_eq!(fuzzy_match_indices("AP", "apple"), Some(vec![0, 1]));
    assert_eq!(fuzzy_match_indices("", "anything"), Some(vec![]));
    assert_eq!(fuzzy_match_indices("zz", "pizza"), Some(vec![2, 3]));
    assert_eq!(fuzzy_match_indices("xyz", "pizza"), None);
}

#[test]
fn confirm_dialog_reserves_footer_rows_below_the_buttons() {
    let mut dialog = ConfirmDialog::new("T", "Go?")
        .buttons(vec!["Yes", "No"])
        .footer_rows(2)
        .opened();
    assert_eq!(dialog.size_for(80).1, 8);
    dialog.serialize(0, 0, 30, 20);
    assert_eq!(dialog.button_areas()[0].y, 4);
    assert_eq!(dialog.footer_area(), Some(Rect::new(2, 5, 26, 2)));
    let mut centered = ConfirmDialog::new("T", "Go?").centered().opened();
    assert!(centered.serialize(0, 0, 30, 20).contains(",c;"));
    assert!(!dialog.serialize(0, 0, 30, 20).contains(",c;"));
}

#[test]
fn dragging_the_scroll_bar_scrolls_until_the_button_is_released() {
    let mut view = ScrollView::new(50);
    view.layout(0, 2, 20, 10);
    let gutter_x = 11;
    assert!(view
        .handle_mouse(Mouse::LeftClick(2, gutter_x))
        .is_handled());
    assert!(view.is_dragging());
    assert_eq!(view.offset(), 0);
    view.handle_mouse(Mouse::Hold(11, 0));
    assert_eq!(view.offset(), 40);
    view.handle_mouse(Mouse::Hold(6, 3));
    assert!(view.offset() > 0 && view.offset() < 40);
    view.handle_mouse(Mouse::Release(6, 3));
    assert!(!view.is_dragging());
    let offset = view.offset();
    assert_eq!(
        view.handle_mouse(Mouse::Hold(11, 0)),
        UiResponse::NotHandled
    );
    assert_eq!(view.offset(), offset);
}

#[test]
fn wrapping_text_keeps_its_colours_on_every_line() {
    let message = Text::from("Delete Alt n from Normal mode?").color_range(3, 7..12);
    let lines = message.wrap(9);
    let contents: Vec<&str> = lines.iter().map(|line| line.content()).collect();
    assert_eq!(contents, vec!["Delete", "Alt n", "from", "Normal", "mode?"]);
    assert_eq!(
        lines[1].serialize(),
        Text::from("Alt n").color_range(3, 0..5).serialize()
    );
    assert_eq!(lines[0].serialize(), Text::from("Delete").serialize());
    assert_eq!(
        message.wrap(60)[0].serialize(),
        Text::from("Delete Alt n from Normal mode?")
            .color_range(3, 7..12)
            .serialize()
    );
    let apart = Text::from("Delete x y").color_range(3, 7..8);
    assert_eq!(apart.wrap(60)[0].serialize(), apart.serialize());
    let contents: Vec<String> = Text::from("abcdefghij\none")
        .wrap(4)
        .iter()
        .map(|line| line.content().to_owned())
        .collect();
    assert_eq!(contents, vec!["abcd", "efgh", "ij", "one"]);
}

#[test]
fn labels_and_messages_carry_their_text_styles_to_the_server() {
    let label = Text::from("Delete main?").color_range(3, 7..11);
    let serialized = label.serialize();
    assert!(TextInput::new("")
        .label(label.clone())
        .serialize(0, 0, 40)
        .contains(&serialized));
    assert!(Dropdown::new(label.clone(), vec!["a"])
        .serialize(0, 0, 40)
        .contains(&serialized));
    assert!(Toggle::new(label.clone(), false)
        .serialize(0, 0)
        .contains(&serialized));
    assert!(NumberStepper::new(label.clone(), 1)
        .serialize(0, 0)
        .contains(&serialized));
    assert!(Button::new(label.clone())
        .serialize(0, 0)
        .contains(&serialized));
    assert!(ConfirmDialog::new("Confirm", label.clone())
        .opened()
        .serialize(0, 0, 60, 20)
        .contains(&serialized));
    assert_eq!(Button::new(label.clone()).label(), "Delete main?");
    assert_eq!(
        Button::new("Go").serialize(0, 0),
        Button::new(Text::from("Go")).serialize(0, 0)
    );
    assert!(Button::new("Go").serialize(0, 0).contains(&enc("Go")));
}
