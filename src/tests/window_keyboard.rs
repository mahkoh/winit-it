use crate::backend::Key::{KeyL, KeyLeftshift};
use crate::backend::{Instance};
use winit::event::{ElementState};
use winit::keyboard::{KeyCode, KeyLocation, Key as WKey, ModifiersState};

test!(run);

async fn run(instance: &dyn Instance) {
    let el = instance.create_event_loop();
    let window = el.create_window(Default::default());
    window.set_visible(true);
    instance.mapped(&window).await;
    let seat = instance.default_seat();
    seat.focus(&window);
    let kb = seat.add_keyboard();

    {
        log::info!("Testing L");
        // L Press
        // L Release
        kb.press(KeyL);
        for i in 0..2 {
            let (_, ki) = el.window_keyboard_input().await;
            assert_eq!(ki.event.physical_key, KeyCode::KeyL);
            assert_eq!(ki.event.logical_key, WKey::Character("l"));
            assert_eq!(ki.event.text, Some("l"));
            assert_eq!(ki.event.location, KeyLocation::Standard);
            if i == 0 {
                assert_eq!(ki.event.state, ElementState::Pressed);
            } else {
                assert_eq!(ki.event.state, ElementState::Released);
            }
            assert_eq!(ki.event.repeat, false);
            #[cfg(have_mod_supplement)]
            {
                assert_eq!(ki.event.mod_supplement.key_without_modifiers, WKey::Character("l"));
                assert_eq!(ki.event.mod_supplement.text_with_all_modifiers.as_deref(), Some("l"));
            }
        }
    }

    {
        log::info!("Testing Shift-L");
        // LeftShift Press
        // L Press
        // L Release
        // LeftShift Release
        {
            let _shift = kb.press(KeyLeftshift);
            kb.press(KeyL);
        }
        // 0: Shift pressed
        // 1: Modifiers changed
        // 2: L pressed
        // 3: L released
        // 4: Shift released
        // 5: Modifiers changed
        for i in 0..6 {
            if i == 1 || i == 5 {
                let (_, mo) = el.window_modifiers().await;
                if i == 1 {
                    assert_eq!(mo, ModifiersState::SHIFT);
                } else {
                    assert_eq!(mo, ModifiersState::empty());
                }
            } else {
                let (_, ki) = el.window_keyboard_input().await;
                assert_eq!(ki.event.repeat, false);
                if i == 0 || i == 4 {
                    assert_eq!(ki.event.physical_key, KeyCode::ShiftLeft);
                    assert_eq!(ki.event.logical_key, WKey::Shift);
                    assert_eq!(ki.event.text, None);
                    assert_eq!(ki.event.location, KeyLocation::Left);
                } else {
                    assert_eq!(ki.event.physical_key, KeyCode::KeyL);
                    assert_eq!(ki.event.logical_key, WKey::Character("L"));
                    assert_eq!(ki.event.text, Some("L"));
                    assert_eq!(ki.event.location, KeyLocation::Standard);
                }
                if i == 0 || i == 2 {
                    assert_eq!(ki.event.state, ElementState::Pressed);
                } else {
                    assert_eq!(ki.event.state, ElementState::Released);
                }
            }
        }
    }

    {
        log::info!("Testing Shift-L (not nested)");
        // LeftShift Press
        // L Press
        // LeftShift Release
        // L Release
        {
            let shift = kb.press(KeyLeftshift);
            let _l = kb.press(KeyL);
            drop(shift);
        }
        // 0: Shift pressed
        // 1: Modifiers changed
        // 2: L pressed
        // 3: Shift released
        // 4: Modifiers changed
        // 5: L released
        for i in 0..6 {
            if i == 1 || i == 4 {
                let (_, mo) = el.window_modifiers().await;
                if i == 1 {
                    assert_eq!(mo, ModifiersState::SHIFT);
                } else {
                    assert_eq!(mo, ModifiersState::empty());
                }
            } else {
                let (_, ki) = el.window_keyboard_input().await;
                assert_eq!(ki.event.repeat, false);
                if i == 0 || i == 3 {
                    assert_eq!(ki.event.physical_key, KeyCode::ShiftLeft);
                    assert_eq!(ki.event.logical_key, WKey::Shift);
                    assert_eq!(ki.event.text, None);
                    assert_eq!(ki.event.location, KeyLocation::Left);
                } else {
                    assert_eq!(ki.event.physical_key, KeyCode::KeyL);
                    if i == 2 {
                        assert_eq!(ki.event.logical_key, WKey::Character("L"));
                        assert_eq!(ki.event.text, Some("L"));
                    } else {
                        assert_eq!(ki.event.logical_key, WKey::Character("l"));
                        assert_eq!(ki.event.text, Some("l"));
                    }
                    assert_eq!(ki.event.location, KeyLocation::Standard);
                }
                if i == 0 || i == 2 {
                    assert_eq!(ki.event.state, ElementState::Pressed);
                } else {
                    assert_eq!(ki.event.state, ElementState::Released);
                }
            }
        }
    }
}
