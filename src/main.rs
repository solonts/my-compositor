#![allow(unused_variables)]
use std::{sync::Arc, time::Duration, ffi::OsString};
use smithay::{
    delegate_compositor, delegate_xdg_shell, delegate_shm, delegate_output,
    delegate_data_device, delegate_seat,
    desktop::{Space, Window, space, PopupManager, PopupKind, WindowSurfaceType},
    reexports::{
        calloop::{generic::Generic, EventLoop, Interest, Mode, PostAction},
        wayland_server::{
            backend::{ClientData, ClientId, DisconnectReason},
            protocol::{wl_surface::WlSurface, wl_seat::WlSeat, wl_buffer::WlBuffer},
            Client, Display, DisplayHandle, Resource,
        },
    },
    backend::{
        input::{
            InputBackend, InputEvent, KeyboardKeyEvent, Event, AbsolutePositionEvent,
        },
        renderer::{
            damage::OutputDamageTracker, gles::GlesRenderer, utils,
            element::surface::WaylandSurfaceRenderElement, 
        },
        winit::{self, WinitEvent},
    },
    input::{
        Seat, SeatHandler, SeatState,
        keyboard::{FilterResult, XkbConfig},
        pointer::{MotionEvent},
    },
    output::{Mode as OutputMode, Output, PhysicalProperties, Subpixel},
    utils::{IsAlive, Serial, Rectangle, Transform, SERIAL_COUNTER, Logical, Point},
    wayland::{
        buffer::BufferHandler,
        compositor::{
            CompositorClientState, CompositorHandler, CompositorState,
            get_parent, is_sync_subsurface, with_states,
        },
        shell::xdg::{
            PopupSurface, PositionerState, ToplevelSurface, 
            XdgShellHandler, XdgShellState, XdgToplevelSurfaceData, 
        },
        shm::{ShmHandler, ShmState},
        socket::ListeningSocketSource,
        output::OutputHandler,
        selection::{
            SelectionHandler,
            data_device::{
                ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
            },
        },
    },
};

#[derive(Default)]
struct ClientState {
    compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

struct State {
    start_time: std::time::Instant,
    socket_name: OsString,
    display_handle: DisplayHandle,

    compositor_state: CompositorState,
    xdg_shell_state: XdgShellState,
    shm_state: ShmState,
    seat_state: SeatState<Self>,
    data_device_state: DataDeviceState,
    popups: PopupManager,

    space: Space<Window>,
    seat: Seat<Self>,
}

impl CompositorHandler for State {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client.get_data::<ClientState>().unwrap().compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        utils::on_commit_buffer_handler::<State>(surface);
        tracing::debug!(sureface = ?surface.id(), "Commit");
        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            if let Some(window) = self
                .space
                .elements()
                .find(|w| w.toplevel().unwrap().wl_surface() == &root)
            {
                window.on_commit();
            }
        };

        handle_commit(&mut self.popups, &self.space, surface);
    }
}

delegate_compositor!(State);

impl SeatHandler for State {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seat_state
    }
}

delegate_seat!(State);

impl XdgShellHandler for State {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        tracing::info!("new toplevel: {:?}", surface.wl_surface().id());
        let window = Window::new_wayland_window(surface.clone());
        self.space.map_element(window, (0, 0), false);
        surface.with_pending_state(|state| {
            state.size = Some((800, 600).into());
        });
        surface.send_configure();

        let keyboard = self.seat.get_keyboard().unwrap();
        let serial = SERIAL_COUNTER.next_serial();
        keyboard.set_focus(self, Some(surface.wl_surface().clone()), serial);
        tracing::debug!(surface = ?surface.wl_surface().id(), "keyboard focus set");
    }

    fn new_popup(&mut self, _surface: PopupSurface, _positioner: PositionerState) {}

    fn grab(&mut self, _surface: PopupSurface, _seat: WlSeat, _serial: Serial) {}

    fn reposition_request(&mut self, _surface: PopupSurface, _positioner: PositionerState, _token: u32) {}
}

delegate_xdg_shell!(State);

impl ShmHandler for State {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

impl BufferHandler for State {
    fn buffer_destroyed(&mut self, buffer: &WlBuffer) {}
}

delegate_shm!(State);



impl SelectionHandler for State {
    type SelectionUserData = ();
}

impl DataDeviceHandler for State {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

impl ClientDndGrabHandler for State {}
impl ServerDndGrabHandler for State {}

delegate_data_device!(State);

impl OutputHandler for State {}
delegate_output!(State);

fn handle_commit(popups: &mut PopupManager, space: &Space<Window>, surface: &WlSurface) {
    if let Some(window) = space
        .elements()
        .find(|w| w.toplevel().unwrap().wl_surface() == surface)
        .cloned()
    {
        let initial_configure_sent = with_states(surface, |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .unwrap()
                .lock()
                .unwrap()
                .initial_configure_sent
        });

        if !initial_configure_sent {
            window.toplevel().unwrap().send_configure();
        }
    }

