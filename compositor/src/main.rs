//! flowde-compositor: mobile Wayland compositor (Smithay 0.7).
//!
//! Structure and protocol-handler boilerplate follow Smithay's MIT-licensed `anvil` example and the
//! FeDe compositor (crates/fede-compositor, same author; read-only reference). No code is linked from
//! FeDe: see docs/mobile-needs-from-fede.md.
//!
//! Model: one output, two kinds of toplevels. The shell (app_id `flowde-shell`) is the base layer; any
//! other toplevel is an "app": fullscreen, exactly one visible at a time. Edge gestures are recognised
//! here (touch that starts in an edge band never reaches the client) and forwarded to the shell.

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("flowde-compositor only runs on Linux (use `cargo test --lib` for the portable parts)");
}

#[cfg(target_os = "linux")]
fn main() {
    if let Err(e) = linux::run() {
        eprintln!("flowde-compositor: {e}");
        std::process::exit(1);
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::io::{self, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use calloop::generic::Generic;
    use calloop::timer::{TimeoutAction, Timer};
    use calloop::{EventLoop, Interest, LoopHandle, LoopSignal, Mode as LoopMode, PostAction};
    use flowde_compositor::gesture::{Recognizer, Verdict};
    use flowde_compositor::protocol;
    use smithay::backend::input::{
        AbsolutePositionEvent, ButtonState, Event, InputBackend, InputEvent, KeyboardKeyEvent,
        PointerButtonEvent,
    };
    use smithay::backend::renderer::damage::OutputDamageTracker;
    use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
    use smithay::backend::renderer::gles::GlesRenderer;
    use smithay::backend::renderer::utils::on_commit_buffer_handler;
    use smithay::backend::winit::{self, WinitEvent, WinitGraphicsBackend};
    use smithay::delegate_compositor;
    use smithay::delegate_data_device;
    use smithay::delegate_layer_shell;
    use smithay::delegate_output;
    use smithay::delegate_seat;
    use smithay::delegate_shm;
    use smithay::delegate_xdg_shell;
    use smithay::desktop::space::space_render_elements;
    use smithay::desktop::{
        layer_map_for_output, PopupKind, PopupManager, Space, Window, WindowSurfaceType,
    };
    use smithay::input::keyboard::{FilterResult, XkbConfig};
    use smithay::input::touch::{DownEvent, MotionEvent, TouchHandle, UpEvent};
    use smithay::input::{Seat, SeatHandler, SeatState};
    use smithay::output::{Mode, Output, PhysicalProperties, Subpixel};
    use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
    use smithay::reexports::wayland_server::backend::{ClientData, ClientId, DisconnectReason};
    use smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer;
    use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
    use smithay::reexports::wayland_server::protocol::wl_seat;
    use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
    use smithay::reexports::wayland_server::{Client, Display, DisplayHandle};
    use smithay::reexports::winit::dpi::LogicalSize;
    use smithay::reexports::winit::window::Window as WinitWindow;
    use smithay::utils::{Logical, Point, Serial, Size, Transform, SERIAL_COUNTER};
    use smithay::wayland::buffer::BufferHandler;
    use smithay::wayland::compositor::{
        get_parent, is_sync_subsurface, with_states, CompositorClientState, CompositorHandler,
        CompositorState,
    };
    use smithay::wayland::output::{OutputHandler, OutputManagerState};
    use smithay::wayland::selection::data_device::{
        set_data_device_focus, ClientDndGrabHandler, DataDeviceHandler, DataDeviceState,
        ServerDndGrabHandler,
    };
    use smithay::wayland::selection::SelectionHandler;
    use smithay::wayland::shell::wlr_layer::{
        Layer, LayerSurface as WlrLayerSurface, WlrLayerShellHandler, WlrLayerShellState,
    };
    use smithay::wayland::shell::xdg::{
        PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
        XdgToplevelSurfaceData,
    };
    use smithay::wayland::shm::{ShmHandler, ShmState};
    use smithay::wayland::socket::ListeningSocketSource;
    use tracing::{info, warn};

    const SHELL_APP_ID: &str = "flowde-shell";
    fn slot() -> smithay::backend::input::TouchSlot {
        smithay::backend::input::TouchSlot::from(Some(0u32))
    }

    #[derive(Default)]
    struct ClientState {
        compositor_state: CompositorClientState,
    }
    impl ClientData for ClientState {
        fn initialized(&self, _: ClientId) {}
        fn disconnected(&self, _: ClientId, _: DisconnectReason) {}
    }

    struct Win {
        window: Window,
        shell: bool,
        mapped: bool,
    }

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum Focus {
        Shell,
        App(usize),
    }

    struct State {
        dh: DisplayHandle,
        loop_signal: LoopSignal,
        socket_name: std::ffi::OsString,
        compositor_state: CompositorState,
        shm_state: ShmState,
        xdg_shell_state: XdgShellState,
        layer_shell_state: WlrLayerShellState,
        data_device_state: DataDeviceState,
        output_manager_state: OutputManagerState,
        seat_state: SeatState<State>,
        seat: Seat<State>,
        touch: TouchHandle<State>,
        popups: PopupManager,
        space: Space<Window>,
        output: Option<Output>,
        wins: Vec<Win>,
        focus: Focus,
        recog: Recognizer,
        deg: u32,
        needs_redraw: bool,
        shell_sock: Option<UnixStream>,
        shell_sock_path: PathBuf,
        compositor_sock_path: PathBuf,
        winit: Option<WinitData>,
        // touch routing: did the active touch go to a client (true) or get consumed by an edge gesture
        touch_to_client: bool,
        mouse_down: bool,
        mouse_pos: Point<f64, Logical>,
        stats: Vec<f64>,
        last_stats: Instant,
        start: Instant,
    }

    struct WinitData {
        backend: WinitGraphicsBackend<GlesRenderer>,
        damage: OutputDamageTracker,
    }

    // ---- state helpers ----------------------------------------------------------------------

    impl State {
        fn now_ms(&self) -> u32 {
            self.start.elapsed().as_millis() as u32
        }

        fn logical_size(&self) -> Size<i32, Logical> {
            self.output
                .as_ref()
                .and_then(|o| self.space.output_geometry(o))
                .map(|g| g.size)
                .unwrap_or((540, 1170).into())
        }

        fn idx_of(&self, s: &WlSurface) -> Option<usize> {
            self.wins.iter().position(|w| w.window.toplevel().is_some_and(|t| t.wl_surface() == s))
        }

        fn configure_all(&mut self) {
            let size = self.logical_size();
            for w in &self.wins {
                if let Some(t) = w.window.toplevel() {
                    t.with_pending_state(|s| {
                        s.size = Some(size);
                        s.states.set(xdg_toplevel::State::Fullscreen);
                    });
                    if t.is_initial_configure_sent() {
                        t.send_pending_configure();
                    }
                }
            }
            // layer surfaces follow the new output size
            if let Some(o) = &self.output {
                layer_map_for_output(o).arrange();
            }
        }

        fn set_keyboard_focus(&mut self, surface: Option<WlSurface>) {
            let kb = self.seat.get_keyboard().expect("keyboard");
            kb.set_focus(self, surface, SERIAL_COUNTER.next_serial());
        }

        fn show_shell(&mut self) {
            for i in 0..self.wins.len() {
                if !self.wins[i].shell && self.wins[i].mapped {
                    let w = self.wins[i].window.clone();
                    self.space.unmap_elem(&w);
                }
            }
            self.focus = Focus::Shell;
            let s = self.wins.iter().find(|w| w.shell && w.mapped).and_then(|w| w.window.toplevel().map(|t| t.wl_surface().clone()));
            self.set_keyboard_focus(s);
            self.needs_redraw = true;
        }

        fn show_app(&mut self, i: usize) {
            for j in 0..self.wins.len() {
                if j != i && !self.wins[j].shell && self.wins[j].mapped {
                    let w = self.wins[j].window.clone();
                    self.space.unmap_elem(&w);
                }
            }
            let w = self.wins[i].window.clone();
            self.space.map_element(w.clone(), (0, 0), true);
            self.focus = Focus::App(i);
            let s = w.toplevel().map(|t| t.wl_surface().clone());
            self.set_keyboard_focus(s);
            self.needs_redraw = true;
        }

        fn rotate(&mut self, deg: u32) {
            let t = match deg % 360 {
                90 => Transform::_90,
                180 => Transform::_180,
                270 => Transform::_270,
                _ => Transform::Normal,
            };
            let Some(o) = self.output.clone() else { return };
            o.change_current_state(None, Some(t), None, None);
            self.deg = deg % 360;
            let size = self.logical_size();
            self.recog.resize(size.w as f64, size.h as f64);
            self.configure_all();
            self.send_shell(&protocol::rotation_event(self.deg, size.w, size.h));
            self.needs_redraw = true;
            info!(deg = self.deg, w = size.w, h = size.h, "rotated");
        }

        fn send_shell(&mut self, line: &str) {
            if self.shell_sock.is_none() {
                self.shell_sock = UnixStream::connect(&self.shell_sock_path).ok();
            }
            if let Some(s) = self.shell_sock.as_mut() {
                if s.write_all(line.as_bytes()).and_then(|_| s.write_all(b"\n")).is_err() {
                    self.shell_sock = None; // reconnect on next event
                }
            }
        }

        fn spawn(&self, cmd: &str) {
            let mut c = std::process::Command::new("sh");
            c.arg("-c").arg(cmd);
            c.env("WAYLAND_DISPLAY", &self.socket_name).env_remove("DISPLAY").env_remove("SWAYSOCK");
            c.env("XDG_SESSION_TYPE", "wayland").env("XDG_CURRENT_DESKTOP", "FlowDE");
            c.env("FLOWDE_SHELL_SOCKET", &self.shell_sock_path);
            c.env("QT_QPA_PLATFORM", std::env::var("QT_QPA_PLATFORM").unwrap_or("wayland".into()));
            match c.spawn() {
                Ok(ch) => info!(cmd, pid = ch.id(), "spawned"),
                Err(e) => warn!(cmd, %e, "spawn failed"),
            }
        }

        // ---- touch pipeline: gesture recogniser first, then the focused client ------------------
        fn touch_down(&mut self, pos: Point<f64, Logical>, t: u32) {
            match self.recog.down(pos.x, pos.y, t) {
                Verdict::Pass => {
                    self.touch_to_client = true;
                    let focus = self.surface_under(pos);
                    let serial = SERIAL_COUNTER.next_serial();
                    let touch = self.touch.clone();
                    touch.down(self, focus, &DownEvent { slot: slot(), location: pos, serial, time: t });
                    touch.frame(self);
                }
                _ => self.touch_to_client = false,
            }
        }

        fn touch_motion(&mut self, pos: Point<f64, Logical>, t: u32) {
            if self.touch_to_client {
                let focus = self.surface_under(pos);
                let touch = self.touch.clone();
                touch.motion(self, focus, &MotionEvent { slot: slot(), location: pos, time: t });
                touch.frame(self);
            } else {
                self.recog.motion(pos.x, pos.y);
            }
        }

        fn touch_up(&mut self, pos: Point<f64, Logical>, t: u32) {
            if self.touch_to_client {
                let touch = self.touch.clone();
                touch.up(self, &UpEvent { slot: slot(), serial: SERIAL_COUNTER.next_serial(), time: t });
                touch.frame(self);
                self.touch_to_client = false;
            } else if let Verdict::Fire(g) = self.recog.up(pos.x, pos.y, t) {
                info!(gesture = g.name(), "edge gesture");
                match g {
                    flowde_compositor::gesture::Gesture::Home
                    | flowde_compositor::gesture::Gesture::Switcher
                    | flowde_compositor::gesture::Gesture::Shade
                    | flowde_compositor::gesture::Gesture::QuickSettings => self.show_shell(),
                    // Back: the shell decides (close shade/search/switcher). Back inside an app needs
                    // client cooperation (key injection/xdg-activation): TODO, see mobile-DECISIONS.md.
                    flowde_compositor::gesture::Gesture::Back => {}
                }
                self.send_shell(&protocol::gesture_event(g.name()));
            }
        }

        fn surface_under(&self, pos: Point<f64, Logical>) -> Option<(WlSurface, Point<f64, Logical>)> {
            let (window, loc) = self.space.element_under(pos)?;
            let (s, sloc) = window.surface_under(pos - loc.to_f64(), WindowSurfaceType::ALL)?;
            Some((s, (loc + sloc).to_f64()))
        }

        /// Map a point in the (physical) winit window to logical (rotated) coordinates.
        fn phys_to_logical(&self, nx: f64, ny: f64) -> Point<f64, Logical> {
            let l = self.logical_size();
            let t = self.output.as_ref().map(|o| o.current_transform()).unwrap_or(Transform::Normal);
            let (pw, ph) = if self.deg % 180 == 90 { (l.h as f64, l.w as f64) } else { (l.w as f64, l.h as f64) };
            let p: Point<f64, Logical> = (nx * pw, ny * ph).into();
            t.invert().transform_point_in(p, &Size::<f64, Logical>::from((pw, ph)))
        }
    }

    // ---- smithay handlers -------------------------------------------------------------------

    impl BufferHandler for State {
        fn buffer_destroyed(&mut self, _: &WlBuffer) {}
    }
    impl ShmHandler for State {
        fn shm_state(&self) -> &ShmState {
            &self.shm_state
        }
    }
    impl OutputHandler for State {}
    impl SelectionHandler for State {
        type SelectionUserData = ();
    }
    impl DataDeviceHandler for State {
        fn data_device_state(&self) -> &DataDeviceState {
            &self.data_device_state
        }
    }
    impl ClientDndGrabHandler for State {}
    impl ServerDndGrabHandler for State {
        fn send(&mut self, _: String, _: std::os::fd::OwnedFd, _: Seat<Self>) {}
    }

    impl SeatHandler for State {
        type KeyboardFocus = WlSurface;
        type PointerFocus = WlSurface;
        type TouchFocus = WlSurface;
        fn seat_state(&mut self) -> &mut SeatState<Self> {
            &mut self.seat_state
        }
        fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
            let client = focused.and_then(|s| self.dh.get_client(smithay::reexports::wayland_server::Resource::id(s)).ok());
            set_data_device_focus(&self.dh, seat, client);
        }
        fn cursor_image(&mut self, _: &Seat<Self>, _: smithay::input::pointer::CursorImageStatus) {}
    }

    impl CompositorHandler for State {
        fn compositor_state(&mut self) -> &mut CompositorState {
            &mut self.compositor_state
        }
        fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
            &client.get_data::<ClientState>().unwrap().compositor_state
        }
        fn commit(&mut self, surface: &WlSurface) {
            on_commit_buffer_handler::<Self>(surface);
            if !is_sync_subsurface(surface) {
                let mut root = surface.clone();
                while let Some(p) = get_parent(&root) {
                    root = p;
                }
                if let Some(i) = self.idx_of(&root) {
                    self.wins[i].window.on_commit();
                    if &root == surface {
                        self.toplevel_commit(i);
                    }
                }
            }
            self.popups.commit(surface);
            if let Some(PopupKind::Xdg(p)) = self.popups.find_popup(surface) {
                if !p.is_initial_configure_sent() {
                    let _ = p.send_configure();
                }
            }
            // layer surfaces
            if let Some(o) = self.output.clone() {
                let mut map = layer_map_for_output(&o);
                if let Some(l) = map.layer_for_surface(surface, WindowSurfaceType::TOPLEVEL).cloned() {
                    map.arrange();
                    let sent = with_states(surface, |s| {
                        s.data_map
                            .get::<smithay::wayland::shell::wlr_layer::LayerSurfaceData>()
                            .map(|d| d.lock().unwrap().initial_configure_sent)
                            .unwrap_or(true)
                    });
                    if !sent {
                        l.layer_surface().send_configure();
                    }
                }
            }
            self.needs_redraw = true;
        }
    }

    impl State {
        fn toplevel_commit(&mut self, i: usize) {
            let t = self.wins[i].window.toplevel().unwrap().clone();
            if !t.is_initial_configure_sent() {
                let size = self.logical_size();
                t.with_pending_state(|s| {
                    s.size = Some(size);
                    s.states.set(xdg_toplevel::State::Fullscreen);
                    s.states.set(xdg_toplevel::State::Activated);
                });
                t.send_configure();
                return;
            }
            let has_buffer = smithay::backend::renderer::utils::with_renderer_surface_state(t.wl_surface(), |s| s.buffer().is_some()).unwrap_or(false);
            if self.wins[i].mapped || !has_buffer {
                return;
            }
            let app_id = with_states(t.wl_surface(), |s| {
                s.data_map.get::<XdgToplevelSurfaceData>().and_then(|d| d.lock().unwrap().app_id.clone())
            });
            self.wins[i].shell = app_id.as_deref() == Some(SHELL_APP_ID);
            self.wins[i].mapped = true;
            info!(app_id = ?app_id, shell = self.wins[i].shell, "toplevel mapped");
            if self.wins[i].shell {
                let w = self.wins[i].window.clone();
                self.space.map_element(w, (0, 0), false);
                if self.focus == Focus::Shell {
                    self.show_shell();
                }
            } else {
                self.show_app(i);
            }
        }
    }

    impl XdgShellHandler for State {
        fn xdg_shell_state(&mut self) -> &mut XdgShellState {
            &mut self.xdg_shell_state
        }
        fn new_toplevel(&mut self, surface: ToplevelSurface) {
            self.wins.push(Win { window: Window::new_wayland_window(surface), shell: false, mapped: false });
        }
        fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
            if let Some(i) = self.idx_of(surface.wl_surface()) {
                let w = self.wins.remove(i);
                self.space.unmap_elem(&w.window);
                if let Focus::App(a) = self.focus {
                    if a == i {
                        self.show_shell();
                    } else if a > i {
                        self.focus = Focus::App(a - 1);
                    }
                }
                self.needs_redraw = true;
            }
        }
        fn new_popup(&mut self, surface: PopupSurface, _: PositionerState) {
            let _ = self.popups.track_popup(PopupKind::Xdg(surface));
        }
        fn reposition_request(&mut self, surface: PopupSurface, positioner: PositionerState, token: u32) {
            surface.with_pending_state(|s| {
                s.geometry = positioner.get_geometry();
                s.positioner = positioner;
            });
            surface.send_repositioned(token);
        }
        fn grab(&mut self, _: PopupSurface, _: wl_seat::WlSeat, _: Serial) {}
    }

    impl WlrLayerShellHandler for State {
        fn shell_state(&mut self) -> &mut WlrLayerShellState {
            &mut self.layer_shell_state
        }
        fn new_layer_surface(&mut self, surface: WlrLayerSurface, output: Option<WlOutput>, _layer: Layer, namespace: String) {
            let o = output.as_ref().and_then(Output::from_resource).or_else(|| self.output.clone());
            if let Some(o) = o {
                let _ = layer_map_for_output(&o).map_layer(&smithay::desktop::LayerSurface::new(surface, namespace));
            }
        }
        fn layer_destroyed(&mut self, surface: WlrLayerSurface) {
            if let Some(o) = self.output.clone() {
                let mut map = layer_map_for_output(&o);
                let l = map.layers().find(|l| l.layer_surface() == &surface).cloned();
                if let Some(l) = l {
                    map.unmap_layer(&l);
                }
            }
        }
    }

    delegate_compositor!(State);
    delegate_shm!(State);
    delegate_output!(State);
    delegate_seat!(State);
    delegate_data_device!(State);
    delegate_xdg_shell!(State);
    delegate_layer_shell!(State);

    // ---- input -------------------------------------------------------------------------------

    fn on_input<B: InputBackend>(state: &mut State, ev: InputEvent<B>) {
        match ev {
            InputEvent::Keyboard { event } => {
                let kb = state.seat.get_keyboard().expect("keyboard");
                kb.input::<(), _>(
                    state,
                    event.key_code(),
                    event.state(),
                    SERIAL_COUNTER.next_serial(),
                    event.time_msec(),
                    |_, _, _| FilterResult::Forward,
                );
            }
            // real touchscreens (nested: only if the parent forwards touch)
            InputEvent::TouchDown { event } => {
                let g = state.logical_size();
                let p = state.phys_to_logical(event.x(), event.y());
                let _ = g;
                state.touch_down(p, event.time_msec());
            }
            InputEvent::TouchMotion { event } => {
                let p = state.phys_to_logical(event.x(), event.y());
                state.touch_motion(p, event.time_msec());
            }
            InputEvent::TouchUp { event } => {
                let p = state.mouse_pos;
                state.touch_up(p, event.time_msec());
            }
            // mouse emulates one finger so the nested prototype is drivable with a pointer
            InputEvent::PointerMotionAbsolute { event } => {
                let p = state.phys_to_logical(event.x(), event.y());
                state.mouse_pos = p;
                if state.mouse_down {
                    state.touch_motion(p, event.time_msec());
                }
            }
            InputEvent::PointerButton { event } => {
                let t = event.time_msec();
                let p = state.mouse_pos;
                if event.state() == ButtonState::Pressed {
                    state.mouse_down = true;
                    state.touch_down(p, t);
                } else if state.mouse_down {
                    state.mouse_down = false;
                    state.touch_up(p, t);
                }
            }
            _ => {}
        }
        state.needs_redraw = true;
    }

    // ---- control socket (tests/tools) --------------------------------------------------------

    fn handle_cmd(state: &mut State, line: &str) -> String {
        let Some(v) = protocol::parse(line) else { return "{\"error\":\"bad json\"}".into() };
        let f = |k: &str| v[k].as_f64().unwrap_or(0.0);
        match v["cmd"].as_str().unwrap_or("") {
            "touch" => {
                let (p, t) = (Point::<f64, Logical>::from((f("x"), f("y"))), v["t"].as_u64().map(|t| t as u32).unwrap_or_else(|| state.now_ms()));
                match v["phase"].as_str() {
                    Some("down") => state.touch_down(p, t),
                    Some("move") => state.touch_motion(p, t),
                    Some("up") => state.touch_up(p, t),
                    _ => return "{\"error\":\"phase\"}".into(),
                }
                "{\"ok\":true}".into()
            }
            "swipe" => {
                let a = (v["from"][0].as_f64().unwrap_or(0.0), v["from"][1].as_f64().unwrap_or(0.0));
                let b = (v["to"][0].as_f64().unwrap_or(0.0), v["to"][1].as_f64().unwrap_or(0.0));
                let ms = v["ms"].as_u64().unwrap_or(200) as u32;
                let t0 = state.now_ms();
                state.touch_down(a.into(), t0);
                let n = 8u32;
                for k in 1..=n {
                    let q = k as f64 / n as f64;
                    state.touch_motion((a.0 + (b.0 - a.0) * q, a.1 + (b.1 - a.1) * q).into(), t0 + ms * k / n);
                }
                state.touch_up(b.into(), t0 + ms);
                state.needs_redraw = true;
                "{\"ok\":true}".into()
            }
            "rotate" => {
                state.rotate(v["deg"].as_u64().unwrap_or(0) as u32);
                "{\"ok\":true}".into()
            }
            "spawn" => {
                state.spawn(v["exec"].as_str().unwrap_or("true"));
                "{\"ok\":true}".into()
            }
            "state" => {
                let s = state.logical_size();
                serde_json::json!({"mode": if state.focus == Focus::Shell {"shell"} else {"app"},
                    "size":[s.w, s.h], "deg": state.deg, "windows": state.wins.len()}).to_string()
            }
            _ => "{\"error\":\"unknown cmd\"}".into(),
        }
    }

    struct Conn {
        s: UnixStream,
        buf: Vec<u8>,
    }
    impl std::os::fd::AsFd for Conn {
        fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
            self.s.as_fd()
        }
    }
    use std::os::fd::AsFd;

    fn start_control(handle: &LoopHandle<'static, State>, path: &PathBuf) -> io::Result<()> {
        let _ = std::fs::remove_file(path);
        let listener = UnixListener::bind(path)?;
        listener.set_nonblocking(true)?;
        let handle2 = handle.clone();
        handle
            .insert_source(Generic::new(listener, Interest::READ, LoopMode::Level), move |_, l, _state| {
                while let Ok((stream, _)) = l.accept() {
                    let _ = stream.set_nonblocking(true);
                    let _ = handle2.insert_source(
                        Generic::new(Conn { s: stream, buf: vec![] }, Interest::READ, LoopMode::Level),
                        move |_, c, state| {
                            use std::io::Read;
                            let conn = unsafe { c.get_mut() };
                            let mut tmp = [0u8; 4096];
                            loop {
                                match conn.s.read(&mut tmp) {
                                    Ok(0) => return Ok(PostAction::Remove),
                                    Ok(n) => conn.buf.extend_from_slice(&tmp[..n]),
                                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                                    Err(_) => return Ok(PostAction::Remove),
                                }
                            }
                            while let Some(p) = conn.buf.iter().position(|b| *b == b'\n') {
                                let line: Vec<u8> = conn.buf.drain(..=p).collect();
                                let reply = handle_cmd(state, &String::from_utf8_lossy(&line));
                                let _ = conn.s.write_all(reply.as_bytes());
                                let _ = conn.s.write_all(b"\n");
                            }
                            Ok(PostAction::Continue)
                        },
                    );
                }
                Ok(PostAction::Continue)
            })
            .map_err(|e| io::Error::other(e.to_string()))?;
        Ok(())
    }

    // ---- rendering ---------------------------------------------------------------------------

    fn render(state: &mut State) {
        if !state.needs_redraw {
            return;
        }
        let Some(output) = state.output.clone() else { return };
        let Some(mut data) = state.winit.take() else { return };
        let t0 = Instant::now();
        let age = data.backend.buffer_age().unwrap_or(0);
        let ok = match data.backend.bind() {
            Ok((renderer, mut fb)) => {
                let elements = space_render_elements::<_, Window, _>(renderer, [&state.space], &output, 1.0).unwrap_or_default();
                let _: &Vec<smithay::desktop::space::SpaceRenderElements<GlesRenderer, WaylandSurfaceRenderElement<GlesRenderer>>> = &elements;
                data.damage
                    .render_output(renderer, &mut fb, age, &elements, [0.04, 0.05, 0.08, 1.0])
                    .map(|r| r.damage.cloned())
                    .ok()
                    .flatten()
            }
            Err(_) => None,
        };
        if let Some(damage) = ok {
            let _ = data.backend.submit(Some(&damage));
            state.stats.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        state.winit = Some(data);
        state.needs_redraw = false;
        let now = state.start.elapsed();
        for w in state.space.elements() {
            w.send_frame(&output, now, Some(Duration::ZERO), |_, _| Some(output.clone()));
        }
        for l in layer_map_for_output(&output).layers() {
            l.send_frame(&output, now, Some(Duration::ZERO), |_, _| Some(output.clone()));
        }
        if state.last_stats.elapsed() >= Duration::from_secs(5) && !state.stats.is_empty() {
            let mut s = std::mem::take(&mut state.stats);
            s.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let avg = s.iter().sum::<f64>() / s.len() as f64;
            info!(frames = s.len(), avg_ms = format!("{avg:.2}"), p95_ms = format!("{:.2}", s[s.len() * 95 / 100]), max_ms = format!("{:.2}", s[s.len() - 1]), "compositor frame-time (render+submit)");
            state.last_stats = Instant::now();
        }
    }

    // ---- entry -------------------------------------------------------------------------------

    pub fn run() -> io::Result<()> {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
            .with_writer(std::io::stderr)
            .try_init();
        let mut spawn = vec![];
        let mut size = (540, 1170);
        let mut args = std::env::args().skip(1);
        while let Some(a) = args.next() {
            match a.as_str() {
                "--spawn" => spawn.push(args.next().unwrap_or_default()),
                "--size" => {
                    let v = args.next().unwrap_or_default();
                    if let Some((w, h)) = v.split_once('x') {
                        size = (w.parse().unwrap_or(540), h.parse().unwrap_or(1170));
                    }
                }
                "-h" | "--help" => {
                    println!("flowde-compositor [--size WxH] [--spawn CMD]...  (nested; needs a parent Wayland session)");
                    return Ok(());
                }
                o => return Err(io::Error::other(format!("unknown argument {o}"))),
            }
        }
        let rt = PathBuf::from(std::env::var("XDG_RUNTIME_DIR").map_err(|_| io::Error::other("XDG_RUNTIME_DIR not set"))?);
        let shell_sock_path = std::env::var("FLOWDE_SHELL_SOCKET").map(PathBuf::from).unwrap_or(rt.join("flowde-shell.sock"));
        let comp_sock_path = std::env::var("FLOWDE_COMPOSITOR_SOCKET").map(PathBuf::from).unwrap_or(rt.join("flowde-compositor.sock"));

        let mut event_loop: EventLoop<'static, State> = EventLoop::try_new().map_err(io::Error::from)?;
        let display: Display<State> = Display::new().map_err(io::Error::other)?;
        let dh = display.handle();
        event_loop
            .handle()
            .insert_source(Generic::new(display, Interest::READ, LoopMode::Level), |_, d, st: &mut State| {
                // SAFETY: display only touched here (anvil pattern)
                unsafe { d.get_mut().dispatch_clients(st).unwrap() };
                Ok(PostAction::Continue)
            })
            .map_err(|e| io::Error::other(e.to_string()))?;

        let mut seat_state = SeatState::new();
        let mut seat = seat_state.new_wl_seat(&dh, "seat0");
        seat.add_keyboard(XkbConfig::default(), 300, 30).map_err(|e| io::Error::other(format!("{e:?}")))?;
        let touch = seat.add_touch();

        let mut state = State {
            dh: dh.clone(),
            loop_signal: event_loop.get_signal(),
            socket_name: Default::default(),
            compositor_state: CompositorState::new::<State>(&dh),
            shm_state: ShmState::new::<State>(&dh, vec![]),
            xdg_shell_state: XdgShellState::new::<State>(&dh),
            layer_shell_state: WlrLayerShellState::new::<State>(&dh),
            data_device_state: DataDeviceState::new::<State>(&dh),
            output_manager_state: OutputManagerState::new_with_xdg_output::<State>(&dh),
            seat_state,
            seat,
            touch,
            popups: PopupManager::default(),
            space: Space::default(),
            output: None,
            wins: vec![],
            focus: Focus::Shell,
            recog: Recognizer::new(size.0 as f64, size.1 as f64),
            deg: 0,
            needs_redraw: true,
            shell_sock: None,
            shell_sock_path,
            compositor_sock_path: comp_sock_path.clone(),
            winit: None,
            touch_to_client: false,
            mouse_down: false,
            mouse_pos: (0.0, 0.0).into(),
            stats: vec![],
            last_stats: Instant::now(),
            start: Instant::now(),
        };
        let _ = &state.output_manager_state;
        let _ = &state.compositor_sock_path;

        // nested winit backend (reads the parent's WAYLAND_DISPLAY: create before our socket exists)
        let attrs = WinitWindow::default_attributes()
            .with_inner_size(LogicalSize::new(size.0 as f64, size.1 as f64))
            .with_title("FlowDE compositor (nested)");
        let (backend, winit_loop) = winit::init_from_attributes::<GlesRenderer>(attrs)
            .map_err(|e| io::Error::other(format!("winit/EGL init failed: {e}")))?;
        let wsize = backend.window_size();
        let mode = Mode { size: wsize, refresh: 60_000 };
        let output = Output::new(
            "FlowDE-nested-1".into(),
            PhysicalProperties { size: (0, 0).into(), subpixel: Subpixel::Unknown, make: "FlowDE".into(), model: "Nested".into() },
        );
        output.create_global::<State>(&dh);
        output.change_current_state(Some(mode), Some(Transform::Normal), None, Some((0, 0).into()));
        output.set_preferred(mode);
        state.space.map_output(&output, (0, 0));
        state.output = Some(output.clone());
        state.recog.resize(wsize.w as f64, wsize.h as f64);
        state.winit = Some(WinitData { backend, damage: OutputDamageTracker::from_output(&output) });
        info!(w = wsize.w, h = wsize.h, "nested output ready");

        event_loop
            .handle()
            .insert_source(winit_loop, |ev, _, st: &mut State| match ev {
                WinitEvent::Resized { size, .. } => {
                    if let Some(o) = st.output.clone() {
                        let m = Mode { size, refresh: 60_000 };
                        o.change_current_state(Some(m), None, None, None);
                        o.set_preferred(m);
                        let l = st.logical_size();
                        st.recog.resize(l.w as f64, l.h as f64);
                        st.configure_all();
                        st.needs_redraw = true;
                    }
                }
                WinitEvent::Input(e) => on_input(st, e),
                WinitEvent::Redraw => st.needs_redraw = true,
                WinitEvent::CloseRequested => st.loop_signal.stop(),
                WinitEvent::Focus(_) => {}
            })
            .map_err(|e| io::Error::other(e.to_string()))?;
        event_loop
            .handle()
            .insert_source(Timer::from_duration(Duration::from_millis(16)), |_, _, st| {
                render(st);
                TimeoutAction::ToDuration(Duration::from_millis(16))
            })
            .map_err(|e| io::Error::other(e.to_string()))?;

        let socket = ListeningSocketSource::new_auto().map_err(io::Error::other)?;
        state.socket_name = socket.socket_name().to_os_string();
        event_loop
            .handle()
            .insert_source(socket, |stream, _, st: &mut State| {
                if let Err(e) = st.dh.insert_client(stream, Arc::new(ClientState::default())) {
                    warn!(%e, "client rejected");
                }
            })
            .map_err(|e| io::Error::other(e.to_string()))?;
        info!(socket = ?state.socket_name, "wayland socket");
        start_control(&event_loop.handle(), &comp_sock_path)?;
        info!(path = %comp_sock_path.display(), "control socket");

        for c in spawn {
            state.spawn(&c);
        }
        event_loop
            .run(Some(Duration::from_millis(16)), &mut state, |st| {
                let _ = st.dh.flush_clients();
            })
            .map_err(io::Error::from)?;
        let _ = std::fs::remove_file(&comp_sock_path);
        Ok(())
    }
}
