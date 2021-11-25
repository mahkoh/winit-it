use crate::event::{
    DeviceEvent, DeviceEventExt, Event, UserEvent, WindowEvent, WindowEventExt, WindowKeyboardInput,
};
use crate::keyboard::{Key, Layout};
use std::any::Any;
use std::fmt::Display;
use std::future::Future;
use std::pin::Pin;
use winit::dpi::{PhysicalPosition, PhysicalSize, Position, Size};
use winit::event::{DeviceId, RawKeyEvent};
use winit::event_loop::EventLoop as WEventLoop;
use winit::keyboard::ModifiersState;
use winit::monitor::MonitorHandle;
use winit::window::{Icon, UserAttentionType, Window as WWindow, WindowBuilder, WindowId};

bitflags::bitflags! {
    pub struct BackendFlags: u32 {
        const MT_SAFE = 1 << 0;
        const WINIT_SET_ALWAYS_ON_TOP = 1 << 1;
        const WINIT_SET_DECORATIONS = 1 << 2;
        const WINIT_SET_INNER_SIZE = 1 << 3;
        const WINIT_SET_OUTER_POSITION = 1 << 4;
        const WINIT_SET_TITLE = 1 << 5;
        const WINIT_SET_MAXIMIZED = 1 << 6;
        const WINIT_SET_SIZE_BOUNDS = 1 << 7;
        const WINIT_SET_ATTENTION = 1 << 8;
        const X11 = 1 << 9;
        const WINIT_SET_MINIMIZED = 1 << 10;
        const WINIT_SET_VISIBLE = 1 << 11;
        const WINIT_SET_RESIZABLE = 1 << 12;
        const WINIT_TRANSPARENCY = 1 << 13;
        const WINIT_SET_ICON = 1 << 14;
        const SET_OUTER_POSITION = 1 << 15;
        const SET_INNER_SIZE = 1 << 16;
        const DEVICE_ADDED = 1 << 17;
        const DEVICE_REMOVED = 1 << 18;
        const CREATE_SEAT = 1 << 19;
        const SECOND_MONITOR = 1 << 20;
        const MONITOR_NAMES = 1 << 21;
        const SINGLE_THREADED = 1 << 22;
    }
}

pub const NON_REQUIREMENT_FLAGS: BackendFlags = BackendFlags::SINGLE_THREADED;

pub trait Backend: Sync {
    fn instantiate(&self) -> Box<dyn Instance>;
    fn flags(&self) -> BackendFlags;
    fn name(&self) -> &str;
}

pub trait Instance {
    fn backend(&self) -> &dyn Backend;
    fn default_seat(&self) -> Box<dyn Seat>;
    fn create_event_loop(&self) -> Box<dyn EventLoop>;
    fn take_screenshot(&self);
    fn before_poll(&self);
    fn create_seat(&self) -> Box<dyn Seat> {
        unimplemented!();
    }
    fn enable_second_monitor(&self, enabled: bool) {
        let _ = enabled;
        unimplemented!();
    }
}

pub trait EventLoop {
    fn event<'a>(&'a self) -> Pin<Box<dyn Future<Output = Event> + 'a>>;
    fn changed<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + 'a>>;
    fn create_window(&self, builder: WindowBuilder) -> Box<dyn Window>;
    fn with_winit<'a>(&self, f: Box<dyn FnOnce(&mut WEventLoop<UserEvent>) + 'a>);
}

impl dyn EventLoop {
    pub fn send_event(&self, event: UserEvent) {
        self.with_winit(Box::new(|el| el.create_proxy().send_event(event).unwrap()));
    }

    pub fn available_monitors(&self) -> Vec<MonitorHandle> {
        let mut res = vec![];
        self.with_winit(Box::new(|el| res.extend(el.available_monitors())));
        res
    }

    pub fn primary_monitor(&self) -> Option<MonitorHandle> {
        let mut res = None;
        self.with_winit(Box::new(|el| res = el.primary_monitor()));
        res
    }

    pub async fn num_available_monitors(&self, n: usize) {
        log::info!("Waiting for number of available monitors to become {}", n);
        loop {
            if self.available_monitors().len() == n {
                return;
            }
            self.changed().await;
        }
    }

    pub async fn user_event(&self) -> UserEvent {
        loop {
            if let Event::UserEvent(ue) = self.event().await {
                return ue;
            }
        }
    }

    pub async fn window_event(&self) -> WindowEventExt {
        loop {
            if let Event::WindowEvent(we) = self.event().await {
                return we;
            }
        }
    }

