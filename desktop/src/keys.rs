//! Map egui keys to PC AT scancodes used by RDP FastPath / VNC keysym helper.

use egui::Key;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyTransition {
    pub key: Key,
    pub pressed: bool,
}

/// Restores keyboard transitions that egui-winit replaces with clipboard events.
///
/// On Ctrl+C/X/V, egui-winit emits `Copy`/`Cut`/`Paste` instead of the key-down
/// event, but still emits the normal key-up event. `Paste` is not emitted at all
/// when the local clipboard is empty, so an unmatched Ctrl+V key-up also needs a
/// synthetic key-down before it is forwarded to the remote session.
#[derive(Default)]
pub struct RemoteKeyboardState {
    clipboard_keys_down: [bool; 3],
}

impl RemoteKeyboardState {
    pub fn transitions(&mut self, event: &egui::Event) -> Vec<KeyTransition> {
        match event {
            egui::Event::Copy => self.clipboard_key_down(Key::C),
            egui::Event::Cut => self.clipboard_key_down(Key::X),
            egui::Event::Paste(_) => self.clipboard_key_down(Key::V),
            egui::Event::Key {
                key,
                pressed,
                repeat: _,
                modifiers,
                ..
            } => {
                let transition = KeyTransition {
                    key: *key,
                    pressed: *pressed,
                };
                let Some(index) = clipboard_key_index(*key) else {
                    return vec![transition];
                };

                if *pressed {
                    self.clipboard_keys_down[index] = true;
                    return vec![transition];
                }

                let saw_key_down = std::mem::take(&mut self.clipboard_keys_down[index]);
                if modifiers.ctrl && !saw_key_down {
                    // egui-winit suppresses Ctrl+V completely when the local clipboard is empty.
                    vec![
                        KeyTransition {
                            key: *key,
                            pressed: true,
                        },
                        transition,
                    ]
                } else {
                    vec![transition]
                }
            }
            _ => Vec::new(),
        }
    }

    fn clipboard_key_down(&mut self, key: Key) -> Vec<KeyTransition> {
        let index = clipboard_key_index(key).expect("clipboard shortcut key");
        if std::mem::replace(&mut self.clipboard_keys_down[index], true) {
            Vec::new()
        } else {
            vec![KeyTransition { key, pressed: true }]
        }
    }
}

fn clipboard_key_index(key: Key) -> Option<usize> {
    match key {
        Key::C => Some(0),
        Key::X => Some(1),
        Key::V => Some(2),
        _ => None,
    }
}

/// Returns (scancode, is_extended_hint).
pub fn egui_key_to_scancode(key: Key) -> Option<(i32, bool)> {
    Some(match key {
        Key::Escape => (0x01, false),
        Key::Tab => (0x0F, false),
        Key::Backspace => (0x0E, false),
        Key::Enter => (0x1C, false),
        Key::Space => (0x39, false),

        Key::Num0 => (0x0B, false),
        Key::Num1 => (0x02, false),
        Key::Num2 => (0x03, false),
        Key::Num3 => (0x04, false),
        Key::Num4 => (0x05, false),
        Key::Num5 => (0x06, false),
        Key::Num6 => (0x07, false),
        Key::Num7 => (0x08, false),
        Key::Num8 => (0x09, false),
        Key::Num9 => (0x0A, false),

        Key::A => (0x1E, false),
        Key::B => (0x30, false),
        Key::C => (0x2E, false),
        Key::D => (0x20, false),
        Key::E => (0x12, false),
        Key::F => (0x21, false),
        Key::G => (0x22, false),
        Key::H => (0x23, false),
        Key::I => (0x17, false),
        Key::J => (0x24, false),
        Key::K => (0x25, false),
        Key::L => (0x26, false),
        Key::M => (0x32, false),
        Key::N => (0x31, false),
        Key::O => (0x18, false),
        Key::P => (0x19, false),
        Key::Q => (0x10, false),
        Key::R => (0x13, false),
        Key::S => (0x1F, false),
        Key::T => (0x14, false),
        Key::U => (0x16, false),
        Key::V => (0x2F, false),
        Key::W => (0x11, false),
        Key::X => (0x2D, false),
        Key::Y => (0x15, false),
        Key::Z => (0x2C, false),

        Key::F1 => (0x3B, false),
        Key::F2 => (0x3C, false),
        Key::F3 => (0x3D, false),
        Key::F4 => (0x3E, false),
        Key::F5 => (0x3F, false),
        Key::F6 => (0x40, false),
        Key::F7 => (0x41, false),
        Key::F8 => (0x42, false),
        Key::F9 => (0x43, false),
        Key::F10 => (0x44, false),
        Key::F11 => (0x57, false),
        Key::F12 => (0x58, false),

        Key::ArrowLeft => (0x4B, true),
        Key::ArrowUp => (0x48, true),
        Key::ArrowRight => (0x4D, true),
        Key::ArrowDown => (0x50, true),
        Key::Home => (0x47, true),
        Key::End => (0x4F, true),
        Key::PageUp => (0x49, true),
        Key::PageDown => (0x51, true),
        Key::Insert => (0x52, true),
        Key::Delete => (0x53, true),

        Key::Minus => (0x0C, false),
        Key::Equals | Key::Plus => (0x0D, false),
        Key::OpenBracket => (0x1A, false),
        Key::CloseBracket => (0x1B, false),
        Key::Backslash | Key::Pipe => (0x2B, false),
        Key::Semicolon | Key::Colon => (0x27, false),
        Key::Quote => (0x28, false),
        Key::Backtick => (0x29, false),
        Key::Comma => (0x33, false),
        Key::Period => (0x34, false),
        Key::Slash | Key::Questionmark => (0x35, false),

        Key::F13 => (0x64, false),
        Key::F14 => (0x65, false),
        Key::F15 => (0x66, false),
        Key::F16 => (0x67, false),
        Key::F17 => (0x68, false),
        Key::F18 => (0x69, false),
        Key::F19 => (0x6A, false),
        Key::F20 => (0x6B, false),

        _ => return None,
    })
}

