use crate::backend::{
    Backend, BackendDeviceId, BackendFlags, BackendIcon, Device, EventLoop, Instance, Keyboard,
    Mouse, PressedKey, Seat, Window, WindowProperties,
};
use crate::backends::x11::layout::{layouts, set_names, Layouts};
use crate::backends::x11::wm::TITLE_HEIGHT;
use crate::backends::x11::MessageType::MT_REMOVE_DEVICE;
use crate::event::{map_event, Event, UserEvent};
use crate::keyboard::{Key, Layout};
use parking_lot::Mutex;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::Display;
use std::future::Future;
use std::pin::Pin;
use std::process::Command;
use std::sync::{Arc, Weak};
use std::task::{Context, Poll, Waker};
use std::{mem, ptr};
use tokio::io::unix::AsyncFd;
use tokio::io::Interest;
use tokio::task::JoinHandle;
use uapi::c::{AF_UNIX, O_CLOEXEC, SOCK_CLOEXEC, SOCK_SEQPACKET};
use uapi::{pipe2, socketpair, IntoUstr, OwnedFd, Pod, UapiReadExt, UstrPtr};
use winit::event::DeviceId;
use winit::event_loop::{ControlFlow, EventLoop as WEventLoop};
use winit::platform::run_return::EventLoopExtRunReturn;
use winit::platform::unix::{
    DeviceIdExtUnix, EventLoopExtUnix, EventLoopWindowTargetExtUnix, WindowExtUnix,
};
use winit::window::{Window as WWindow, WindowBuilder};
use xcb_dl::{ffi, Xcb, XcbRender, XcbXinput, XcbXkb};
use xcb_dl_util::error::XcbErrorParser;
use MessageType::{MT_CREATE_KEYBOARD, MT_CREATE_KEYBOARD_REPLY, MT_KEY_PRESS, MT_KEY_RELEASE};

mod evdev;
mod keysyms;
mod layout;
mod wm;

static ENV_LOCK: Mutex<()> = parking_lot::const_mutex(());

const DEFAULT_X_PATH: &str = "/usr/lib/Xorg";
// const DEFAULT_X_PATH: &str = "/home/julian/c/xserver/install/bin/X";

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
            render: XcbRender::load_loose().unwrap(),
            xkb: XcbXkb::load_loose().unwrap(),
            layouts: layouts(),
        }))
    }
}

