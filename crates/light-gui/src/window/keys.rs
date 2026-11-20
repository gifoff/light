//! Вспомогательные функции для обработки клавиатуры: преобразование событий
//! клавиатуры в буквы и `KeyInfo`, маппинг кнопок мыши.

use winit::event::{KeyEvent, MouseButton};
use winit::keyboard::{Key, KeyCode, ModifiersState, PhysicalKey};

use light_vdom::{KeyInfo, MouseButton as DomButton};

pub(super) fn control_letter(event: &KeyEvent) -> Option<String> {
    if let PhysicalKey::Code(code) = event.physical_key
        && let Some(letter) = keycode_letter(code)
    {
        return Some(letter);
    }
    let Key::Character(ch) = &event.logical_key else {
        return None;
    };
    let mapped: String = ch
        .chars()
        .map(|c| {
            let code = c as u32;
            if (1..=26).contains(&code) {
                char::from(b'a' + (code as u8 - 1))
            } else {
                c
            }
        })
        .collect();
    Some(mapped.to_ascii_lowercase())
}

/// Строчная латинская буква для физической клавиши `A`..`Z` (без зависимости
/// от активной раскладки).
pub(super) fn keycode_letter(code: KeyCode) -> Option<String> {
    let letter = match code {
        KeyCode::KeyA => 'a',
        KeyCode::KeyB => 'b',
        KeyCode::KeyC => 'c',
        KeyCode::KeyD => 'd',
        KeyCode::KeyE => 'e',
        KeyCode::KeyF => 'f',
        KeyCode::KeyG => 'g',
        KeyCode::KeyH => 'h',
        KeyCode::KeyI => 'i',
        KeyCode::KeyJ => 'j',
        KeyCode::KeyK => 'k',
        KeyCode::KeyL => 'l',
        KeyCode::KeyM => 'm',
        KeyCode::KeyN => 'n',
        KeyCode::KeyO => 'o',
        KeyCode::KeyP => 'p',
        KeyCode::KeyQ => 'q',
        KeyCode::KeyR => 'r',
        KeyCode::KeyS => 's',
        KeyCode::KeyT => 't',
        KeyCode::KeyU => 'u',
        KeyCode::KeyV => 'v',
        KeyCode::KeyW => 'w',
        KeyCode::KeyX => 'x',
        KeyCode::KeyY => 'y',
        KeyCode::KeyZ => 'z',
        _ => return None,
    };
    Some(letter.to_string())
}

/// Формирует [`KeyInfo`] из события клавиатуры.
pub(super) fn key_info(event: &KeyEvent, modifiers: ModifiersState) -> KeyInfo {
    let key = match &event.logical_key {
        Key::Character(ch) => ch.to_string(),
        Key::Named(named) => format!("{named:?}"),
        _ => String::new(),
    };
    KeyInfo::new(
        key,
        modifiers.shift_key(),
        modifiers.control_key(),
        modifiers.alt_key(),
    )
}

/// Отображение кнопки мыши winit в событие виртуального дерева.
pub(super) fn map_button(button: MouseButton) -> DomButton {
    match button {
        MouseButton::Left => DomButton::Left,
        MouseButton::Right => DomButton::Right,
        MouseButton::Middle => DomButton::Middle,
        _ => DomButton::Left,
    }
}

/// ТЗ 0.14.3-fix7: восстановление geometry (как и state) допустимо ТОЛЬКО при
/// hot-swap (`LIGHT_RESUME_FROM_SWAP=1`). На cold start окно появляется в
/// дефолтной позиции (центр экрана), а устаревшая geometry из прошлой сессии
/// не применяется — иначе холодный старт брал бы «сломанную» позицию
/// (например `(-32000, -32000)` от свёрнутого окна).
pub(super) fn is_resume_from_swap() -> bool {
    std::env::var("LIGHT_RESUME_FROM_SWAP").is_ok()
}
