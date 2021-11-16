use crate::backend::Instance;
use crate::keyboard::Key::{KeyL, KeyLeftshift, KeyQ, KeyRightalt, KeyRightctrl};
use crate::keyboard::Layout;
use winit::event::ElementState;
use winit::keyboard::{Key as WKey, KeyCode, KeyLocation, ModifiersState};

test!(run);

async fn run(instance: &dyn Instance) {
    let el = instance.create_event_loop();
    let window = el.create_window(Default::default());
    window.mapped(true).await;
    let seat = instance.default_seat();
    seat.focus(&*window);
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
                assert_eq!(
                    ki.event.mod_supplement.key_without_modifiers,
                    WKey::Character("l")
                );
                assert_eq!(
                    ki.event.mod_supplement.text_with_all_modifiers.as_deref(),
                    Some("l")
                );
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
                    #[cfg(have_mod_supplement)]
                    {
                        assert_eq!(ki.event.mod_supplement.key_without_modifiers, WKey::Shift);
                        assert_eq!(
                            ki.event.mod_supplement.text_with_all_modifiers.as_deref(),
                            None
                        );
                    }
                } else {
                    assert_eq!(ki.event.physical_key, KeyCode::KeyL);
                    assert_eq!(ki.event.logical_key, WKey::Character("L"));
                    assert_eq!(ki.event.text, Some("L"));
                    assert_eq!(ki.event.location, KeyLocation::Standard);
                    #[cfg(have_mod_supplement)]
                    {
                        assert_eq!(
                            ki.event.mod_supplement.key_without_modifiers,
                            WKey::Character("l")
                        );
                        assert_eq!(
                            ki.event.mod_supplement.text_with_all_modifiers.as_deref(),
                            Some("L")
                        );
                    }
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
                    #[cfg(have_mod_supplement)]
                    {
                        assert_eq!(ki.event.mod_supplement.key_without_modifiers, WKey::Shift);
                        assert_eq!(
                            ki.event.mod_supplement.text_with_all_modifiers.as_deref(),
                            None
                        );
                    }
                } else {
                    assert_eq!(ki.event.physical_key, KeyCode::KeyL);
                    #[cfg(have_mod_supplement)]
                    assert_eq!(
                        ki.event.mod_supplement.key_without_modifiers,
                        WKey::Character("l")
                    );
                    if i == 2 {
                        assert_eq!(ki.event.logical_key, WKey::Character("L"));
                        assert_eq!(ki.event.text, Some("L"));
                        #[cfg(have_mod_supplement)]
                        assert_eq!(
                            ki.event.mod_supplement.text_with_all_modifiers.as_deref(),
                            Some("L")
                        );
                    } else {
                        assert_eq!(ki.event.logical_key, WKey::Character("l"));
                        assert_eq!(ki.event.text, Some("l"));
                        #[cfg(have_mod_supplement)]
                        assert_eq!(
                            ki.event.mod_supplement.text_with_all_modifiers.as_deref(),
                            Some("l")
                        );
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

    {
        log::info!("Testing Ctrl-L");
        // RightCtrl Press
        // L Press
        // L Release
        // RightCtrl Release
        {
            let _shift = kb.press(KeyRightctrl);
            kb.press(KeyL);
        }
        // 0: Ctrl pressed
        // 1: Modifiers changed
        // 2: L pressed
        // 3: L released
        // 4: Ctrl released
        // 5: Modifiers changed
        for i in 0..6 {
            if i == 1 || i == 5 {
                let (_, mo) = el.window_modifiers().await;
                if i == 1 {
                    assert_eq!(mo, ModifiersState::CONTROL);
                } else {
                    assert_eq!(mo, ModifiersState::empty());
                }
            } else {
                let (_, ki) = el.window_keyboard_input().await;
                assert_eq!(ki.event.repeat, false);
                if i == 0 || i == 4 {
                    assert_eq!(ki.event.physical_key, KeyCode::ControlRight);
                    assert_eq!(ki.event.logical_key, WKey::Control);
                    assert_eq!(ki.event.text, None);
                    assert_eq!(ki.event.location, KeyLocation::Right);
                    #[cfg(have_mod_supplement)]
                    {
                        assert_eq!(ki.event.mod_supplement.key_without_modifiers, WKey::Control);
                        assert_eq!(
                            ki.event.mod_supplement.text_with_all_modifiers.as_deref(),
                            None
                        );
                    }
                } else {
                    assert_eq!(ki.event.physical_key, KeyCode::KeyL);
                    assert_eq!(ki.event.logical_key, WKey::Character("l"));
                    assert_eq!(ki.event.text, Some("l"));
                    assert_eq!(ki.event.location, KeyLocation::Standard);
                    #[cfg(have_mod_supplement)]
                    {
                        assert_eq!(
                            ki.event.mod_supplement.key_without_modifiers,
                            WKey::Character("l")
                        );
                        assert_eq!(
                            ki.event.mod_supplement.text_with_all_modifiers.as_deref(),
                            Some("\x0c")
                        );
                    }
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
        log::info!("Testing Ctrl-Shift-L");
        // RightCtrl Press
        // LiftShift Press
        // L Press
        // L Release
        // RightCtrl Release
        // LiftShift Release
        {
            let ctrl = kb.press(KeyRightctrl);
            let _shift = kb.press(KeyLeftshift);
            kb.press(KeyL);
            drop(ctrl);
        }
        // 0: Ctrl pressed
        // 1: Modifiers changed
        // 2: Shift pressed
        // 3: Modifiers changed
        // 4: L pressed
        // 5: L released
        // 6: Ctrl released
        // 7: Modifiers changed
        // 8: Shift released
        // 9: Modifiers changed
        for i in 0..10 {
            if matches!(i, 1 | 3 | 7 | 9) {
                let (_, mo) = el.window_modifiers().await;
                match i {
                    1 => assert_eq!(mo, ModifiersState::CONTROL),
                    3 => assert_eq!(mo, ModifiersState::CONTROL | ModifiersState::SHIFT),
                    7 => assert_eq!(mo, ModifiersState::SHIFT),
                    9 => assert_eq!(mo, ModifiersState::empty()),
                    _ => unreachable!(),
                }
            } else {
                let (_, ki) = el.window_keyboard_input().await;
                assert_eq!(ki.event.repeat, false);
                if matches!(i, 0 | 6) {
                    assert_eq!(ki.event.physical_key, KeyCode::ControlRight);
                    assert_eq!(ki.event.logical_key, WKey::Control);
                    assert_eq!(ki.event.text, None);
                    assert_eq!(ki.event.location, KeyLocation::Right);
                    #[cfg(have_mod_supplement)]
                    {
                        assert_eq!(ki.event.mod_supplement.key_without_modifiers, WKey::Control);
                        assert_eq!(
                            ki.event.mod_supplement.text_with_all_modifiers.as_deref(),
                            None
                        );
                    }
                } else if matches!(i, 2 | 8) {
                    assert_eq!(ki.event.physical_key, KeyCode::ShiftLeft);
                    assert_eq!(ki.event.logical_key, WKey::Shift);
                    assert_eq!(ki.event.text, None);
                    assert_eq!(ki.event.location, KeyLocation::Left);
                    #[cfg(have_mod_supplement)]
                    {
                        assert_eq!(ki.event.mod_supplement.key_without_modifiers, WKey::Shift);
                        assert_eq!(
                            ki.event.mod_supplement.text_with_all_modifiers.as_deref(),
                            None
                        );
                    }
                } else {
                    assert_eq!(ki.event.physical_key, KeyCode::KeyL);
                    assert_eq!(ki.event.logical_key, WKey::Character("L"));
                    assert_eq!(ki.event.text, Some("L"));
                    assert_eq!(ki.event.location, KeyLocation::Standard);
                    #[cfg(have_mod_supplement)]
                    {
                        assert_eq!(
                            ki.event.mod_supplement.key_without_modifiers,
                            WKey::Character("l")
                        );
                        assert_eq!(
                            ki.event.mod_supplement.text_with_all_modifiers.as_deref(),
                            Some("\x0c")
                        );
                    }
                }
                if matches!(i, 0 | 2 | 4) {
                    assert_eq!(ki.event.state, ElementState::Pressed);
                } else {
                    assert_eq!(ki.event.state, ElementState::Released);
                }
            }
        }
    }

    {
        log::info!("Testing Alt");
        // LeftAlt Press
        // LeftAlt Release
        {
            kb.press(KeyRightalt);
        }
        // 0: Alt pressed
        // 1: Modifiers changed
        // 2: Alt released
        // 3: Modifiers changed
        for i in 0..4 {
            if i == 1 || i == 3 {
                let (_, mo) = el.window_modifiers().await;
                if i == 1 {
                    assert_eq!(mo, ModifiersState::ALT);
                } else {
                    assert_eq!(mo, ModifiersState::empty());
                }
            } else {
                let (_, ki) = el.window_keyboard_input().await;
                assert_eq!(ki.event.repeat, false);
                assert_eq!(ki.event.physical_key, KeyCode::AltRight);
                assert_eq!(ki.event.logical_key, WKey::Alt);
                assert_eq!(ki.event.text, None);
                assert_eq!(ki.event.location, KeyLocation::Right);
                #[cfg(have_mod_supplement)]
                {
                    assert_eq!(ki.event.mod_supplement.key_without_modifiers, WKey::Alt);
                    assert_eq!(
                        ki.event.mod_supplement.text_with_all_modifiers.as_deref(),
                        None
                    );
                }
                if i == 0 {
                    assert_eq!(ki.event.state, ElementState::Pressed);
                } else {
                    assert_eq!(ki.event.state, ElementState::Released);
                }
            }
        }
    }
    {
        log::info!("Testing Ctrl-Shift-L");
        // RightCtrl Press
        // LiftShift Press
        // L Press
        // L Release
        // RightCtrl Release
        // LiftShift Release
        {
            let ctrl = kb.press(KeyRightctrl);
            let _shift = kb.press(KeyLeftshift);
            kb.press(KeyL);
            drop(ctrl);
        }
        // 0: Ctrl pressed
        // 1: Modifiers changed
        // 2: Shift pressed
        // 3: Modifiers changed
        // 4: L pressed
        // 5: L released
        // 6: Ctrl released
        // 7: Modifiers changed
        // 8: Shift released
        // 9: Modifiers changed
        for i in 0..10 {
            if matches!(i, 1 | 3 | 7 | 9) {
                let (_, mo) = el.window_modifiers().await;
                match i {
                    1 => assert_eq!(mo, ModifiersState::CONTROL),
                    3 => assert_eq!(mo, ModifiersState::CONTROL | ModifiersState::SHIFT),
                    7 => assert_eq!(mo, ModifiersState::SHIFT),
                    9 => assert_eq!(mo, ModifiersState::empty()),
                    _ => unreachable!(),
                }
            } else {
                let (_, ki) = el.window_keyboard_input().await;
                assert_eq!(ki.event.repeat, false);
                if matches!(i, 0 | 6) {
                    assert_eq!(ki.event.physical_key, KeyCode::ControlRight);
                    assert_eq!(ki.event.logical_key, WKey::Control);
                    assert_eq!(ki.event.text, None);
                    assert_eq!(ki.event.location, KeyLocation::Right);
                    #[cfg(have_mod_supplement)]
                    {
                        assert_eq!(ki.event.mod_supplement.key_without_modifiers, WKey::Control);
                        assert_eq!(
                            ki.event.mod_supplement.text_with_all_modifiers.as_deref(),
                            None
                        );
                    }
                } else if matches!(i, 2 | 8) {
                    assert_eq!(ki.event.physical_key, KeyCode::ShiftLeft);
                    assert_eq!(ki.event.logical_key, WKey::Shift);
                    assert_eq!(ki.event.text, None);
                    assert_eq!(ki.event.location, KeyLocation::Left);
                    #[cfg(have_mod_supplement)]
                    {
                        assert_eq!(ki.event.mod_supplement.key_without_modifiers, WKey::Shift);
                        assert_eq!(
                            ki.event.mod_supplement.text_with_all_modifiers.as_deref(),
                            None
                        );
                    }
                } else {
                    assert_eq!(ki.event.physical_key, KeyCode::KeyL);
                    assert_eq!(ki.event.logical_key, WKey::Character("L"));
                    assert_eq!(ki.event.text, Some("L"));
                    assert_eq!(ki.event.location, KeyLocation::Standard);
                    #[cfg(have_mod_supplement)]
                    {
                        assert_eq!(
                            ki.event.mod_supplement.key_without_modifiers,
                            WKey::Character("l")
                        );
                        assert_eq!(
                            ki.event.mod_supplement.text_with_all_modifiers.as_deref(),
                            Some("\x0c")
                        );
                    }
                }
                if matches!(i, 0 | 2 | 4) {
                    assert_eq!(ki.event.state, ElementState::Pressed);
                } else {
                    assert_eq!(ki.event.state, ElementState::Released);
                }
            }
        }
    }

    log::info!("Switching to Azerty layout.");
    seat.set_layout(Layout::Azerty);

    {
        log::info!("Testing Q");
        // Q Press
        // Q Release
        {
            kb.press(KeyQ);
        }
        // 0: Q pressed
        // 1: Q released
        for i in 0..2 {
            let (_, ki) = el.window_keyboard_input().await;
            assert_eq!(ki.event.physical_key, KeyCode::KeyQ);
            assert_eq!(ki.event.logical_key, WKey::Character("a"));
            assert_eq!(ki.event.text, Some("a"));
            assert_eq!(ki.event.location, KeyLocation::Standard);
            #[cfg(have_mod_supplement)]
            {
                assert_eq!(
                    ki.event.mod_supplement.key_without_modifiers,
                    WKey::Character("a")
                );
                assert_eq!(
                    ki.event.mod_supplement.text_with_all_modifiers.as_deref(),
                    Some("a")
                );
            }
            if i == 0 {
                assert_eq!(ki.event.state, ElementState::Pressed);
            } else {
                assert_eq!(ki.event.state, ElementState::Released);
            }
        }
    }
}
