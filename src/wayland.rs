use std::{collections::HashMap, ptr::NonNull};

use smithay_client_toolkit::{compositor::{CompositorHandler, CompositorState}, delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_seat, output::{OutputHandler, OutputState}, registry::{ProvidesRegistryState, RegistryState}, registry_handlers, seat::{SeatHandler, SeatState}, shell::{wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface}, WaylandSurface}};
use wayland_client::{globals::registry_queue_init, protocol::{wl_output::WlOutput, wl_surface::WlSurface}, Connection, Proxy, QueueHandle};
use wgpu::rwh::{RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle};

use crate::{configuration::{Configuration, MonitorConfig}, renderer::Renderer};

pub struct WaylandState {
    close: bool,
    started_drawing: bool,
    targets: HashMap<String, OutputTarget>, // output name -> output target struct
    config: Option<Configuration>,

    conn: Connection,
    compositor: CompositorState,
    layer_shell: LayerShell,

    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
}
struct OutputTarget {
    output: WlOutput,
    layer: LayerSurface,
    surface: WlSurface,
    renderer: Option<Renderer>,
    configured: bool
}

impl CompositorHandler for WaylandState {
    fn frame(&mut self, _conn: &wayland_client::Connection, qh: &wayland_client::QueueHandle<Self>, _surface: &wayland_client::protocol::wl_surface::WlSurface, _time: u32) {
        self.draw(qh);
    }

    fn scale_factor_changed(&mut self, _conn: &wayland_client::Connection, _qh: &wayland_client::QueueHandle<Self>, _surface: &wayland_client::protocol::wl_surface::WlSurface, _new_factor: i32) {}
    fn transform_changed(&mut self, _conn: &wayland_client::Connection, _qh: &wayland_client::QueueHandle<Self>, _surface: &wayland_client::protocol::wl_surface::WlSurface, _new_transform: wayland_client::protocol::wl_output::Transform) {}
    fn surface_enter(&mut self, _conn: &wayland_client::Connection, _qh: &wayland_client::QueueHandle<Self>, _surface: &wayland_client::protocol::wl_surface::WlSurface, _output: &wayland_client::protocol::wl_output::WlOutput) {}
    fn surface_leave(&mut self, _conn: &wayland_client::Connection, _qh: &wayland_client::QueueHandle<Self>, _surface: &wayland_client::protocol::wl_surface::WlSurface, _output: &wayland_client::protocol::wl_output::WlOutput) {}
}

impl OutputHandler for WaylandState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _conn: &wayland_client::Connection, qh: &QueueHandle<Self>, output: WlOutput) {
        let output_info = match self.output_state.info(&output) {
            Some(r) => r,
            None => return, // don't bother with it
        };

        let name = match output_info.name {
            Some(r) => r,
            None => return,
        };

        if let Some(config) = &self.config {
            let monitor_config = config.monitor_config(&name);
            if monitor_config.is_none() {
                println!("output {name} skipped as it's not defined in the config.");
                return;
            }
        }

        let width: u32 = output_info.modes[0].dimensions.0 as u32;
        let height: u32 = output_info.modes[0].dimensions.1 as u32;
        let surface = self.compositor.create_surface(qh);
        let layer = self.layer_shell.create_layer_surface(qh, surface.clone(), Layer::Background, Some(format!("gpuburden-{name}")), Some(&output));
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_size(width, height);
        layer.set_anchor(Anchor::LEFT | Anchor::TOP);
        layer.commit();

        let target = OutputTarget {
            output,
            layer,
            surface,
            renderer: None,
            configured: false
        };
        println!("new output {name}");
        self.targets.insert(name, target);
    }
    fn update_output(&mut self, _conn: &wayland_client::Connection, _qh: &QueueHandle<Self>, _output: wayland_client::protocol::wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _conn: &wayland_client::Connection, _qh: &QueueHandle<Self>, _output: wayland_client::protocol::wl_output::WlOutput) {}
}

impl LayerShellHandler for WaylandState {
    fn closed(&mut self, _conn: &wayland_client::Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        self.close = true;
    }