struct XBackend {
    x_path: String,
    default_module_path: String,
    xcb: Xcb,
    xinput: XcbXinput,
    render: XcbRender,
    xkb: XcbXkb,
    layouts: Layouts,
}

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

        let mut instance = XInstanceData {
            backend: self.clone(),
            xserver_pid: chpid,
            sock: psock,
            display,
            wm_data: Mutex::new(WmData {
                wakers: vec![],
                windows: Default::default(),
                parents: Default::default(),
                pongs: Default::default(),
            }),
            atoms: Default::default(),
        };

        let c = XConnection::new(self, display);

        instance.atoms.net_wm_state = c.atom("_NET_WM_STATE");
        instance.atoms.wm_change_state = c.atom("WM_CHANGE_STATE");
        instance.atoms.wm_state = c.atom("WM_STATE");
        instance.atoms.net_wm_name = c.atom("_NET_WM_NAME");
        instance.atoms.net_wm_icon = c.atom("_NET_WM_ICON");
        instance.atoms.wm_delete_window = c.atom("WM_DELETE_WINDOW");
        instance.atoms.net_wm_ping = c.atom("_NET_WM_PING");
        instance.atoms.utf8_string = c.atom("UTF8_STRING");
        instance.atoms.net_wm_state_above = c.atom("_NET_WM_STATE_ABOVE");
        instance.atoms.net_frame_extents = c.atom("_NET_FRAME_EXTENTS");
        instance.atoms.net_wm_state_maximized_horz = c.atom("_NET_WM_STATE_MAXIMIZED_HORZ");
        instance.atoms.net_wm_state_maximized_vert = c.atom("_NET_WM_STATE_MAXIMIZED_VERT");
        instance.atoms.motif_wm_hints = c.atom("_MOTIF_WM_HINTS");
        instance.atoms.wm_name = c.atom("WM_NAME");
        instance.atoms.wm_normal_hints = c.atom("WM_NORMAL_HINTS");
        instance.atoms.wm_hints = c.atom("WM_HINTS");
        instance.atoms.wm_class = c.atom("WM_CLASS");
        instance.atoms.wm_protocols = c.atom("WM_PROTOCOLS");
        instance.atoms.net_active_window = c.atom("_NET_ACTIVE_WINDOW");
        instance.atoms.net_supported = c.atom("_NET_SUPPORTED");
        instance.atoms.net_client_list = c.atom("_NET_CLIENT_LIST");
        instance.atoms.net_client_list_stacking = c.atom("_NET_CLIENT_LIST_STACKING");
        instance.atoms.net_frame_extents = c.atom("_NET_FRAME_EXTENTS");
        instance.atoms.net_supporting_wm_check = c.atom("_NET_SUPPORTING_WM_CHECK");

        let instance = Arc::new(instance);

        let wm = Some(tokio::task::spawn_local(wm::run(instance.clone())));

        let (core_p, core_kb) = unsafe {
            let mut err = ptr::null_mut();
            let reply = self.xkb.xcb_xkb_use_extension_reply(
                c.c,
                self.xkb.xcb_xkb_use_extension(c.c, 1, 0),
                &mut err,
            );
            c.errors.check(&self.xcb, reply, err).unwrap();
            let cookie = self.xinput.xcb_input_xi_query_version(c.c, 2, 0);
            let reply = self
                .xinput
                .xcb_input_xi_query_version_reply(c.c, cookie, &mut err);
            let _reply = c.errors.check(&self.xcb, reply, err).unwrap();
            let cookie = self
                .xinput
                .xcb_input_xi_query_device(c.c, ffi::XCB_INPUT_DEVICE_ALL_MASTER as _);
            let reply = self
                .xinput
                .xcb_input_xi_query_device_reply(c.c, cookie, &mut err);
            let reply = c.errors.check(&self.xcb, reply, err).unwrap();
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

        Box::new(Arc::new(XInstance {
            c,
            data: instance.clone(),
            wm,
            core_p,
            core_kb,
            core_layout: Arc::new(Cell::new(Layout::Qwerty)),
        }))
    }

    fn name(&self) -> &str {
        "x11"
    }

    fn flags(&self) -> BackendFlags {
        BackendFlags::MT_SAFE
            | BackendFlags::WINIT_SET_ALWAYS_ON_TOP
            | BackendFlags::WINIT_SET_DECORATIONS
            | BackendFlags::WINIT_SET_INNER_SIZE
            | BackendFlags::WINIT_SET_OUTER_POSITION
            | BackendFlags::WINIT_SET_TITLE
            | BackendFlags::WINIT_SET_VISIBLE
            | BackendFlags::WINIT_SET_MAXIMIZED
            | BackendFlags::WINIT_SET_MINIMIZED
            | BackendFlags::WINIT_SET_SIZE_BOUNDS
            | BackendFlags::WINIT_SET_ATTENTION
            | BackendFlags::WINIT_SET_RESIZABLE
            | BackendFlags::WINIT_SET_ICON
            // | BackendFlags::WINIT_TRANSPARENCY
            | BackendFlags::X11
            | BackendFlags::SET_OUTER_POSITION
            | BackendFlags::SET_INNER_SIZE
            | BackendFlags::DEVICE_ADDED
            | BackendFlags::DEVICE_REMOVED
    }
}

struct XConnection {
    backend: Arc<XBackend>,
    c: *mut ffi::xcb_connection_t,
    fd: libc::c_int,
    errors: XcbErrorParser,
    screen: ffi::xcb_screen_t,
}

impl XConnection {
    fn new(backend: &Arc<XBackend>, display: u32) -> Self {
        unsafe {
            let display_str = uapi::format_ustr!(":{}", display);
            let c = backend
                .xcb
                .xcb_connect(display_str.as_ptr(), ptr::null_mut());
            let parser = XcbErrorParser::new(&backend.xcb, c);
            parser.check_connection(&backend.xcb).unwrap();
            let screen = *backend
                .xcb
                .xcb_setup_roots_iterator(backend.xcb.xcb_get_setup(c))
                .data;
            Self {
                backend: backend.clone(),
                c,
                fd: backend.xcb.xcb_get_file_descriptor(c),
                errors: parser,
                screen,
            }
        }
    }

    fn atom(&self, name: &str) -> ffi::xcb_atom_t {
        unsafe {
            let mut err = ptr::null_mut();
            let reply = self.backend.xcb.xcb_intern_atom_reply(
                self.c,
                self.backend
                    .xcb
                    .xcb_intern_atom(self.c, 0, name.len() as _, name.as_ptr() as _),
                &mut err,
            );
            self.errors
                .check(&self.backend.xcb, reply, err)
                .unwrap()
                .atom
        }
    }
}

impl Drop for XConnection {
    fn drop(&mut self) {
        unsafe {
            self.backend.xcb.xcb_disconnect(self.c);
        }
    }
}

struct XInstanceData {
    backend: Arc<XBackend>,
    xserver_pid: libc::pid_t,
    sock: OwnedFd,
    display: u32,
    wm_data: Mutex<WmData>,
    atoms: Atoms,
}