    pub async fn device_event(&self) -> DeviceEventExt {
        loop {
            if let Event::DeviceEvent(we) = self.event().await {
                return we;
            }
        }
    }

    pub async fn device_added_event(&self) -> DeviceEventExt {
        log::info!("Waiting for device added event");
        loop {
            let de = self.device_event().await;
            if de.event == DeviceEvent::Added {
                return de;
            }
        }
    }

    pub async fn device_removed_event(&self) -> DeviceEventExt {
        log::info!("Waiting for device removed event");
        loop {
            let de = self.device_event().await;
            if de.event == DeviceEvent::Removed {
                return de;
            }
        }
    }

    pub async fn device_key_event(&self) -> (DeviceEventExt, RawKeyEvent) {
        log::info!("Waiting for device key event");
        loop {
            let de = self.device_event().await;
            if let DeviceEvent::Key(e) = de.event {
                log::debug!("Got key event {:?}", e);
                return (de, e);
            }
        }
    }

    pub async fn window_destroyed_event(&self) -> WindowEventExt {
        log::debug!("Awaiting window destroyed");
        loop {
            let we = self.window_event().await;
            if let WindowEvent::Destroyed = &we.event {
                log::debug!("Got window destroyed");
                return we.clone();
            };
        }
    }

    pub async fn window_focus_event(&self) -> (WindowEventExt, bool) {
        log::debug!("Awaiting window focus");
        loop {
            let we = self.window_event().await;
            if let WindowEvent::Focused(v) = &we.event {
                log::debug!("Got window focus {}", v);
                return (we.clone(), *v);
            };
        }
    }

    pub async fn window_move_event(&self) -> (WindowEventExt, PhysicalPosition<i32>) {
        log::debug!("Awaiting window move");
        loop {
            let we = self.window_event().await;
            if let WindowEvent::Moved(pos) = &we.event {
                log::debug!("Got window move");
                return (we.clone(), pos.clone());
            };
        }
    }

    pub async fn window_resize_event(&self) -> (WindowEventExt, PhysicalSize<u32>) {
        log::debug!("Awaiting window resize");
        loop {
            let we = self.window_event().await;
            if let WindowEvent::Resized(pos) = &we.event {
                log::debug!("Got window resize");
                return (we.clone(), pos.clone());
            };
        }
    }