pub fn is_extended_scancode(scancode: i32) -> bool {
    matches!(
        scancode,
        0x4B | 0x48 | 0x4D | 0x50 | 0x47 | 0x4F | 0x49 | 0x51 | 0x52 | 0x53 | 0x37 | 0x5D
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Modifiers;

    fn key_event(key: Key, pressed: bool, modifiers: Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn restores_copy_key_down_and_uses_native_key_up() {
        let mut state = RemoteKeyboardState::default();

        assert_eq!(
            state.transitions(&egui::Event::Copy),
            vec![KeyTransition {
                key: Key::C,
                pressed: true,
            }]
        );
        assert_eq!(
            state.transitions(&key_event(Key::C, false, Modifiers::CTRL)),
            vec![KeyTransition {
                key: Key::C,
                pressed: false,
            }]
        );
    }

    #[test]
    fn restores_paste_key_down_and_uses_native_key_up() {
        let mut state = RemoteKeyboardState::default();

        assert_eq!(
            state.transitions(&egui::Event::Paste("clipboard text".into())),
            vec![KeyTransition {
                key: Key::V,
                pressed: true,
            }]
        );
        assert_eq!(
            state.transitions(&key_event(Key::V, false, Modifiers::CTRL)),
            vec![KeyTransition {
                key: Key::V,
                pressed: false,
            }]
        );
    }

    #[test]
    fn reconstructs_paste_when_empty_local_clipboard_suppresses_key_down() {
        let mut state = RemoteKeyboardState::default();

        assert_eq!(
            state.transitions(&key_event(Key::V, false, Modifiers::CTRL)),
            vec![
                KeyTransition {
                    key: Key::V,
                    pressed: true,
                },
                KeyTransition {
                    key: Key::V,
                    pressed: false,
                },
            ]
        );
    }

    #[test]
    fn restores_cut_and_leaves_normal_keys_unchanged() {
        let mut state = RemoteKeyboardState::default();

        assert_eq!(
            state.transitions(&egui::Event::Cut),
            vec![KeyTransition {
                key: Key::X,
                pressed: true,
            }]
        );
        assert_eq!(
            state.transitions(&key_event(Key::A, true, Modifiers::CTRL)),
            vec![KeyTransition {
                key: Key::A,
                pressed: true,
            }]
        );
    }

    #[test]
    fn maps_plus_and_special_keys_to_scancodes() {
        assert_eq!(egui_key_to_scancode(Key::Plus), Some((0x0D, false)));
        assert_eq!(egui_key_to_scancode(Key::Equals), Some((0x0D, false)));
        assert_eq!(egui_key_to_scancode(Key::Colon), Some((0x27, false)));
        assert_eq!(egui_key_to_scancode(Key::Pipe), Some((0x2B, false)));
        assert_eq!(egui_key_to_scancode(Key::Questionmark), Some((0x35, false)));
    }

    #[test]
    fn standard_letters_are_not_extended_scancodes() {
        let test_letters = [
            Key::Q, Key::W, Key::E, Key::R, Key::T, Key::Y, Key::U, Key::I, Key::O, Key::P,
            Key::A, Key::S, Key::D, Key::F, Key::G, Key::H, Key::J, Key::K, Key::L,
            Key::Z, Key::X, Key::C, Key::V, Key::B, Key::N, Key::M,
        ];
        for letter in test_letters {
            if let Some((scancode, _)) = egui_key_to_scancode(letter) {
                assert!(
                    !is_extended_scancode(scancode),
                    "Letter {:?} with scancode 0x{:02X} must not be marked as extended scancode",
                    letter, scancode
                );
            }
        }
    }
}