struct XInstance {
    c: XConnection,
    data: Arc<XInstanceData>,
    wm: Option<JoinHandle<()>>,
    core_p: ffi::xcb_input_device_id_t,
    core_kb: ffi::xcb_input_device_id_t,
    core_layout: Arc<Cell<Layout>>,
}

unsafe impl Send for XInstance {}
unsafe impl Sync for XInstance {}

impl XInstance {
    fn add_keyboard(&self) -> ffi::xcb_input_device_id_t {
        let mut msg = Message {
            ty: MT_CREATE_KEYBOARD as _,
        };
        uapi::write(self.data.sock.raw(), &msg).unwrap();
        uapi::read(self.data.sock.raw(), &mut msg).unwrap();
        unsafe {
            assert_eq!(msg.ty, MT_CREATE_KEYBOARD_REPLY as _);
            msg.create_keyboard_reply.id as _
        }
    }

    fn assign_slave(&self, slave: ffi::xcb_input_device_id_t, master: ffi::xcb_input_device_id_t) {
        unsafe {
            let xcb = &self.data.backend.xcb;
            let xinput = &self.data.backend.xinput;
            #[repr(C)]
            struct Change {
                hc: ffi::xcb_input_hierarchy_change_t,
                data: ffi::xcb_input_hierarchy_change_data_t__attach_slave,
            }
            let change = Change {
                hc: ffi::xcb_input_hierarchy_change_t {
                    type_: ffi::XCB_INPUT_HIERARCHY_CHANGE_TYPE_ATTACH_SLAVE as _,
                    len: (mem::size_of::<Change>() / 4) as _,
                },
                data: ffi::xcb_input_hierarchy_change_data_t__attach_slave {
                    deviceid: slave,
                    master,
                },
            };
            let cookie = xinput.xcb_input_xi_change_hierarchy_checked(self.c.c, 1, &change.hc);
            if let Err(e) = self.c.errors.check_cookie(xcb, cookie) {
                panic!("Could not assign slave to master: {}", e);
            }
        }
    }

    fn set_layout(
        &self,
        slave: ffi::xcb_input_device_id_t,
        layout: Layout,
        prev_layout: Option<Layout>,
    ) {
        if Some(layout) == prev_layout {
            return;
        }
        let change_map = match (layout, prev_layout) {
            (_, None) => true,
            (Layout::QwertySwapped, _) => true,
            (_, Some(Layout::QwertySwapped)) => true,
            _ => false,
        };
        let backend = &self.data.backend;
        let (group, msg) = match layout {
            Layout::Qwerty => (0, &backend.layouts.msg1),
            Layout::Azerty => (1, &backend.layouts.msg1),
            Layout::QwertySwapped => (0, &backend.layouts.msg2),
        };
        unsafe {
            let xcb = &self.data.backend.xcb;
            let xkb = &self.data.backend.xkb;
            if change_map {
                let mut header = msg.header;
                header.device_spec = slave;
                let mut iovecs = [
                    libc::iovec {
                        iov_base: ptr::null_mut(),
                        iov_len: 0,
                    },
                    libc::iovec {
                        iov_base: ptr::null_mut(),
                        iov_len: 0,
                    },
                    libc::iovec {
                        iov_base: &mut header as *mut _ as _,
                        iov_len: mem::size_of_val(&header),
                    },
                    libc::iovec {
                        iov_base: msg.body.as_ptr() as _,
                        iov_len: msg.body.len(),
                    },
                ];
                let request = ffi::xcb_protocol_request_t {
                    count: 2,
                    ext: xkb.xcb_xkb_id(),
                    opcode: ffi::XCB_XKB_SET_MAP,
                    isvoid: 1,
                };
                let sequence = xcb.xcb_send_request(
                    self.c.c,
                    ffi::XCB_REQUEST_CHECKED,
                    &mut iovecs[2],
                    &request,
                );
                let cookie = ffi::xcb_void_cookie_t { sequence };
                if let Err(e) = self.c.errors.check_cookie(xcb, cookie) {
                    panic!("Could not set keymap: {}", e);
                }
                let cookie = set_names(xkb, &self.c, slave);
                if let Err(e) = self.c.errors.check_cookie(xcb, cookie) {
                    panic!("Could not set level names: {}", e);
                }
            }
            let cookie =
                xkb.xcb_xkb_latch_lock_state_checked(self.c.c, slave, 0, 0, 1, group, 0, 0, 0);
            if let Err(e) = self.c.errors.check_cookie(xcb, cookie) {
                panic!("Could not set keymap group: {}", e);
            }
        }
    }
}

impl Instance for Arc<XInstance> {
    fn backend(&self) -> &dyn Backend {
        &self.data.backend
    }

