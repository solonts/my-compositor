#![allow(unused_variables)]
use std::{sync::Arc, time::Duration};
use smithay::{
    delegate_compositor, delegate_xdg_shell,
    desktop::{Space, Window},
    reexports::{
        // calloop::{EventLoop, timer::{Timer, TimeoutAction}, LoopSignal},
        calloop::{generic::Generic, EventLoop, Interest, Mode, PostAction},
        wayland_server::{
            backend::{ClientData, ClientId, DisconnectReason},
            protocol::{wl_surface::WlSurface, wl_seat::WlSeat},
            Client, Display, DisplayHandle,
        },
    },
    backend::input::{KeyState},
    input::{
        Seat, SeatHandler, SeatState,
        keyboard::{KeyboardTarget, KeysymHandle, ModifiersState},
        pointer::{
            PointerTarget, MotionEvent, RelativeMotionEvent, ButtonEvent, AxisFrame,
            GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent, GesturePinchUpdateEvent,
            GesturePinchEndEvent, GestureSwipeBeginEvent, GestureSwipeUpdateEvent, GestureSwipeEndEvent,
        },
        touch::{
            TouchTarget, DownEvent, UpEvent, MotionEvent as TouchMotionEvent, 
            ShapeEvent, OrientationEvent,
        },
    },
    utils::{IsAlive, Serial},
    wayland::{
        compositor::{CompositorClientState, CompositorHandler, CompositorState},
        shell::xdg::{
            PopupSurface, PositionerState, ToplevelSurface, 
            XdgShellHandler, XdgShellState,
        },
        socket::ListeningSocketSource,
    },
};

#[derive(Default)]
struct ClientState {
    compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

#[derive(Debug, PartialEq, Clone)]
enum KeyboardFocusTarget {
    Window(Window)
}

impl IsAlive for KeyboardFocusTarget {
    fn alive(&self) -> bool {
        match self {
            KeyboardFocusTarget::Window(w) => w.alive(),
        }
    }
}


impl KeyboardTarget<State> for KeyboardFocusTarget {
    fn enter(&self, seat: &Seat<State>, data: &mut State, keys: Vec<KeysymHandle<'_>>, serial: Serial) {}

    fn leave(&self, seat: &Seat<State>, data: &mut State, serial: Serial) {}

    fn key(&self, seat: &Seat<State>, data: &mut State, key: KeysymHandle<'_>, state: KeyState, serial: Serial, time: u32) {}

    fn modifiers(&self, seat: &Seat<State>, data: &mut State, modifiers: ModifiersState, serial: Serial) {}
}

#[derive(Debug, PartialEq, Clone)]
enum PointerFocusTarget {
    WlSurface(WlSurface)
}

impl IsAlive for PointerFocusTarget {
    fn alive(&self) -> bool {
        match self {
            PointerFocusTarget::WlSurface(w) => w.alive(),
        }
    }
}

impl PointerTarget<State> for PointerFocusTarget {
    fn enter(&self, seat: &Seat<State>, data: &mut State, event: &MotionEvent) {}

    fn motion(&self, seat: &Seat<State>, data: &mut State, event: &MotionEvent) {}

    fn relative_motion(&self, seat: &Seat<State>, data: &mut State, event: &RelativeMotionEvent) {}

    fn button(&self, seat: &Seat<State>, data: &mut State, event: &ButtonEvent) {}

    fn axis(&self, seat: &Seat<State>, data: &mut State, frame: AxisFrame) {}

    fn frame(&self, seat: &Seat<State>, data: &mut State) {}

    fn leave(&self, seat: &Seat<State>, data: &mut State, serial: Serial, time: u32) {}

    fn gesture_swipe_begin(&self, seat: &Seat<State>, data: &mut State, event: &GestureSwipeBeginEvent) {}

    fn gesture_swipe_update(&self, seat: &Seat<State>, data: &mut State, event: &GestureSwipeUpdateEvent) {}

    fn gesture_swipe_end(&self, seat: &Seat<State>, data: &mut State, event: &GestureSwipeEndEvent) {}

    fn gesture_pinch_begin(&self, seat: &Seat<State>, data: &mut State, event: &GesturePinchBeginEvent) {}

    fn gesture_pinch_update(&self, seat: &Seat<State>, data: &mut State, event: &GesturePinchUpdateEvent) {}

    fn gesture_pinch_end(&self, seat: &Seat<State>, data: &mut State, event: &GesturePinchEndEvent) {}

    fn gesture_hold_begin(&self, seat: &Seat<State>, data: &mut State, event: &GestureHoldBeginEvent) {}

    fn gesture_hold_end(&self, seat: &Seat<State>, data: &mut State, event: &GestureHoldEndEvent) {}
}

impl TouchTarget<State> for PointerFocusTarget {
    fn down(&self, seat: &Seat<State>, data: &mut State, event: &DownEvent, seq: Serial) {}

    fn up(&self, seat: &Seat<State>, data: &mut State, event: &UpEvent, seq: Serial) {}

    fn motion(&self, seat: &Seat<State>, data: &mut State, event: &TouchMotionEvent, seq: Serial) {}

    fn frame(&self, seat: &Seat<State>, data: &mut State, seq: Serial) {}

    fn cancel(&self, seat: &Seat<State>, data: &mut State, seq: Serial) {}

    fn shape(&self, seat: &Seat<State>, data: &mut State, event: &ShapeEvent, seq: Serial) {}

    fn orientation(&self, seat: &Seat<State>, data: &mut State, event: &OrientationEvent, seq: Serial) {}
}


struct State {
    display_handle: DisplayHandle,

    compositor_state: CompositorState,
    xdg_shell_state: XdgShellState,
    seat_state: SeatState<Self>,

    space: Space<Window>,
}

impl CompositorHandler for State {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client.get_data::<ClientState>().unwrap().compositor_state
    }

    fn commit(&mut self, _surface: &WlSurface) {}
}

impl SeatHandler for State {
    type KeyboardFocus = KeyboardFocusTarget;
    type PointerFocus = PointerFocusTarget;
    type TouchFocus = PointerFocusTarget;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seat_state
    }
}

delegate_compositor!(State);

impl XdgShellHandler for State {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let window = Window::new_wayland_window(surface);
        self.space.map_element(window, (0, 0), false);
    }