    pub async fn window_close_requested(&self) -> WindowEventExt {
        log::debug!("Awaiting window delete");
        loop {
            let we = self.window_event().await;
            if let WindowEvent::CloseRequested = &we.event {
                log::debug!("Got close requested");
                return we;
            };
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

#[derive(Clone, Eq, PartialEq)]
pub struct BackendIcon {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl Into<Icon> for BackendIcon {
    fn into(self) -> Icon {
        Icon::from_rgba(self.rgba, self.width, self.height).unwrap()
    }
}

pub trait WindowProperties {
    fn mapped(&self) -> bool;
    fn always_on_top(&self) -> bool;
    fn decorations(&self) -> bool;
    fn x(&self) -> i32;
    fn y(&self) -> i32;
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn min_size(&self) -> Option<(u32, u32)>;
    fn max_size(&self) -> Option<(u32, u32)>;
    fn title(&self) -> Option<String>;
    fn maximized(&self) -> Option<bool>;
    fn minimized(&self) -> Option<bool>;
    fn resizable(&self) -> Option<bool>;
    fn icon(&self) -> Option<BackendIcon>;
    fn attention(&self) -> bool;
    fn supports_transparency(&self) -> bool;
    fn class(&self) -> Option<String> {
        unimplemented!();
    }
    fn instance(&self) -> Option<String> {
        unimplemented!();
    }
}

pub trait Window {
    fn id(&self) -> &dyn Display;
    fn backend(&self) -> &dyn Backend;
    fn event_loop(&self) -> &dyn EventLoop;
    fn winit(&self) -> &WWindow;
    fn properties_changed<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + 'a>>;
    fn properties(&self) -> &dyn WindowProperties;
    fn set_background_color(&self, r: u8, g: u8, b: u8);
    fn any(&self) -> &dyn Any;
    fn delete(&self);
    /// left, right, top, bottom
    fn frame_extents(&self) -> (u32, u32, u32, u32);
    fn set_outer_position(&self, x: i32, y: i32) {
        let _ = x;
        let _ = y;
        unimplemented!();
    }
    fn set_inner_size(&self, width: u32, height: u32) {
        let _ = width;
        let _ = height;
        unimplemented!();
    }
    fn ping<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + 'a>> {
        unimplemented!();
    }
}

pub const NONE_SIZE: Option<Size> = None;

impl dyn Window {
    pub fn reset_dead_keys(&self) {
        log::info!("Resetting dead keys");
        self.winit().reset_dead_keys();
    }

    pub fn winit_set_decorations(&self, decorations: bool) {
        log::info!(
            "Setting decorations of window {} to {}",
            self.id(),
            decorations
        );
        self.winit().set_decorations(decorations);
    }

    pub fn winit_set_visible(&self, visible: bool) {
        log::info!("Setting visibility of window {} to {}", self.id(), visible);
        self.winit().set_visible(visible);
    }

    pub fn winit_set_always_on_top(&self, always_on_top: bool) {
        log::info!(
            "Setting always-on-top of window {} to {}",
            self.id(),
            always_on_top
        );
        self.winit().set_always_on_top(always_on_top);
    }

    pub fn winit_set_inner_size<S: Into<Size>>(&self, size: S) {
        let size = size.into();
        log::info!("Setting inner size of window {} to {:?}", self.id(), size);
        self.winit().set_inner_size(size);
    }

    pub fn winit_set_title(&self, title: &str) {
        log::info!("Setting title of window {} to {}", self.id(), title,);
        self.winit().set_title(title);
    }

    pub fn winit_set_outer_position<S: Into<Position>>(&self, size: S) {
        let size = size.into();
        log::info!(
            "Setting outer position of window {} to {:?}",
            self.id(),
            size,
        );
        self.winit().set_outer_position(size);
    }

    pub fn winit_id(&self) -> WindowId {
        self.winit().id()
    }

    pub fn winit_set_minimized(&self, minimized: bool) {
        log::info!(
            "Setting minimized of window {} to {:?}",
            self.id(),
            minimized,
        );
        self.winit().set_minimized(minimized);
    }

    pub fn winit_set_maximized(&self, maximized: bool) {
        log::info!(
            "Setting maximized of window {} to {:?}",
            self.id(),
            maximized,
        );
        self.winit().set_maximized(maximized);
    }

    pub fn winit_set_min_size<S: Into<Size>>(&self, size: Option<S>) {
        let size = size.map(|s| s.into());
        log::info!("Setting minimum size of window {} to {:?}", self.id(), size,);
        self.winit().set_min_inner_size(size);
    }

    pub fn winit_set_max_size<S: Into<Size>>(&self, size: Option<S>) {
        let size = size.map(|s| s.into());
        log::info!("Setting maximum size of window {} to {:?}", self.id(), size,);
        self.winit().set_max_inner_size(size);
    }

    pub fn winit_set_attention(&self, urgency: Option<UserAttentionType>) {
        log::info!("Setting urgency of window {} to {:?}", self.id(), urgency,);
        self.winit().request_user_attention(urgency);
    }

    pub fn winit_set_resizable(&self, resizable: bool) {
        log::info!(
            "Setting resizable of window {} to {:?}",
            self.id(),
            resizable
        );
        self.winit().set_resizable(resizable);
    }

    pub fn winit_set_window_icon(&self, icon: Option<Icon>) {
        log::info!(
            "Setting window icon of window {} to {}",
            self.id(),
            icon.is_some()
        );
        self.winit().set_window_icon(icon);
    }

    pub async fn mapped(&self, mapped: bool) {
        log::info!(
            "Waiting for window {} to become mapped {}",
            self.id(),
            mapped
        );
        self.await_property(|p| p.mapped() == mapped).await
    }

    pub async fn always_on_top(&self, always_on_top: bool) {
        log::info!(
            "Waiting for window {} to become always-on-top {}",
            self.id(),
            always_on_top
        );
        self.await_property(|p| p.always_on_top() == always_on_top)
            .await
    }

    pub async fn decorations(&self, decorations: bool) {
        log::info!(
            "Waiting for window {} to become decorations {}",
            self.id(),
            decorations,
        );
        self.await_property(|p| p.decorations() == decorations)
            .await
    }

    pub async fn title(&self, title: &str) {
        log::info!("Waiting for window {} to become title {}", self.id(), title,);
        self.await_property(|p| p.title().as_deref() == Some(title))
            .await
    }

    pub async fn inner_size(&self, width: u32, height: u32) {
        log::info!(
            "Waiting for window {} to become inner size {}x{}",
            self.id(),
            width,
            height,
        );
        self.await_property(|p| p.width() == width && p.height() == height)
            .await
    }

    pub async fn icon(&self, icon: Option<&BackendIcon>) {
        log::info!(
            "Waiting for window {} to become icon {}",
            self.id(),
            icon.is_some()
        );
        self.await_property(|p| p.icon().as_ref() == icon).await;
    }

    pub fn inner_offset(&self) -> (i32, i32) {
        let (left, _, top, _) = self.frame_extents();
        (left as i32, top as i32)
    }

    pub async fn outer_position(&self, x: i32, y: i32) {
        log::info!(
            "Waiting for window {} to become outer position {}x{}",
            self.id(),
            x,
            y,
        );
        self.await_property(|p| p.x() == x && p.y() == y).await
    }

    pub async fn maximized(&self, maximized: bool) {
        log::info!(
            "Waiting for window {} to become maximized {}",
            self.id(),
            maximized
        );
        self.await_property(|p| p.maximized() == Some(maximized))
            .await
    }

    pub async fn minimized(&self, minimized: bool) {
        log::info!(
            "Waiting for window {} to become minimized {}",
            self.id(),
            minimized
        );
        self.await_property(|p| p.minimized() == Some(minimized))
            .await
    }

    pub async fn min_size(&self, size: Option<(u32, u32)>) {
        log::info!(
            "Waiting for window {} to become min size {:?}",
            self.id(),
            size
        );
        self.await_property(|p| p.min_size() == size).await
    }

    pub async fn max_size(&self, size: Option<(u32, u32)>) {
        log::info!(
            "Waiting for window {} to become max size {:?}",
            self.id(),
            size
        );
        self.await_property(|p| p.max_size() == size).await
    }

    pub async fn attention(&self, attention: bool) {
        log::info!(
            "Waiting for window {} to become attention {:?}",
            self.id(),
            attention,
        );
        self.await_property(|p| p.attention() == attention).await
    }

    pub async fn class(&self, class: &str) {
        log::info!(
            "Waiting for window {} to become class {:?}",
            self.id(),
            class,
        );
        self.await_property(|p| p.class().as_deref() == Some(class))
            .await
    }

    pub async fn instance(&self, instance: &str) {
        log::info!(
            "Waiting for window {} to become instance {:?}",
            self.id(),
            instance,
        );
        self.await_property(|p| p.instance().as_deref() == Some(instance))
            .await
    }

    pub async fn resizable(&self, resizable: bool) {
        log::info!(
            "Waiting for window {} to become resizable {:?}",
            self.id(),
            resizable,
        );
        self.await_property(|p| p.resizable() == Some(resizable))
            .await
    }

    pub async fn winit_inner_size(&self, width: u32, height: u32) {
        log::info!(
            "Waiting for window {} to become winit inner size {}x{}",
            self.id(),
            width,
            height,
        );
        self.await_winit(|w| {
            let is = w.inner_size();
            let os = w.outer_size();
            log::trace!("Inner size: {:?}, outer size: {:?}", is, os);
            let (left, right, top, bottom) = self.frame_extents();
            is.width == width
                && is.height == height
                && os.width == width + left + right
                && os.height == height + top + bottom
        })
        .await
    }

    pub async fn winit_outer_position(&self, x: i32, y: i32) {
        log::info!(
            "Waiting for window {} to become winit outer position {}x{}",
            self.id(),
            x,
            y,
        );
        self.await_winit(|w| {
            let o_pos = w.outer_position().unwrap();
            let i_pos = w.inner_position().unwrap();
            let (xoff, yoff) = self.inner_offset();
            o_pos.x == x && o_pos.y == y && i_pos.x == x + xoff && i_pos.y == y + yoff
        })
        .await
    }

    pub async fn await_winit<F: FnMut(&WWindow) -> bool>(&self, mut f: F) {
        loop {
            if f(self.winit()) {
                return;
            }
            self.event_loop().changed().await;
        }
    }

    pub async fn await_property<F: FnMut(&dyn WindowProperties) -> bool>(&self, mut f: F) {
        loop {
            if f(self.properties()) {
                return;
            }
            self.properties_changed().await;
        }
    }
}

pub trait Seat {
    fn add_keyboard(&self) -> Box<dyn Keyboard>;
    fn add_mouse(&self) -> Box<dyn Mouse>;
    fn focus(&self, window: &dyn Window);
    fn un_focus(&self);
    fn set_layout(&self, layout: Layout);
}

pub trait BackendDeviceId {
    fn is(&self, device: DeviceId) -> bool;
}

pub trait Device {
    fn id(&self) -> Box<dyn BackendDeviceId>;
}

pub trait Keyboard: Device {
    fn press(&self, key: Key) -> Box<dyn PressedKey>;
}

pub trait Mouse: Device {}

pub trait PressedKey {}