    fn default_seat(&self) -> Box<dyn Seat> {
        Box::new(Arc::new(XSeat {
            instance: self.clone(),
            pointer: self.core_p,
            keyboard: self.core_kb,
            layout: self.core_layout.clone(),
        }))
    }

    fn create_event_loop(&self) -> Box<dyn EventLoop> {
        let _lock = ENV_LOCK.lock();
        std::env::set_var("DISPLAY", format!(":{}", self.data.display));
        let el = WEventLoop::new_x11_any_thread().unwrap();
        let el_c = el.xcb_connection().unwrap();
        let el_fd = unsafe { self.data.backend.xcb.xcb_get_file_descriptor(el_c as _) };
        let el = Arc::new(XEventLoopData {
            instance: self.clone(),
            el: Mutex::new(el),
            waiters: Default::default(),
            events: Default::default(),
            version: Cell::new(1),
        });
        let el2 = el.clone();
        let jh = tokio::task::spawn_local(async move {
            let afd = AsyncFd::with_interest(el_fd, Interest::READABLE).unwrap();
            loop {
                {
                    let mut el = el2.el.lock();
                    let mut events = el2.events.lock();
                    el.run_return(|ev, _, cf| {
                        *cf = ControlFlow::Exit;
                        if let Some(ev) = map_event(ev) {
                            log::debug!("winit event: {:?}", ev);
                            events.push_back(ev);
                        }
                    });
                    log::info!("Winit event loop ran");
                    el2.version.set(el2.version.get() + 1);
                    let mut waiters = el2.waiters.lock();
                    for waiter in waiters.drain(..) {
                        waiter.wake();
                    }
                }
                afd.readable().await.unwrap().clear_ready();
            }
        });
        Box::new(Arc::new(XEventLoop {
            data: el,
            jh: Some(jh),
        }))
    }

    fn take_screenshot(&self) {
        unsafe {
            let mut err = ptr::null_mut();
            let reply = self.data.backend.xcb.xcb_get_geometry_reply(
                self.c.c,
                self.data
                    .backend
                    .xcb
                    .xcb_get_geometry(self.c.c, self.c.screen.root),
                &mut err,
            );
            let attr = self
                .c
                .errors
                .check(&self.data.backend.xcb, reply, err)
                .unwrap();
            let reply = self.data.backend.xcb.xcb_get_image_reply(
                self.c.c,
                self.data.backend.xcb.xcb_get_image(
                    self.c.c,
                    ffi::XCB_IMAGE_FORMAT_Z_PIXMAP as u8,
                    self.c.screen.root,
                    attr.x,
                    attr.y,
                    attr.width,
                    attr.height,
                    !0,
                ),
                &mut err,
            );
            let image = self
                .c
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
}

struct WmData {
    wakers: Vec<Waker>,
    windows: HashMap<ffi::xcb_window_t, Weak<XWindow>>,
    parents: HashMap<ffi::xcb_window_t, Weak<XWindow>>,
    pongs: HashSet<ffi::xcb_window_t>,
}

impl WmData {
    fn changed(&mut self) {
        for waker in self.wakers.drain(..) {
            waker.wake();
        }
    }

    fn window(&self, win: ffi::xcb_window_t) -> Option<Arc<XWindow>> {
        if let Some(win) = self.windows.get(&win) {
            return win.upgrade();
        }
        None
    }

    fn parent(&self, win: ffi::xcb_window_t) -> Option<Arc<XWindow>> {
        if let Some(win) = self.parents.get(&win) {
            return win.upgrade();
        }
        None
    }
}

impl Drop for XInstanceData {
    fn drop(&mut self) {
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
    el: Mutex<WEventLoop<UserEvent>>,
    waiters: Mutex<Vec<Waker>>,
    events: Mutex<VecDeque<Event>>,
    version: Cell<u32>,
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

impl XEventLoop {
    fn get_window_format(&self, id: ffi::xcb_window_t) -> ffi::xcb_render_directformat_t {
        unsafe {
            let instance = &self.data.instance;
            let xcb = &instance.data.backend.xcb;
            let render = &instance.data.backend.render;
            let mut err = ptr::null_mut();
            let window_visual = {
                let reply = xcb.xcb_get_window_attributes_reply(
                    instance.c.c,
                    xcb.xcb_get_window_attributes(instance.c.c, id),
                    &mut err,
                );
                instance.c.errors.check(xcb, reply, err).unwrap().visual
            };
            let formats = render.xcb_render_query_pict_formats_reply(
                instance.c.c,
                render.xcb_render_query_pict_formats(instance.c.c),
                &mut err,
            );
            let formats = instance.c.errors.check(xcb, formats, err).unwrap();
            let mut screens = render.xcb_render_query_pict_formats_screens_iterator(&*formats);
            while screens.rem > 0 {
                let mut depths = render.xcb_render_pictscreen_depths_iterator(screens.data);
                while depths.rem > 0 {
                    let visuals = std::slice::from_raw_parts(
                        render.xcb_render_pictdepth_visuals(depths.data),
                        render.xcb_render_pictdepth_visuals_length(depths.data) as _,
                    );
                    for visual in visuals {
                        if visual.visual == window_visual {
                            let formats = std::slice::from_raw_parts(
                                render.xcb_render_query_pict_formats_formats(&*formats),
                                formats.num_formats as _,
                            );
                            for format in formats {
                                if format.id == visual.format {
                                    return format.direct;
                                }
                            }
                        }
                    }
                    render.xcb_render_pictdepth_next(&mut depths);
                }
                render.xcb_render_pictscreen_next(&mut screens);
            }
            unreachable!();
        }
    }
}

impl EventLoop for Arc<XEventLoop> {
    fn event<'a>(&'a self) -> Pin<Box<dyn Future<Output = Event> + 'a>> {
        struct Changed<'b>(&'b XEventLoopData);
        impl<'b> Future for Changed<'b> {
            type Output = Event;
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

    fn changed<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + 'a>> {
        struct Changed<'b>(&'b XEventLoopData, u32);
        impl<'b> Future for Changed<'b> {
            type Output = ();
            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                if self.1 != self.0.version.get() {
                    Poll::Ready(())
                } else {
                    self.0.waiters.lock().push(cx.waker().clone());
                    Poll::Pending
                }
            }
        }
        Box::pin(Changed(&self.data, self.data.version.get()))
    }

