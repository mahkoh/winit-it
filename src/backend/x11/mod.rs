use crate::backend::x11::MessageType::{
    MT_CREATE_KEYBOARD, MT_CREATE_KEYBOARD_REPLY, MT_KEY_PRESS, MT_KEY_RELEASE,
};
use crate::backend::{Backend, EventLoop, Instance, Key, Keyboard, Mouse, PressedKey, Seat};
use crate::event::{map_event, Event};
use parking_lot::Mutex;
use std::any::Any;
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::process::Command;
use std::ptr;
use std::sync::{Arc, Weak};
use std::task::{Context, Poll, Waker};
use tokio::io::unix::AsyncFd;
use tokio::io::Interest;
use tokio::task::JoinHandle;
use uapi::c::{AF_UNIX, O_CLOEXEC, SOCK_CLOEXEC, SOCK_SEQPACKET};
use uapi::{pipe2, socketpair, IntoUstr, OwnedFd, Pod, UapiReadExt, UstrPtr};
use winit::event_loop::{ControlFlow, EventLoop as WEventLoop};
use winit::platform::run_return::EventLoopExtRunReturn;
use winit::platform::unix::{EventLoopExtUnix, EventLoopWindowTargetExtUnix, WindowExtUnix};
use winit::window::{Window, WindowBuilder};
use xcb_dl::{ffi, Xcb, XcbXinput};
use xcb_dl_util::error::XcbErrorParser;

mod evdev;
mod wm;

static ENV_LOCK: Mutex<()> = parking_lot::const_mutex(());

const DEFAULT_X_PATH: &str = "/usr/lib/Xorg";

pub fn backend() -> Box<dyn Backend> {
    let x_path = match std::env::var("X_PATH") {
        Ok(p) => p,
        _ => DEFAULT_X_PATH.to_string(),
    };
    let default_module_path = Command::new(&x_path)
        .arg("-showDefaultModulePath")
        .output()
        .unwrap()
        .stderr;
    unsafe {
        Box::new(Arc::new(XBackend {
            x_path,
            default_module_path: String::from_utf8(default_module_path)
                .unwrap()
                .trim()
                .to_string(),
            xcb: Xcb::load_loose().unwrap(),
            xinput: XcbXinput::load_loose().unwrap(),
        }))
    }
}

struct XBackend {
    x_path: String,
    default_module_path: String,
    xcb: Xcb,
    xinput: XcbXinput,
}

struct XInstanceData {
    backend: Arc<XBackend>,
    screen: ffi::xcb_screen_t,
    xserver_pid: libc::pid_t,
    sock: OwnedFd,
    display: u32,
    c: *mut ffi::xcb_connection_t,
    fd: libc::c_int,
    errors: XcbErrorParser,
    core_p: ffi::xcb_input_device_id_t,
    core_kb: ffi::xcb_input_device_id_t,
    wm_data: Mutex<WmData>,
}

struct XInstance {
    data: Arc<XInstanceData>,
    wm: Option<JoinHandle<()>>,
}

unsafe impl Send for XInstance {}
unsafe impl Sync for XInstance {}