    fn new_popup(&mut self, _surface: PopupSurface, _positioner: PositionerState) {}

    fn grab(&mut self, _surface: PopupSurface, _seat: WlSeat, _serial: Serial) {}

    fn reposition_request(&mut self, _surface: PopupSurface, _positioner: PositionerState, _token: u32) {}
}
delegate_xdg_shell!(State);

fn main() {
    init_logging();

    let mut event_loop: EventLoop<State> = EventLoop::try_new()
        .expect("Failed to create the event loop!");

    let display: Display<State> = Display::new()
        .expect("Failed to create the wayland display!");
    let mut display_handle = display.handle();

    let mut state = State {
        display_handle: display_handle.clone(),
        compositor_state: CompositorState::new::<State>(&display_handle),
        xdg_shell_state: XdgShellState::new::<State>(&display_handle),
        seat_state: SeatState::<State>::new(),
        space: Space::default(),
    };

    let listening_socket = ListeningSocketSource::new_auto()
        .expect("Failed to bind the wayland socket!");
    let socket_name = listening_socket.socket_name().to_string_lossy().into_owned();


    // std::env::set_var("WAYLAND_DISPLAY", &socket_name);
    tracing::info!(?socket_name, "compositor listening");

    event_loop.handle()
        .insert_source(listening_socket, |client_stream, _, state: &mut State| {
            state.display_handle
                .insert_client(client_stream, Arc::new(ClientState::default()))
                .expect("Failed to register new client!");
        })
        .expect("Failed to init the wayland listener source!");

    event_loop.handle()
        .insert_source(
            Generic::new(display, Interest::READ, Mode::Level),
            |_, display, state: &mut State| {
                unsafe {
                    display.get_mut().dispatch_clients(state).unwrap();
                }
                Ok(PostAction::Continue)
            },
        )
        .expect("Failed to init the wayland display source");

    loop {
        event_loop
            .dispatch(Some(Duration::from_millis(16)), &mut state)
            .expect("Event loop error");
        state.display_handle.flush_clients().unwrap();
    }
}

fn init_logging() {
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    } else {
        tracing_subscriber::fmt().compact().init();
    }
}

/*
fn main() {
    init_logging();

    let mut event_loop: EventLoop<LoopSignal> = EventLoop::try_new()
        .expect("Failed to create the event loop!");

    let handle = event_loop.handle();

    let source = Timer::from_duration(std::time::Duration::from_secs(2));

    handle
        .insert_source(
            source,
            |event, _metadata, shared_data| {
                println!("Timeout for {:?} expired!", event);
                shared_data.stop();
                TimeoutAction::Drop
            }
        )
        .expect("Failed to insert event source!");

    let mut shared_data = event_loop.get_signal();

    event_loop
        .run(
            std::time::Duration::from_millis(20),
            &mut shared_data,
            |_shared_data| {
                println!("Something happened");
            }
        )
        .expect("Error during event loop!");

    /*
    let display: Display<State> = Display::new()
        .expect("Failed to create the wayland display!");
    let mut display_handle = display.handle();

    let mut state = State {
        display_handle: display_handle.clone(),
        compositor_state: CompositorState::new::<State>(&display_handle),
        xdg_shell_state: XdgShellState::new::<State>(&display_handle),
        seat_state: SeatState::<State>::new(),
        space: Space::default(),
    };

    let listening_socket = ListeningSocket::bind_auto("wayland", 1..33)
        .expect("Failed to bind the wayland socket!");
    let socket_name = listening_socket.socket_name().expect("To work").to_os_string();


    std::env::set_var("WAYLAND_DISPLAY", &socket_name);
    tracing::info!(?socket_name, "compositor listening");

    event_loop.handle()
        .insert_source(listening_socket, |client_stream, _, state: &mut State| {
            state.display_handle
                .insert_client(client_stream, Arc::new(ClientState::default()))
                .expect("Failed to register new client!");
        })
        .expect("Failed to init the wayland listener source!");

    event_loop.handle()
        .insert_source(
            Generic::new(display, Interest::READ, Mode::Level),
            |_, display, state: &mut State| {
                unsafe {
                    display.get_mut().dispatch_clients(state).unwrap();
                }
                Ok(PostAction::Continue)
            },
        )
        .expect("Failed to init the wayland display source");

    loop {
        event_loop
            .dispatch(Some(Duration::from_millis(16)), &mut state)
            .expect("Event loop error");
        state.display_handle.flush_clients().unwrap();
    }

    */
}

fn init_logging() {
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    } else {
        tracing_subscriber::fmt().compact().init();
    }
}
*/