    fn create_window(&self, builder: WindowBuilder) -> Box<dyn Window> {
        let winit = builder.build(&*self.data.el.lock()).unwrap();
        let id = winit.x11_window().unwrap();
        let format = self.get_window_format(id);
        log::info!("Created window {}", id);
        log::info!("Pixel format: {:?}", format);
        let win = Arc::new(XWindow {
            el: self.clone(),
            id,
            format,
            parent_id: Cell::new(0),
            winit: Some(winit),
            property_generation: Cell::new(0),
            created: Cell::new(false),
            destroyed: Cell::new(false),
            mapped: Cell::new(false),
            always_on_top: Cell::new(false),
            maximized_vert: Cell::new(false),
            maximized_horz: Cell::new(false),
            decorations: Cell::new(true),
            border: Cell::new(0),
            x: Cell::new(0),
            y: Cell::new(0),
            width: Cell::new(0),
            height: Cell::new(0),
            min_size: Cell::new(None),
            max_size: Cell::new(None),
            wm_name: RefCell::new("".to_string()),
            utf8_title: RefCell::new("".to_string()),
            urgency: Cell::new(false),
            class: RefCell::new(None),
            instance: RefCell::new(None),
            protocols: Cell::new(Protocols::empty()),
            initial_state: Cell::new(WindowState::Withdrawn),
            desired_state: Cell::new(WindowState::Withdrawn),
            current_state: Cell::new(WindowState::Withdrawn),
            maximizable: Cell::new(true),
            icon: RefCell::new(None),
        });
        self.data
            .instance
            .data
            .wm_data
            .lock()
            .windows
            .insert(win.id, Arc::downgrade(&win));
        Box::new(win)
    }
}