impl Backend for Arc<XBackend> {
    fn instantiate(&self) -> Box<dyn Instance> {
        let (psock, chsock) = socketpair(AF_UNIX, SOCK_SEQPACKET | SOCK_CLOEXEC, 0).unwrap();
        let (mut ppipe, chpipe) = pipe2(O_CLOEXEC).unwrap();
        let tmpdir = crate::test::with_test_data(|td| td.test_dir.join("x11_data"));
        std::fs::create_dir_all(&tmpdir).unwrap();
        let config_file = tmpdir.join("config.conf");
        let log_file = tmpdir.join("log");
        let stderr_file = tmpdir.join("stderr").into_ustr();
        let config_dir = tmpdir.join("conf");
        let module_path = format!(
            "{},{}/x11-module/install",
            self.default_module_path,
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::write(&config_file, CONFIG).unwrap();
        let env = {
            let mut env = UstrPtr::new();
            for name in ["HOME", "PATH"] {
                env.push(format!("{}={}", name, std::env::var(name).unwrap()));
            }
            env.push(format!("WINIT_IT_SOCKET={}", chsock.raw()));
            env
        };
        let args = {
            let mut args = UstrPtr::new();
            args.push(&*self.x_path);
            args.push("-config");
            args.push(&*config_file);
            args.push("-configdir");
            args.push(&*config_dir);
            args.push("-modulepath");
            args.push(&*module_path);
            args.push("-seat");
            args.push("winit-seat");
            args.push("-logfile");
            args.push(&*log_file);
            args.push("-noreset");
            args.push("-displayfd");
            args.push(chpipe.to_string().into_ustr().to_owned());
            args
        };
        log::trace!("args: {:?}", args);
        log::trace!("env: {:?}", env);
        let chpid = unsafe { uapi::fork().unwrap() };
        if chpid == 0 {
            let null = uapi::open("/dev/null\0", libc::O_RDWR, 0).unwrap();
            let stderr = uapi::open(&*stderr_file, libc::O_CREAT | libc::O_WRONLY, 0o666).unwrap();
            uapi::dup2(null.raw(), 0).unwrap();
            uapi::dup2(null.raw(), 1).unwrap();
            uapi::dup2(stderr.raw(), 2).unwrap();
            uapi::fcntl_setfd(chsock.raw(), 0).unwrap();
            uapi::fcntl_setfd(chpipe.raw(), 0).unwrap();
            drop(null);
            drop(stderr);
            unsafe {
                uapi::map_err!(libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL)).unwrap();
            }
            uapi::execvpe(&*self.x_path, &args, &env).unwrap();
        }
        drop(chpipe);
        let display = ppipe
            .read_to_new_ustring()
            .unwrap()
            .into_string()
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        log::trace!("display: {}", display);

        let (c, parser) = unsafe {
            let display_str = uapi::format_ustr!(":{}", display);
            let c = self.xcb.xcb_connect(display_str.as_ptr(), ptr::null_mut());
            let parser = XcbErrorParser::new(&self.xcb, c);
            parser.check_connection(&self.xcb).unwrap();
            (c, parser)
        };

        let (core_p, core_kb) = unsafe {
            let cookie = self.xinput.xcb_input_xi_query_version(c, 2, 0);
            let mut err = ptr::null_mut();
            let reply = self
                .xinput
                .xcb_input_xi_query_version_reply(c, cookie, &mut err);
            let _reply = parser.check(&self.xcb, reply, err).unwrap();
            let cookie = self
                .xinput
                .xcb_input_xi_query_device(c, ffi::XCB_INPUT_DEVICE_ALL_MASTER as _);
            let reply = self
                .xinput
                .xcb_input_xi_query_device_reply(c, cookie, &mut err);
            let reply = parser.check(&self.xcb, reply, err).unwrap();
            let mut iter = self
                .xinput
                .xcb_input_xi_query_device_infos_iterator(&*reply);
            let mut core = None;
            while iter.rem > 0 {
                let info = &*iter.data;
                if info.type_ == ffi::XCB_INPUT_DEVICE_TYPE_MASTER_POINTER as _ {
                    assert!(core.is_none());
                    core = Some((info.deviceid, info.attachment));
                }
                self.xinput.xcb_input_xi_device_info_next(&mut iter);
            }
            core.unwrap()
        };

        let screen = unsafe {
            *self
                .xcb
                .xcb_setup_roots_iterator(self.xcb.xcb_get_setup(c))
                .data
        };

        let instance = Arc::new(XInstanceData {
            backend: self.clone(),
            screen,
            xserver_pid: chpid,
            sock: psock,
            display,
            c,
            fd: unsafe { self.xcb.xcb_get_file_descriptor(c) },
            errors: parser,
            core_p,
            core_kb,
            wm_data: Mutex::new(WmData {
                wakers: vec![],
                windows: vec![],
            }),
        });

        let wm = Some(tokio::task::spawn_local(wm::run(instance.clone())));

        Box::new(Arc::new(XInstance {
            data: instance.clone(),
            wm,
        }))
    }

    fn is_mt_safe(&self) -> bool {
        false
    }

    fn name(&self) -> &str {
        "x11"
    }
}

impl Instance for Arc<XInstance> {
    fn default_seat(&self) -> Box<dyn Seat> {
        Box::new(Arc::new(XSeat {
            instance: self.clone(),
            is_core: true,
            pointer: self.data.core_p,
            keyboard: self.data.core_kb,
        }))
    }

    fn create_event_loop(&self) -> Box<dyn EventLoop> {
        let _lock = ENV_LOCK.lock();
        std::env::set_var("DISPLAY", format!(":{}", self.data.display));
        let el = WEventLoop::new_x11_any_thread().unwrap();
        let xcon = el.xlib_xconnection().unwrap();
        let el = Arc::new(XEventLoopData {
            instance: self.clone(),
            el: Mutex::new(el),
            waiters: Default::default(),
            events: Default::default(),
        });
        let el2 = el.clone();
        let jh = tokio::task::spawn_local(async move {
            let afd = AsyncFd::with_interest(xcon.fd, Interest::READABLE).unwrap();
            loop {
                afd.readable().await.unwrap().clear_ready();
                let mut el = el2.el.lock();
                let mut events = el2.events.lock();
                el.run_return(|ev, _, cf| {
                    *cf = ControlFlow::Exit;
                    if let Some(ev) = map_event(ev) {
                        log::debug!("winit event: {:?}", ev);
                        events.push_back(ev);
                    }
                });
                let mut waiters = el2.waiters.lock();
                for waiter in waiters.drain(..) {
                    waiter.wake();
                }
            }
        });
        Box::new(XEventLoop {
            data: el,
            jh: Some(jh),
        })
    }