    popups.commit(surface);
    if let Some(popup) = popups.find_popup(surface) {
        match popup {
            PopupKind::Xdg(ref xdg) => {
                if !xdg.is_initial_configure_sent() {
                    xdg.send_configure().expect("initial configure failed");
                }
            }
            PopupKind::InputMethod(ref _input_method) => {}
        }
    }
}

impl State {
    fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time = Event::time_msec(&event);

                self.seat.get_keyboard().unwrap().input::<(), _>(
                    self,
                    event.key_code(),
                    event.state(),
                    serial,
                    time,
                    |_, _, _| {
                        tracing::debug!("Keyboard input reached filter");
                        FilterResult::Forward
                    }
                );
            }
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let output = self.space.outputs().next().unwrap();

                let output_geo = self.space.output_geometry(output).unwrap();

                let pos = event.position_transformed(output_geo.size) + output_geo.loc.to_f64();

                let serial = SERIAL_COUNTER.next_serial();

                let pointer = self.seat.get_pointer().unwrap();

                let under = self.surface_under(pos);

                pointer.motion(
                    self,
                    under,
                    &MotionEvent {
                        location: pos,
                        serial,
                        time: event.time_msec(),
                    },
                );
                pointer.frame(self);
            }
            _ => {}
        }
    }

    fn surface_under(&self, pos: Point<f64, Logical>) -> Option<(WlSurface, Point<f64, Logical>)> {
        self.space.element_under(pos).and_then(|(window, location)| {
            window
                .surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
                .map(|(s, p)| (s, (p + location).to_f64()))
        })
    }
}

fn init_winit(
    event_loop: &mut EventLoop<State>, 
    state: &mut State,
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut backend, winit) = winit::init()?;

    let mode = OutputMode {
        size: backend.window_size(),
        refresh: 60_000,
    };

    let output = Output::new(
        "winit".to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "Smithay".into(),
            model: "Winit".into(),
            // serial_number: "Unknown".into(),
        }
    );

    let _global = output.create_global::<State>(&state.display_handle);
    output.change_current_state(Some(mode), Some(Transform::Flipped180), None, Some((0, 0).into()));
    output.set_preferred(mode);

    state.space.map_output(&output, (0, 0));

    let mut damage_tracker = OutputDamageTracker::from_output(&output);

    unsafe{ std::env::set_var("WAYLAND_DISPLAY", &state.socket_name.clone()) };

    event_loop.handle().insert_source(winit, move |event, _, state| {

        match event {
            WinitEvent::Input(event) => state.process_input_event(event),
            WinitEvent::Redraw => {
                let size = backend.window_size();
                let damage = Rectangle::from_size(size);

                {
                    let (renderer, mut framebuffer) = backend.bind().unwrap();
                    // tracing::debug!("{:?}", state.space.elements());
                    space::render_output::<
                        _, WaylandSurfaceRenderElement<GlesRenderer>, _, _
                    >(
                        &output,
                        renderer, 
                        &mut framebuffer,
                        1.0,
                        0,
                        [&state.space], 
                        &[], 
                        &mut damage_tracker, 
                        [0.1, 0.1, 0.1, 1.0],
                    ).unwrap();
                }
                backend.submit(Some(&[damage])).unwrap();

                state.space.elements().for_each(|window| {
                    window.send_frame(
                        &output,
                        state.start_time.elapsed(),
                        Some(Duration::ZERO),
                        |_, _| Some(output.clone()),
                    )
                });

                state.space.refresh();
                state.popups.cleanup();
                let _ = state.display_handle.flush_clients();

                backend.window().request_redraw();
            }
            _ => (),
        };
    })?;

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();

    let mut event_loop: EventLoop<State> = EventLoop::try_new()
        .expect("Failed to create the event loop!");

    let display: Display<State> = Display::new()
        .expect("Failed to create the wayland display!");
    let display_handle = display.handle();

    let listening_socket = ListeningSocketSource::new_auto()
        .expect("Failed to bind the wayland socket!");
    let socket_name = listening_socket.socket_name().to_string_lossy().into_owned();


    let mut seat_state = SeatState::<State>::new();
    let mut seat: Seat<State> = seat_state.new_wl_seat(&display_handle, "winit");

    seat.add_keyboard(XkbConfig::default(), 200, 25).unwrap();
    seat.add_pointer();
        
    let mut state = State {
        start_time: std::time::Instant::now(),
        socket_name: socket_name.clone().into(),
        display_handle: display_handle.clone(),
        compositor_state: CompositorState::new::<State>(&display_handle),
        xdg_shell_state: XdgShellState::new::<State>(&display_handle),
        seat_state,
        shm_state: ShmState::new::<State>(&display_handle, Vec::new()),
        data_device_state: DataDeviceState::new::<State>(&display_handle),
        popups: PopupManager::default(),
        space: Space::default(),
        seat
    };

    init_winit(&mut event_loop, &mut state)?;


    unsafe { std::env::set_var("WAYLAND_DISPLAY", &socket_name) };
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
