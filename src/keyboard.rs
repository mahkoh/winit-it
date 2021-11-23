/// Keys on the 104 key windows keyboard
#[allow(dead_code)]
#[derive(Copy, Clone, Hash, Eq, PartialEq)]
pub enum Key {
    Key0,
    Key1,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
    Key8,
    Key9,
    KeyA,
    KeyApostrophe,
    KeyB,
    KeyBackslash,
    KeyBackspace,
    KeyC,
    KeyCapslock,
    KeyComma,
    KeyD,
    KeyDelete,
    KeyDot,
    KeyDown,
    KeyE,
    KeyEnd,
    KeyEnter,
    KeyEqual,
    KeyEsc,
    KeyF,
    KeyF1,
    KeyF10,
    KeyF11,
    KeyF12,
    KeyF2,
    KeyF3,
    KeyF4,
    KeyF5,
    KeyF6,
    KeyF7,
    KeyF8,
    KeyF9,
    KeyG,
    KeyGrave,
    KeyH,
    KeyHome,
    KeyI,
    KeyInsert,
    KeyJ,
    KeyK,
    KeyKp0,
    KeyKp1,
    KeyKp2,
    KeyKp3,
    KeyKp4,
    KeyKp5,
    KeyKp6,
    KeyKp7,
    KeyKp8,
    KeyKp9,
    KeyKpasterisk,
    KeyKpdot,
    KeyKpenter,
    KeyKpminus,
    KeyKpplus,
    KeyKpslash,
    KeyL,
    KeyLeft,
    KeyLeftalt,
    KeyLeftbrace,
    KeyLeftctrl,
    KeyLeftmeta,
    KeyLeftshift,
    KeyM,
    KeyMenu,
    KeyMinus,
    KeyN,
    KeyNumlock,
    KeyO,
    KeyP,
    KeyPagedown,
    KeyPageup,
    KeyPause,
    KeyQ,
    KeyR,
    KeyRight,
    KeyRightalt,
    KeyRightbrace,
    KeyRightctrl,
    KeyRightmeta,
    KeyRightshift,
    KeyS,
    KeyScrolllock,
    KeySemicolon,
    KeySlash,
    KeySpace,
    KeySysRq,
    KeyT,
    KeyTab,
    KeyU,
    KeyUp,
    KeyV,
    KeyW,
    KeyX,
    KeyY,
    KeyZ,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Layout {
    Qwerty,
    Azerty,
    /// Qwerty with Left/Right shift swapped and Esc/Capslock swapped.
    QwertySwapped,
}