    fn set_background_color(&self, window: &Window, r: u8, g: u8, b: u8) {
        let window = window.xlib_window().unwrap();
        let color = b as u32 | (g as u32) << 8 | (r as u32) << 16;
        unsafe {
            let cookie = self.data.backend.xcb.xcb_change_window_attributes_checked(
                self.data.c,
                window as _,
                ffi::XCB_CW_BACK_PIXEL,
                &color as *const u32 as *const _,
            );
            if let Err(e) = self
                .data
                .errors
                .check_cookie(&self.data.backend.xcb, cookie)
            {
                panic!("Could not change back pixel: {}", e);
            }
            let cookie =
                self.data
                    .backend
                    .xcb
                    .xcb_clear_area(self.data.c, 0, window as _, 0, 0, 0, 0);
            if let Err(e) = self
                .data
                .errors
                .check_cookie(&self.data.backend.xcb, cookie)
            {
                panic!("Could not clear window: {}", e);
            }
        }
    }

    fn take_screenshot(&self) {
        unsafe {
            let mut err = ptr::null_mut();
            let reply = self.data.backend.xcb.xcb_get_geometry_reply(
                self.data.c,
                self.data
                    .backend
                    .xcb
                    .xcb_get_geometry(self.data.c, self.data.screen.root),
                &mut err,
            );
            let attr = self
                .data
                .errors
                .check(&self.data.backend.xcb, reply, err)
                .unwrap();
            let reply = self.data.backend.xcb.xcb_get_image_reply(
                self.data.c,
                self.data.backend.xcb.xcb_get_image(
                    self.data.c,
                    ffi::XCB_IMAGE_FORMAT_Z_PIXMAP as u8,
                    self.data.screen.root,
                    attr.x,
                    attr.y,
                    attr.width,
                    attr.height,
                    !0,
                ),
                &mut err,
            );
            let image = self
                .data
                .errors
                .check(&self.data.backend.xcb, reply, err)
                .unwrap();
            let data = std::slice::from_raw_parts(
                self.data.backend.xcb.xcb_get_image_data(&*image),
                image.length as usize * 4,
            );
            crate::screenshot::log_image(data, attr.width as _, attr.height as _);
        }
    }

    fn mapped<'b>(&'b self, window: &Window) -> Pin<Box<dyn Future<Output=()> + 'b>> {
        struct Wait<'a>(&'a XInstanceData, ffi::xcb_window_t);
        impl<'a> Future for Wait<'a> {
            type Output = ();
            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let mut data = self.0.wm_data.lock();
                for w in &data.windows {
                    if w.id == self.1 && w.mapped {
                        return Poll::Ready(());
                    }
                }
                data.wakers.push(cx.waker().clone());
                Poll::Pending
            }
        }
        Box::pin(Wait(&self.data, window.xlib_window().unwrap() as _))
    }
}

struct WmData {
    wakers: Vec<Waker>,
    windows: Vec<WindowData>,
}

impl WmData {
    fn changed(&mut self) {
        for waker in self.wakers.drain(..) {
            waker.wake();
        }
    }
}

struct WindowData {
    id: ffi::xcb_window_t,
    mapped: bool,
}

impl Drop for XInstanceData {
    fn drop(&mut self) {
        unsafe {
            self.backend.xcb.xcb_disconnect(self.c);
        }
        log::info!("Killing the X server");
        uapi::kill(self.xserver_pid, libc::SIGKILL).unwrap();
        log::info!("Waiting for the X server to terminate");
        uapi::waitpid(self.xserver_pid, 0).unwrap();
    }
}

impl Drop for XInstance {
    fn drop(&mut self) {
        self.wm.take().unwrap().abort();
    }
}

struct XEventLoopData {
    instance: Arc<XInstance>,
    el: Mutex<WEventLoop<Box<dyn Any>>>,
    waiters: Mutex<Vec<Waker>>,
    events: Mutex<VecDeque<Event<Box<dyn Any>>>>,
}

struct XEventLoop {
    data: Arc<XEventLoopData>,
    jh: Option<JoinHandle<()>>,
}

impl Drop for XEventLoop {
    fn drop(&mut self) {
        self.jh.take().unwrap().abort();
    }
}

