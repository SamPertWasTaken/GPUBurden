use std::{ptr::NonNull, time::Instant};

use noise::{NoiseFn, Perlin};
use smithay_client_toolkit::{compositor::{CompositorHandler, CompositorState}, delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_seat, delegate_shm, output::{OutputHandler, OutputState}, registry::{ProvidesRegistryState, RegistryState}, registry_handlers, seat::{SeatHandler, SeatState}, shell::{wlr_layer::{KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface}, WaylandSurface}, shm::{slot::SlotPool, Shm, ShmHandler}};
use wayland_client::{globals::registry_queue_init, protocol::{wl_shm, wl_surface::WlSurface}, Connection, Proxy, QueueHandle};
use wgpu::{rwh::{RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle}, Backends, Instance, InstanceDescriptor, LoadOp, RenderPassColorAttachment, RenderPassDescriptor, RequestAdapterOptions, RequestAdapterOptionsBase, StoreOp, SurfaceConfiguration, SurfaceTargetUnsafe, TextureViewDescriptor};

use crate::{color::Color, color_ramp::ColorRamp};

pub struct WaylandState {
    width: u32,
    height: u32,
    close: bool,
    wgpu_surface: wgpu::Surface<'static>,
    wgpu_adapter: wgpu::Adapter,
    wgpu_device: wgpu::Device,
    wgpu_queue: wgpu::Queue,

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
        
        // also heavily based off the wgpu example wayland-client-toolkit gives
        let surface_capabilities = self.wgpu_surface.get_capabilities(&self.wgpu_adapter);
        let surface_config = SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_capabilities.formats[0],
            view_formats: vec![surface_capabilities.formats[0]],
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            width: self.width,
            height: self.height,
            desired_maximum_frame_latency: 2,
            present_mode: wgpu::PresentMode::Mailbox
        };
        self.wgpu_surface.configure(&self.wgpu_device, &surface_config);

        // re-draw the frame
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

impl ProvidesRegistryState for WaylandState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

impl WaylandState {
    pub fn draw(&mut self, qh: &QueueHandle<Self>) {
        let texture = self.wgpu_surface.get_current_texture().expect("Failed to get next swapchain texture");
        let texture_view = texture.texture.create_view(&TextureViewDescriptor::default());

        let mut encoder = self.wgpu_device.create_command_encoder(&Default::default());
        {
            let _renderpass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &texture_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: LoadOp::Clear(wgpu::Color::BLUE),
                        store: StoreOp::Store
                    }
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None
            });
        }
        self.wgpu_queue.submit(Some(encoder.finish()));
        texture.present();
    }
}

delegate_compositor!(WaylandState);
delegate_output!(WaylandState);
delegate_seat!(WaylandState);
delegate_layer!(WaylandState);
delegate_registry!(WaylandState);

pub fn start() {
    let conn = Connection::connect_to_env().expect("Unable to connect to a compositor.");
    let (globals, mut event_queue) = registry_queue_init(&conn).unwrap();
    let qh = event_queue.handle();

    let compositor = CompositorState::bind(&globals, &qh).expect("Compositor does not support 'wl_compositor'");
    let layer_shell = LayerShell::bind(&globals, &qh).expect("Compositor does not support 'zwlr_layer_shell_v1'");

    // TODO
    let width: u32 = 1920;
    let height: u32 = 1080;
    let surface = compositor.create_surface(&qh);
    let layer = layer_shell.create_layer_surface(&qh, surface.clone(), Layer::Background, Some("noisy-wallpaper"), None);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    layer.set_size(width, height);
    layer.commit();

    // setup web gpu 
    // mostly based off of https://github.com/Smithay/client-toolkit/blob/master/examples/wgpu.rs
    let wgpu_instance = Instance::new(&InstanceDescriptor {
        backends: Backends::all(),
        ..Default::default()
    });
    let raw_display_handle = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
        NonNull::new(conn.backend().display_ptr() as *mut _).expect("Failed to create display handle for wgpu.")
    ));
    let raw_window_handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(
        NonNull::new(surface.id().as_ptr() as *mut _).expect("Failed to create window handle for wgpu.")
    ));

    let wgpu_surface = unsafe { 
        // TODO not sure why this has to be unsafe?
        wgpu_instance.create_surface_unsafe(SurfaceTargetUnsafe::RawHandle {
            raw_display_handle,
            raw_window_handle
        }).expect("Failed to create wgpu surface.")
    };

    // Adapter 
    let wgpu_adapter = pollster::block_on(wgpu_instance.request_adapter(&RequestAdapterOptions {
        compatible_surface: Some(&wgpu_surface),
        ..Default::default()
    })).expect("Wgpu failed to find a compatible adapter.");

    let (wgpu_device, wgpu_queue) = pollster::block_on(wgpu_adapter.request_device(&Default::default())).expect("Failed to request a wgpu device.");

    let mut state = WaylandState {
        width,
        height,
        close: false,
        wgpu_surface,
        wgpu_adapter,
        wgpu_device,
        wgpu_queue,
        
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

    drop(state.wgpu_surface);
}