bitflags::bitflags! {
    struct Protocols: u32 {
        const DELETE_WINDOW = 1 << 0;
        const PING = 1 << 1;
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum WindowState {
    Withdrawn,
    Normal,
    Iconic,
}

struct XWindow {
    el: Arc<XEventLoop>,
    id: ffi::xcb_window_t,
    format: ffi::xcb_render_directformat_t,
    parent_id: Cell<ffi::xcb_window_t>,
    winit: Option<WWindow>,
    property_generation: Cell<u32>,
    created: Cell<bool>,
    destroyed: Cell<bool>,
    mapped: Cell<bool>,
    always_on_top: Cell<bool>,
    maximized_vert: Cell<bool>,
    maximized_horz: Cell<bool>,
    decorations: Cell<bool>,
    border: Cell<u32>,
    x: Cell<i32>,
    y: Cell<i32>,
    width: Cell<u32>,
    height: Cell<u32>,
    min_size: Cell<Option<(u32, u32)>>,
    max_size: Cell<Option<(u32, u32)>>,
    wm_name: RefCell<String>,
    utf8_title: RefCell<String>,
    urgency: Cell<bool>,
    class: RefCell<Option<String>>,
    instance: RefCell<Option<String>>,
    protocols: Cell<Protocols>,
    initial_state: Cell<WindowState>,
    desired_state: Cell<WindowState>,
    current_state: Cell<WindowState>,
    maximizable: Cell<bool>,
    icon: RefCell<Option<BackendIcon>>,
}

impl XWindow {
    fn upgade(&self) {
        self.property_generation
            .set(self.property_generation.get() + 1);
    }
}

impl Window for Arc<XWindow> {
    fn id(&self) -> &dyn Display {
        &self.id
    }

    fn backend(&self) -> &dyn Backend {
        &self.el.data.instance.data.backend
    }

    fn event_loop(&self) -> &dyn EventLoop {
        &self.el
    }

    fn winit(&self) -> &WWindow {
        self.winit.as_ref().unwrap()
    }

    fn properties_changed<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + 'a>> {
        struct Changed<'b>(&'b XWindow, u32);
        impl<'b> Future for Changed<'b> {
            type Output = ();
            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                if self.1 != self.0.property_generation.get() {
                    Poll::Ready(())
                } else {
                    let mut data = self.0.el.data.instance.data.wm_data.lock();
                    data.wakers.push(cx.waker().clone());
                    Poll::Pending
                }
            }
        }
        Box::pin(Changed(&self, self.property_generation.get()))
    }

    fn properties(&self) -> &dyn WindowProperties {
        self
    }

    fn set_inner_size(&self, width: u32, height: u32) {
        unsafe {
            let instance = &self.el.data.instance;
            let xcb = &instance.data.backend.xcb;
            let cookie = xcb.xcb_configure_window_checked(
                instance.c.c,
                self.id,
                (ffi::XCB_CONFIG_WINDOW_WIDTH | ffi::XCB_CONFIG_WINDOW_HEIGHT) as _,
                [width, height].as_ptr() as _,
            );
            if let Err(e) = instance.c.errors.check_cookie(xcb, cookie) {
                log::warn!("Could not resize window: {}", e);
            }
        }
    }

    fn set_background_color(&self, r: u8, g: u8, b: u8) {
        let color = b as u32 | (g as u32) << 8 | (r as u32) << 16;
        let instance = &self.el.data.instance;
        let backend = &instance.data.backend;
        unsafe {
            let cookie = backend.xcb.xcb_change_window_attributes_checked(
                self.el.data.instance.c.c,
                self.id,
                ffi::XCB_CW_BACK_PIXEL,
                &color as *const u32 as *const _,
            );
            if let Err(e) = instance.c.errors.check_cookie(&backend.xcb, cookie) {
                panic!("Could not change back pixel: {}", e);
            }
            let cookie = backend
                .xcb
                .xcb_clear_area(instance.c.c, 0, self.id, 0, 0, 0, 0);
            if let Err(e) = instance.c.errors.check_cookie(&backend.xcb, cookie) {
                panic!("Could not clear window: {}", e);
            }
        }
    }

    fn any(&self) -> &dyn Any {
        self
    }

    fn delete(&self) {
        log::info!("Deleting window {}", self.id);
        unsafe {
            let instance = &self.el.data.instance;
            let xcb = &instance.data.backend.xcb;
            let protocols = self.protocols.get();
            let cookie = if protocols.contains(Protocols::DELETE_WINDOW) {
                let event = ffi::xcb_client_message_event_t {
                    response_type: ffi::XCB_CLIENT_MESSAGE,
                    format: 32,
                    window: self.id,
                    type_: instance.data.atoms.wm_protocols,
                    data: ffi::xcb_client_message_data_t {
                        data32: [instance.data.atoms.wm_delete_window, 0, 0, 0, 0],
                    },
                    ..Default::default()
                };
                xcb.xcb_send_event_checked(instance.c.c, 0, self.id, 0, &event as *const _ as _)
            } else {
                xcb.xcb_destroy_window_checked(instance.c.c, self.id)
            };
            if let Err(e) = instance.c.errors.check_cookie(xcb, cookie) {
                log::warn!("Could not destroy window: {}", e);
            }
        }
    }

    fn frame_extents(&self) -> (u32, u32, u32, u32) {
        (
            self.border.get(),
            self.border.get(),
            self.border.get() + TITLE_HEIGHT as u32,
            self.border.get(),
        )
    }

    fn set_outer_position(&self, x: i32, y: i32) {
        log::info!("Setting outer position of {} to {}x{}", self.id, x, y);
        unsafe {
            let instance = &self.el.data.instance;
            let xcb = &instance.data.backend.xcb;
            let cookie = xcb.xcb_configure_window_checked(
                instance.c.c,
                self.parent_id.get(),
                (ffi::XCB_CONFIG_WINDOW_X | ffi::XCB_CONFIG_WINDOW_Y) as _,
                [x, y].as_ptr() as _,
            );
            if let Err(e) = instance.c.errors.check_cookie(xcb, cookie) {
                log::warn!("Could not configure window: {}", e);
            }
        }
    }

    fn ping<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + 'a>> {
        struct Changed<'b>(&'b XWindow);
        impl<'b> Future for Changed<'b> {
            type Output = ();
            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let mut data = self.0.el.data.instance.data.wm_data.lock();
                if data.pongs.remove(&self.0.id) {
                    Poll::Ready(())
                } else {
                    data.wakers.push(cx.waker().clone());
                    Poll::Pending
                }
            }
        }
        log::info!("Pinging {}", self.id);
        self.el
            .data
            .instance
            .data
            .wm_data
            .lock()
            .pongs
            .remove(&self.id);
        unsafe {
            let instance = &self.el.data.instance;
            let xcb = &instance.data.backend.xcb;
            let msg = ffi::xcb_client_message_event_t {
                response_type: ffi::XCB_CLIENT_MESSAGE,
                format: 32,
                window: self.id,
                type_: instance.data.atoms.wm_protocols,
                data: ffi::xcb_client_message_data_t {
                    data32: [instance.data.atoms.net_wm_ping, 0, self.id, 0, 0],
                },
                ..Default::default()
            };
            xcb.xcb_send_event(instance.c.c, 0, self.id, 0, &msg as *const _ as _);
            xcb.xcb_flush(instance.c.c);
        }
        Box::pin(Changed(&self))
    }
}

