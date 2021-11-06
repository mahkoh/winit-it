use crate::event::{Event, WindowEvent, WindowEventExt, WindowKeyboardInput};
use std::any::Any;
use std::future::Future;
use std::pin::Pin;
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowBuilder};

mod x11;

pub fn backends() -> Vec<Box<dyn Backend>> {
    vec![x11::backend()]
}

pub trait Backend {
    fn instantiate(&self) -> Box<dyn Instance>;
    fn is_mt_safe(&self) -> bool;
    fn name(&self) -> &str;
}

pub trait Instance {
    fn default_seat(&self) -> Box<dyn Seat>;
    fn create_event_loop(&self) -> Box<dyn EventLoop>;
    fn set_background_color(&self, window: &Window, r: u8, g: u8, b: u8);
    fn take_screenshot(&self);
    fn mapped<'b>(&'b self, window: &Window) -> Pin<Box<dyn Future<Output=()> + 'b>>;
}

pub trait EventLoop {
    fn event<'a>(&'a self) -> Pin<Box<dyn Future<Output=Event<Box<dyn Any>>> + 'a>>;
    fn create_window(&self, builder: WindowBuilder) -> Window;
}

impl dyn EventLoop {
    pub async fn window_event(&self) -> WindowEventExt {
        loop {
            if let Event::WindowEvent(we) = self.event().await {
                return we;
            }
        }
    }

    pub async fn window_keyboard_input(&self) -> (WindowEventExt, WindowKeyboardInput) {
        log::debug!("Awaiting keyboard input");
        loop {
            let we = self.window_event().await;
            if let WindowEvent::KeyboardInput(ki) = &we.event {
                log::debug!("Got keyboard input {:?}", ki);
                let ki = ki.clone();
                return (we, ki);
            }
        }
    }

    pub async fn window_modifiers(&self) -> (WindowEventExt, ModifiersState) {
        log::debug!("Awaiting window modifiers");
        loop {
            let we = self.window_event().await;
            if let WindowEvent::ModifiersChanged(ki) = &we.event {
                log::debug!("Got window modifiers {:?}", ki);
                let ki = ki.clone();
                return (we, ki);
            }
        }
    }
}

pub trait Seat {
    fn add_keyboard(&self) -> Box<dyn Keyboard>;
    fn add_mouse(&self) -> Box<dyn Mouse>;
    fn focus(&self, window: &Window);
}

pub trait Keyboard {
    fn press(&self, key: Key) -> Box<dyn PressedKey>;
}

pub trait Mouse {}

pub trait PressedKey {}

#[allow(dead_code)]
#[derive(Copy, Clone, Hash, Eq, PartialEq)]
pub enum Key {
    Key0,
    Key1,
    Key102Nd,
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
    KeyDollar,
    KeyDot,
    KeyDown,
    KeyE,
    KeyEnd,
    KeyEnter,
    KeyEqual,
    KeyEsc,
    KeyEuro,
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
    KeyKpcomma,
    KeyKpdot,
    KeyKpenter,
    KeyKpequal,
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
