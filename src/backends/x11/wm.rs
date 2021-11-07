use super::{WindowData, XInstanceData};
use std::future::Future;
use std::sync::Arc;
use tokio::io::unix::AsyncFd;
use tokio::io::Interest;
use xcb_dl::ffi;
use xcb_dl_util::error::XcbErrorType;

pub(super) fn run(instance: Arc<XInstanceData>) -> impl Future<Output = ()> {
    unsafe {
        let events = ffi::XCB_EVENT_MASK_SUBSTRUCTURE_REDIRECT
            | ffi::XCB_EVENT_MASK_SUBSTRUCTURE_NOTIFY
            | ffi::XCB_EVENT_MASK_PROPERTY_CHANGE;
        let cookie = instance.backend.xcb.xcb_change_window_attributes_checked(
            instance.c,
            instance.screen.root,
            ffi::XCB_CW_EVENT_MASK,
            &events as *const ffi::xcb_event_mask_t as _,
        );
        if let Err(e) = instance.errors.check_cookie(&instance.backend.xcb, cookie) {
            panic!("Could not select wm events: {}", e);
        }
        let wm = Wm { instance };
        wm.run()
    }
}

struct Wm {
    instance: Arc<XInstanceData>,
}

impl Wm {
    async fn run(mut self) {
        let fd = AsyncFd::with_interest(self.instance.fd, Interest::READABLE).unwrap();
        loop {
            fd.readable().await.unwrap().clear_ready();
            self.handle_events();
        }
    }

    fn handle_events(&mut self) {
        loop {
            unsafe {
                let event = self
                    .instance
                    .backend
                    .xcb
                    .xcb_poll_for_event(self.instance.c);
                let event = match self
                    .instance
                    .errors
                    .check_val(&self.instance.backend.xcb, event)
                {
                    Ok(e) => e,
                    Err(e) => {
                        if matches!(e.ty, XcbErrorType::MissingReply) {
                            return;
                        }
                        panic!("The connection is in error: {}", e);
                    }
                };
                self.handle_event(&event);
            }
        }
    }

    fn handle_event(&mut self, event: &ffi::xcb_generic_event_t) {
        match event.response_type {
            ffi::XCB_CREATE_NOTIFY => self.handle_create_notify(event),
            ffi::XCB_MAP_REQUEST => self.handle_map_request(event),
            ffi::XCB_CONFIGURE_REQUEST => self.handle_configure_request(event),
            ffi::XCB_PROPERTY_NOTIFY => self.handle_property_notify(event),
            ffi::XCB_MAP_NOTIFY => self.handle_map_notify(event),
            ffi::XCB_UNMAP_NOTIFY => self.handle_unmap_notify(event),
            ffi::XCB_DESTROY_NOTIFY => self.handle_destroy_notify(event),
            ffi::XCB_MAPPING_NOTIFY => {}
            _ => {
                log::warn!("Received unexpected event: {:?}", event);
            }
        }
    }

    fn handle_property_notify(&mut self, event: &ffi::xcb_generic_event_t) {
        let event = unsafe { &*(event as *const _ as *const ffi::xcb_property_notify_event_t) };
        log::info!("{:?}", event);
    }

    fn handle_configure_request(&mut self, event: &ffi::xcb_generic_event_t) {
        let event = unsafe { &*(event as *const _ as *const ffi::xcb_configure_request_event_t) };
        unsafe {
            let list = ffi::xcb_configure_window_value_list_t {
                x: event.x as _,
                y: event.y as _,
                width: event.width as _,
                height: event.height as _,
                border_width: event.border_width as _,
                sibling: event.sibling as _,
                stack_mode: event.stack_mode as _,
            };
            let cookie = self.instance.backend.xcb.xcb_configure_window_aux_checked(
                self.instance.c,
                event.window,
                event.value_mask,
                &list,
            );
            let error = self
                .instance
                .errors
                .check_cookie(&self.instance.backend.xcb, cookie);
            if let Err(e) = error {
                log::error!("Could not configure window: {}", e);
            }
        }
    }

    fn handle_map_request(&mut self, event: &ffi::xcb_generic_event_t) {
        let event = unsafe { &*(event as *const _ as *const ffi::xcb_map_request_event_t) };
        unsafe {
            let cookie = self
                .instance
                .backend
                .xcb
                .xcb_map_window_checked(self.instance.c, event.window);
            let error = self
                .instance
                .errors
                .check_cookie(&self.instance.backend.xcb, cookie);
            if let Err(e) = error {
                log::error!("Could not map window: {}", e);
            }
        }
    }

    fn handle_create_notify(&mut self, event: &ffi::xcb_generic_event_t) {
        let event = unsafe { &*(event as *const _ as *const ffi::xcb_create_notify_event_t) };
        log::info!("Window created: {}", event.window);
        let mut data = self.instance.wm_data.lock();
        data.windows.push(WindowData {
            id: event.window,
            mapped: false,
        });
        data.changed();
    }

    fn handle_destroy_notify(&mut self, event: &ffi::xcb_generic_event_t) {
        let event = unsafe { &*(event as *const _ as *const ffi::xcb_destroy_notify_event_t) };
        log::info!("Window destroyed: {}", event.window);
        let mut data = self.instance.wm_data.lock();
        data.windows.retain(|w| w.id != event.window);
        data.changed();
    }

    fn handle_map_notify(&mut self, event: &ffi::xcb_generic_event_t) {
        let event = unsafe { &*(event as *const _ as *const ffi::xcb_map_notify_event_t) };
        log::info!("Window mapped: {}", event.window);
        let mut data = self.instance.wm_data.lock();
        if let Some(w) = data.windows.iter_mut().find(|w| w.id == event.window) {
            w.mapped = true;
            data.changed();
        }
    }

    fn handle_unmap_notify(&mut self, event: &ffi::xcb_generic_event_t) {
        let event = unsafe { &*(event as *const _ as *const ffi::xcb_unmap_notify_event_t) };
        log::info!("Window unmapped: {}", event.window);
        let mut data = self.instance.wm_data.lock();
        if let Some(w) = data.windows.iter_mut().find(|w| w.id == event.window) {
            w.mapped = false;
            data.changed();
        }
    }
}