impl WindowProperties for Arc<XWindow> {
    fn mapped(&self) -> bool {
        self.mapped.get()
    }

    fn always_on_top(&self) -> bool {
        self.always_on_top.get()
    }

    fn decorations(&self) -> bool {
        self.decorations.get()
    }

    fn x(&self) -> i32 {
        self.x.get()
    }

    fn y(&self) -> i32 {
        self.y.get()
    }

    fn width(&self) -> u32 {
        self.width.get()
    }

    fn height(&self) -> u32 {
        self.height.get()
    }

    fn min_size(&self) -> Option<(u32, u32)> {
        self.min_size.get()
    }

    fn max_size(&self) -> Option<(u32, u32)> {
        self.max_size.get()
    }

    fn title(&self) -> Option<String> {
        let title = self.wm_name.borrow();
        let utf8_title = self.utf8_title.borrow();
        if *title == *utf8_title {
            return Some(title.to_string());
        }
        None
    }

    fn maximized(&self) -> Option<bool> {
        if self.maximized_vert.get() == self.maximized_horz.get() {
            Some(self.maximized_vert.get())
        } else {
            None
        }
    }

    fn minimized(&self) -> Option<bool> {
        Some(self.current_state.get() == WindowState::Iconic)
    }

    fn resizable(&self) -> Option<bool> {
        Some(
            self.max_size() != Some((self.width(), self.height()))
                || self.max_size() != self.min_size(),
        )
    }

    fn attention(&self) -> bool {
        self.urgency.get()
    }

    fn class(&self) -> Option<String> {
        self.class.borrow().clone()
    }

    fn instance(&self) -> Option<String> {
        self.instance.borrow().clone()
    }

    fn supports_transparency(&self) -> bool {
        self.format.alpha_mask != 0
    }

    fn icon(&self) -> Option<BackendIcon> {
        self.icon.borrow().clone()
    }
}

impl Drop for XWindow {
    fn drop(&mut self) {
        let data = &self.el.data.instance;
        data.data.wm_data.lock().windows.remove(&self.id);
        unsafe {
            if self.parent_id.get() != 0 {
                data.data
                    .backend
                    .xcb
                    .xcb_destroy_window(data.c.c, self.parent_id.get());
            }
        }
    }
}

struct XSeat {
    instance: Arc<XInstance>,
    pointer: ffi::xcb_input_device_id_t,
    keyboard: ffi::xcb_input_device_id_t,
    layout: Arc<Cell<Layout>>,
}

impl Seat for Arc<XSeat> {
    fn add_keyboard(&self) -> Box<dyn Keyboard> {
        let id = self.instance.add_keyboard();
        self.instance.assign_slave(id, self.keyboard);
        self.instance.set_layout(id, self.layout.get(), None);
        self.instance.set_layout(self.keyboard, self.layout.get(), None);
        Box::new(Arc::new(XKeyboard {
            pressed_keys: Default::default(),
            dev: XDevice {
                seat: self.clone(),
                id,
            },
        }))
    }

    fn add_mouse(&self) -> Box<dyn Mouse> {
        todo!()
    }

    fn focus(&self, window: &dyn Window) {
        let window: &Arc<XWindow> = window.any().downcast_ref().unwrap();
        unsafe {
            let cookie = self
                .instance
                .data
                .backend
                .xinput
                .xcb_input_xi_set_focus_checked(self.instance.c.c, window.id, 0, self.keyboard);
            if let Err(e) = self
                .instance
                .c
                .errors
                .check_cookie(&self.instance.data.backend.xcb, cookie)
            {
                panic!("Could not set focus: {}", e);
            }
        }
    }