    fn configure(&mut self, _conn: &wayland_client::Connection, qh: &QueueHandle<Self>, layer: &LayerSurface, configure: smithay_client_toolkit::shell::wlr_layer::LayerSurfaceConfigure, _serial: u32) {
        for target in &mut self.targets {
            if target.1.layer != *layer {
                continue
            }

            let target = target.1;
            let info = self.output_state.info(&target.output).expect("Failed to get info for display.");
            let name = match info.name {
                Some(r) => r,
                None => return,
            };

            let mut width = configure.new_size.0;
            let mut height = configure.new_size.1;
            match info.transform {
                wayland_client::protocol::wl_output::Transform::_90 | wayland_client::protocol::wl_output::Transform::_270 |
                wayland_client::protocol::wl_output::Transform::Flipped90 | wayland_client::protocol::wl_output::Transform::Flipped270 => {
                    width = configure.new_size.1;
                    height = configure.new_size.0;
                },
                _ => {}
            }

            if target.configured {
                println!("configure called on already-configed monitor {name}");
                return;
            }

            // setup renderer
            let raw_display_handle = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
                    NonNull::new(self.conn.backend().display_ptr() as *mut _).expect("Failed to create display handle for wgpu.")
            ));
            let raw_window_handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(
                    NonNull::new(target.surface.id().as_ptr() as *mut _).expect("Failed to create window handle for wgpu.")
            ));
            let config: Option<MonitorConfig> = if let Some(config) = &self.config {
                config.monitor_config(&name)
            } else {
                None
            };
            let mut renderer = Renderer::for_layer(raw_display_handle, raw_window_handle, &config);
            renderer.configure_surface(width, height);
            target.renderer = Some(renderer);
            target.configured = true;
            println!("{name} configured for {width}x{height}");
        }
        if !self.started_drawing {
            self.draw(qh);
            self.started_drawing = true;
        }
    }
}

impl SeatHandler for WaylandState {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_capability(&mut self, _conn: &wayland_client::Connection, _qh: &QueueHandle<Self>, _seat: wayland_client::protocol::wl_seat::WlSeat, _capability: smithay_client_toolkit::seat::Capability) {}
    fn remove_capability(&mut self, _conn: &wayland_client::Connection, _qh: &QueueHandle<Self>, _seat: wayland_client::protocol::wl_seat::WlSeat, _capability: smithay_client_toolkit::seat::Capability) {}
    fn new_seat(&mut self, _conn: &wayland_client::Connection, _qh: &QueueHandle<Self>, _seat: wayland_client::protocol::wl_seat::WlSeat) {}
    fn remove_seat(&mut self, _conn: &wayland_client::Connection, _qh: &QueueHandle<Self>, _seat: wayland_client::protocol::wl_seat::WlSeat) {}
}

impl ProvidesRegistryState for WaylandState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

impl WaylandState {
    pub fn draw(&mut self, qh: &QueueHandle<Self>) {
        for render_target in &mut self.targets {
            let target = render_target.1;
            if let Some(renderer) = &mut target.renderer {
                renderer.draw();
                // target.layer.wl_surface().damage_buffer(0, 0, renderer.width as i32, renderer.height as i32);
            }
            target.layer.wl_surface().frame(qh, target.layer.wl_surface().clone());
            target.layer.commit();
        }
    }
}

delegate_compositor!(WaylandState);
delegate_output!(WaylandState);
delegate_seat!(WaylandState);
delegate_layer!(WaylandState);
delegate_registry!(WaylandState);

pub fn start(config: Option<Configuration>) {
    let conn = Connection::connect_to_env().expect("Unable to connect to a compositor.");
    let (globals, mut event_queue) = registry_queue_init(&conn).unwrap();
    let qh = event_queue.handle();

    let compositor = CompositorState::bind(&globals, &qh).expect("Compositor does not support 'wl_compositor'");
    let layer_shell = LayerShell::bind(&globals, &qh).expect("Compositor does not support 'zwlr_layer_shell_v1'");

    let mut state = WaylandState {
        close: false,
        started_drawing: false,
        targets: HashMap::new(),
        config,

        conn,
        compositor,
        layer_shell,

        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
    };
    loop {
        event_queue.blocking_dispatch(&mut state).unwrap();

        if state.close {
            break;
        }
    }

    for render_target in &mut state.targets {
        let target = render_target.1;
        if let Some(renderer) = target.renderer.take() {
            renderer.free_surface();
        }
    }
}
