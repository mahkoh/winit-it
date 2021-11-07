use crate::event::{Event, WindowEvent, WindowEventExt, WindowKeyboardInput};
use crate::keyboard::Key;
use std::future::Future;
use std::pin::Pin;
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowBuilder};

bitflags::bitflags! {
    pub struct BackendFlags: u32 {
        const MT_SAFE = 1 << 0;
    }
}

pub trait Backend: Sync {
    fn instantiate(&self) -> Box<dyn Instance>;
    fn flags(&self) -> BackendFlags;
    fn name(&self) -> &str;
}

pub trait Instance {
    fn backend(&self) -> &dyn Backend;
    fn default_seat(&self) -> Box<dyn Seat>;
    fn create_event_loop(&self) -> Box<dyn EventLoop>;
    fn set_background_color(&self, window: &Window, r: u8, g: u8, b: u8);
    fn take_screenshot(&self);
    fn mapped<'b>(&'b self, window: &Window) -> Pin<Box<dyn Future<Output = ()> + 'b>>;
}

pub trait EventLoop {
    fn event<'a>(&'a self) -> Pin<Box<dyn Future<Output = Event> + 'a>>;
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