    fn set_layout(&self, layout: Layout) {
        self.instance
            .set_layout(self.keyboard, layout, Some(self.layout.get()));
        self.layout.set(layout);
    }
}

impl Drop for XSeat {
    fn drop(&mut self) {
        if self.keyboard == self.instance.core_kb {
            return;
        }
        unsafe {
            let instance = &self.instance;
            let xinput = &instance.data.backend.xinput;
            let xcb = &instance.data.backend.xcb;
            #[repr(C)]
            struct Change {
                hc: ffi::xcb_input_hierarchy_change_t,
                data: ffi::xcb_input_hierarchy_change_data_t__remove_master,
            }
            let change = Change {
                hc: ffi::xcb_input_hierarchy_change_t {
                    type_: ffi::XCB_INPUT_HIERARCHY_CHANGE_TYPE_REMOVE_MASTER as _,
                    len: (mem::size_of::<Change>() / 4) as _,
                },
                data: ffi::xcb_input_hierarchy_change_data_t__remove_master {
                    deviceid: self.keyboard,
                    return_mode: ffi::XCB_INPUT_CHANGE_MODE_FLOAT as _,
                    ..Default::default()
                },
            };
            let cookie = xinput.xcb_input_xi_change_hierarchy_checked(instance.c.c, 1, &change.hc);
            if let Err(e) = instance.c.errors.check_cookie(xcb, cookie) {
                log::warn!("Could not remove master: {}", e);
            }
        }
    }
}

struct XDevice {
    seat: Arc<XSeat>,
    id: ffi::xcb_input_device_id_t,
}

impl Drop for XDevice {
    fn drop(&mut self) {
        let msg = Message {
            remove_device: RemoveDevice {
                ty: MT_REMOVE_DEVICE as _,
                id: self.id as _,
            },
        };
        uapi::write(self.seat.instance.data.sock.raw(), &msg).unwrap();
    }
}

struct XDeviceId {
    id: ffi::xcb_input_device_id_t,
}

impl BackendDeviceId for XDeviceId {
    fn is(&self, device: DeviceId) -> bool {
        device.xinput_id() == Some(self.id as u32)
    }
}

struct XKeyboard {
    pressed_keys: Mutex<HashMap<Key, Weak<XPressedKey>>>,
    dev: XDevice,
}

impl Device for Arc<XKeyboard> {
    fn id(&self) -> Box<dyn BackendDeviceId> {
        Box::new(XDeviceId { id: self.dev.id })
    }
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
                id: self.dev.id as _,
                key: evdev::map_key(key),
            },
        };
        uapi::write(self.dev.seat.instance.data.sock.raw(), &msg).unwrap();
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
                id: self.kb.dev.id as _,
                key: evdev::map_key(self.key),
            },
        };
        uapi::write(self.kb.dev.seat.instance.data.sock.raw(), &msg).unwrap();
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
    MT_REMOVE_DEVICE,
}

#[repr(C)]
#[derive(Copy, Clone)]
union Message {
    ty: u32,
    create_keyboard_reply: CreateKeyboardReply,
    key_press: KeyPress,
    remove_device: RemoveDevice,
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

#[repr(C)]
#[derive(Copy, Clone)]
struct RemoveDevice {
    ty: u32,
    id: u32,
}

#[derive(Default)]
struct Atoms {
    net_wm_state: ffi::xcb_atom_t,
    wm_change_state: ffi::xcb_atom_t,
    wm_state: ffi::xcb_atom_t,
    net_wm_name: ffi::xcb_atom_t,
    net_wm_icon: ffi::xcb_atom_t,
    wm_delete_window: ffi::xcb_atom_t,
    net_wm_ping: ffi::xcb_atom_t,
    utf8_string: ffi::xcb_atom_t,
    net_wm_state_above: ffi::xcb_atom_t,
    net_frame_extents: ffi::xcb_atom_t,
    net_wm_state_maximized_horz: ffi::xcb_atom_t,
    net_wm_state_maximized_vert: ffi::xcb_atom_t,
    motif_wm_hints: ffi::xcb_atom_t,
    wm_name: ffi::xcb_atom_t,
    wm_normal_hints: ffi::xcb_atom_t,
    wm_hints: ffi::xcb_atom_t,
    wm_class: ffi::xcb_atom_t,
    wm_protocols: ffi::xcb_atom_t,
    net_active_window: ffi::xcb_atom_t,
    net_supported: ffi::xcb_atom_t,
    net_client_list: ffi::xcb_atom_t,
    net_client_list_stacking: ffi::xcb_atom_t,
    net_supporting_wm_check: ffi::xcb_atom_t,
}