impl EventLoop for XEventLoop {
    fn event<'a>(&'a self) -> Pin<Box<dyn Future<Output=Event<Box<dyn Any>>> + 'a>> {
        struct Changed<'b>(&'b XEventLoopData);
        impl<'b> Future for Changed<'b> {
            type Output = Event<Box<dyn Any>>;
            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                if let Some(e) = self.0.events.lock().pop_front() {
                    Poll::Ready(e)
                } else {
                    self.0.waiters.lock().push(cx.waker().clone());
                    Poll::Pending
                }
            }
        }
        Box::pin(Changed(&self.data))
    }

    fn create_window(&self, builder: WindowBuilder) -> Window {
        builder.build(&*self.data.el.lock()).unwrap()
    }
}

struct XSeat {
    instance: Arc<XInstance>,
    is_core: bool,
    pointer: ffi::xcb_input_device_id_t,
    keyboard: ffi::xcb_input_device_id_t,
}

impl Seat for Arc<XSeat> {
    fn add_keyboard(&self) -> Box<dyn Keyboard> {
        let mut msg = Message {
            ty: MT_CREATE_KEYBOARD as _,
        };
        uapi::write(self.instance.data.sock.raw(), &msg).unwrap();
        uapi::read(self.instance.data.sock.raw(), &mut msg).unwrap();
        let id = unsafe {
            assert_eq!(msg.ty, MT_CREATE_KEYBOARD_REPLY as _);
            msg.create_keyboard_reply.id
        };
        assert!(self.is_core);
        Box::new(Arc::new(XKeyboard {
            seat: self.clone(),
            pressed_keys: Default::default(),
            id: id as _,
        }))
    }

    fn add_mouse(&self) -> Box<dyn Mouse> {
        todo!()
    }

    fn focus(&self, window: &Window) {
        let window = window.xlib_window().unwrap();
        unsafe {
            let cookie = self
                .instance
                .data
                .backend
                .xinput
                .xcb_input_xi_set_focus_checked(
                    self.instance.data.c,
                    window as _,
                    0,
                    self.keyboard,
                );
            if let Err(e) = self
                .instance
                .data
                .errors
                .check_cookie(&self.instance.data.backend.xcb, cookie)
            {
                panic!("Could not set focus: {}", e);
            }
        }
    }
}

struct XKeyboard {
    seat: Arc<XSeat>,
    pressed_keys: Mutex<HashMap<Key, Weak<XPressedKey>>>,
    id: ffi::xcb_input_device_id_t,
}

impl Keyboard for Arc<XKeyboard> {
    fn press(&self, key: Key) -> Box<dyn PressedKey> {
        let mut keys = self.pressed_keys.lock();
        if let Some(p) = keys.get(&key) {
            if let Some(p) = p.upgrade() {
                return Box::new(p);
            }
        }
        let msg = Message {
            key_press: KeyPress {
                ty: MT_KEY_PRESS as _,
                id: self.id as _,
                key: evdev::map_key(key),
            },
        };
        uapi::write(self.seat.instance.data.sock.raw(), &msg).unwrap();
        let p = Arc::new(XPressedKey {
            kb: self.clone(),
            key,
        });
        keys.insert(key, Arc::downgrade(&p));
        Box::new(p)
    }
}

struct XPressedKey {
    kb: Arc<XKeyboard>,
    key: Key,
}

impl PressedKey for Arc<XPressedKey> {}

impl Drop for XPressedKey {
    fn drop(&mut self) {
        let msg = Message {
            key_press: KeyPress {
                ty: MT_KEY_RELEASE as _,
                id: self.kb.id as _,
                key: evdev::map_key(self.key),
            },
        };
        uapi::write(self.kb.seat.instance.data.sock.raw(), &msg).unwrap();
    }
}

const CONFIG: &str = r#"
Section "Device"
    Identifier  "winit device"
    Driver      "winit"
EndSection

Section "Screen"
    Identifier  "winit screen"
    Device      "winit device"
EndSection

Section "Serverlayout"
    Identifier  "winit layout"
    Screen      "winit screen"
EndSection
"#;

#[repr(u32)]
#[allow(dead_code, non_camel_case_types)]
enum MessageType {
    MT_NONE,
    MT_CREATE_KEYBOARD,
    MT_CREATE_KEYBOARD_REPLY,
    MT_KEY_PRESS,
    MT_KEY_RELEASE,
}

#[repr(C)]
#[derive(Copy, Clone)]
union Message {
    ty: u32,
    create_keyboard_reply: CreateKeyboardReply,
    key_press: KeyPress,
}

unsafe impl Pod for Message {}

#[repr(C)]
#[derive(Copy, Clone)]
struct CreateKeyboardReply {
    ty: u32,
    id: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct KeyPress {
    ty: u32,
    id: u32,
    key: u32,
}
