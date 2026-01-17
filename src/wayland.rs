use std::time::Instant;

use noise::{NoiseFn, Perlin};
use smithay_client_toolkit::{compositor::{CompositorHandler, CompositorState}, delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_seat, delegate_shm, output::{OutputHandler, OutputState}, registry::{ProvidesRegistryState, RegistryState}, registry_handlers, seat::{SeatHandler, SeatState}, shell::{wlr_layer::{KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface}, WaylandSurface}, shm::{slot::SlotPool, Shm, ShmHandler}};
use wayland_client::{globals::registry_queue_init, protocol::wl_shm, Connection, QueueHandle};

use crate::{color::Color, color_ramp::ColorRamp};

pub struct WaylandState {
    width: u32,
    height: u32,
    close: bool,
    shm: Shm,
    pool: SlotPool,
    layer: LayerSurface,
    noise: Perlin,
    noise_z: f64,
    last_frame_time: Instant,
    color_ramp: ColorRamp,

    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
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

    fn new_output(&mut self, _conn: &wayland_client::Connection, _qh: &QueueHandle<Self>, _output: wayland_client::protocol::wl_output::WlOutput) {}
    fn update_output(&mut self, _conn: &wayland_client::Connection, _qh: &QueueHandle<Self>, _output: wayland_client::protocol::wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _conn: &wayland_client::Connection, _qh: &QueueHandle<Self>, _output: wayland_client::protocol::wl_output::WlOutput) {}
}

impl LayerShellHandler for WaylandState {
    fn closed(&mut self, _conn: &wayland_client::Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        self.close = true;
    }

    fn configure(&mut self, _conn: &wayland_client::Connection, qh: &QueueHandle<Self>, _layer: &LayerSurface, configure: smithay_client_toolkit::shell::wlr_layer::LayerSurfaceConfigure, _serial: u32) {
        self.width = configure.new_size.0;
        self.height = configure.new_size.1;
        self.draw(qh);
    }
}

impl SeatHandler for WaylandState {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_capability(&mut self, _conn: &wayland_client::Connection, qh: &QueueHandle<Self>, seat: wayland_client::protocol::wl_seat::WlSeat, capability: smithay_client_toolkit::seat::Capability) {}
    fn remove_capability(&mut self, _conn: &wayland_client::Connection, _qh: &QueueHandle<Self>, _seat: wayland_client::protocol::wl_seat::WlSeat, capability: smithay_client_toolkit::seat::Capability) {}
    fn new_seat(&mut self, _conn: &wayland_client::Connection, _qh: &QueueHandle<Self>, _seat: wayland_client::protocol::wl_seat::WlSeat) {}
    fn remove_seat(&mut self, _conn: &wayland_client::Connection, _qh: &QueueHandle<Self>, _seat: wayland_client::protocol::wl_seat::WlSeat) {}
}

impl ShmHandler for WaylandState {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for WaylandState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

impl WaylandState {
    pub fn draw(&mut self, qh: &QueueHandle<Self>) {
        let width = self.width;
        let height = self.height;
        let width_int = i32::try_from(width).expect("selected height to i32 failed");
        let height_int = i32::try_from(height).expect("height to i32 failed");
        let stride = width_int * 4;

        let (buffer, canvas) = self.pool.create_buffer(width_int, height_int, stride, wl_shm::Format::Argb8888).expect("Failed to create buffer on draw.");

        // figure out the noise movement 
        let frame_time = self.last_frame_time.elapsed().as_nanos();
        let movement = frame_time as f64 / 1000000.0;
        self.noise_z += movement;

        canvas.chunks_exact_mut(4).enumerate().for_each(|(index, chunk)| {
            let width_usize = usize::try_from(self.width).expect("width to usize failed");
            let x = u32::try_from(index % width_usize).expect("x to u32 failed");
            let y = u32::try_from(index / width_usize).expect("y to u32 failed");

            let noise_size_x = 1920.0 / 32.0;
            let noise_size_y = 1080.0 / 32.0;
            let noise_x = (x as f64 / width as f64) * noise_size_x;
            let noise_y = (y as f64 / height as f64) * noise_size_y;

            // noise math stuff 
            // thanks https://github.com/Razaekel/noise-rs/issues/354 for making me realize im stupid 
            // distortion algorithm provided by https://gamedev.stackexchange.com/a/162460
            let strength = 1.0;
            let distorted_x: f64 = self.noise.get([noise_x + 0.6335, noise_y + 0.6241, self.noise_z]) * strength;
            let distorted_y: f64 = self.noise.get([noise_x - 0.2316, noise_y - 0.5251, self.noise_z]) * strength;
            let value: f64 = self.noise.get([(noise_x + 0.1) + distorted_x, (noise_y + 0.1) + distorted_y]);
            let value_normalized: f32 = ((value + 1.0) / 2.0) as f32;

            let color = self.color_ramp.get_color_at_point(value_normalized);
            let wayland_color = color.get_wayland_color();
            let array: &mut [u8; 4] = chunk.try_into().unwrap();
            *array = wayland_color.to_le_bytes();
        });

        self.layer.wl_surface().damage_buffer(0, 0, width as i32, height as i32);
        self.layer.wl_surface().frame(qh, self.layer.wl_surface().clone());
        buffer.attach_to(self.layer.wl_surface()).expect("Failed to attach to buffer");
        self.layer.commit();

        self.last_frame_time = Instant::now();
    }
}

delegate_compositor!(WaylandState);
delegate_output!(WaylandState);
delegate_shm!(WaylandState);
delegate_seat!(WaylandState);
delegate_layer!(WaylandState);
delegate_registry!(WaylandState);

pub fn start() {
    let conn = Connection::connect_to_env().expect("Unable to connect to a compositor.");
    let (globals, mut event_queue) = registry_queue_init(&conn).unwrap();
    let qh = event_queue.handle();

    let compositor = CompositorState::bind(&globals, &qh).expect("Compositor does not support 'wl_compositor'");
    let layer_shell = LayerShell::bind(&globals, &qh).expect("Compositor does not support 'zwlr_layer_shell_v1'");
    let shm = Shm::bind(&globals, &qh).expect("Compositor does not support `wl_shm`");

    // TODO
    let width: u32 = 1920;
    let height: u32 = 1080;
    let surface = compositor.create_surface(&qh);
    let layer = layer_shell.create_layer_surface(&qh, surface, Layer::Background, Some("noisy-wallpaper"), None);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    layer.set_size(width, height);
    layer.commit();
    let pool = SlotPool::new((width * height * 4) as usize, &shm).expect("Failed to create pool");

    let noise = Perlin::new(2903568236);
    let color_ramp = ColorRamp::new(vec![
        Color::new(0, 0, 0, 255),
        Color::new(100, 58, 144, 255),
    ]);

    let mut state = WaylandState {
        width,
        height,
        close: false,
        shm,
        pool,
        layer,
        noise,
        noise_z: 0.0,
        last_frame_time: Instant::now(),
        color_ramp,
        
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
    };
    loop {
        event_queue.blocking_dispatch(&mut state).unwrap();
    }
}
