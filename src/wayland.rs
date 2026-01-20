use std::{num::NonZero, ptr::NonNull};

use bytemuck::NoUninit;
use rand::{rngs::ThreadRng, Rng};
use smithay_client_toolkit::{compositor::{CompositorHandler, CompositorState}, delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_seat, output::{OutputHandler, OutputState}, registry::{ProvidesRegistryState, RegistryState}, registry_handlers, seat::{SeatHandler, SeatState}, shell::{wlr_layer::{KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface}, WaylandSurface}};
use wayland_client::{globals::registry_queue_init, Connection, Proxy, QueueHandle};
use wgpu::{include_wgsl, rwh::{RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle}, util::{BufferInitDescriptor, DeviceExt}, Backends, BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, BlendState, Buffer, ColorTargetState, ColorWrites, Face, FragmentState, FrontFace, Instance, InstanceDescriptor, LoadOp, MultisampleState, PipelineCompilationOptions, PipelineLayoutDescriptor, PolygonMode, PrimitiveState, PrimitiveTopology, RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline, RenderPipelineDescriptor, RequestAdapterOptions, ShaderStages, StoreOp, SurfaceConfiguration, SurfaceTargetUnsafe, TextureViewDescriptor};

pub struct WaylandState {
    width: u32,
    height: u32,
    layer: LayerSurface,
    rand: ThreadRng,

    close: bool,
    frame: u32,

    wgpu_surface: wgpu::Surface<'static>,
    wgpu_adapter: wgpu::Adapter,
    wgpu_device: wgpu::Device,
    wgpu_queue: wgpu::Queue,
    wgpu_pipeline: Option<RenderPipeline>,
    wgpu_fragment_buffer: Option<Buffer>,
    wgpu_bind_group: Option<BindGroup>,
    wgpu_configured: bool,

    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
}

#[repr(C)]
#[derive(Copy, Clone, NoUninit)]
struct FragmentInputBuffer {
    screen_size: [u32; 2],
    frame: u32,
    seed: u32,
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

        // Shader stuff now 
        // Credit for teaching me this part goes to https://sotrh.github.io/learn-wgpu/beginner/tutorial3-pipeline
        let vertex_shader = self.wgpu_device.create_shader_module(include_wgsl!("shaders/vertex.wgsl"));
        let fragment_shader = self.wgpu_device.create_shader_module(include_wgsl!("shaders/frag.wgsl"));

        // bind groups 
        // thanks to the wgpu matrix server for making me realize these can pass into to the fragment shader
        let fragment_input_buffer = FragmentInputBuffer {
            screen_size: [self.width, self.height],
            frame: self.frame,
            seed: self.rand.random(),
        };
        let wgpu_fragment_buffer = self.wgpu_device.create_buffer_init(&BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&[fragment_input_buffer]),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
        });
        let wgpu_bind_group_layout = self.wgpu_device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer { 
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZero::new(std::mem::size_of::<FragmentInputBuffer>() as u64)
                    },
                    count: None
                }
            ],
            label: None
        });
        let wgpu_bind_group = self.wgpu_device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &wgpu_bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: wgpu_fragment_buffer.as_entire_binding(),
                },
            ],
        });

        let pipeline_layout = self.wgpu_device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&wgpu_bind_group_layout],
            push_constant_ranges: &[]
        });

        let wgpu_pipeline = self.wgpu_device.create_render_pipeline(&RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vertex_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: PipelineCompilationOptions::default()
            },
            fragment: Some(FragmentState {
                module: &fragment_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(ColorTargetState {
                    format: surface_config.format,
                    blend: Some(BlendState::REPLACE),
                    write_mask: ColorWrites::ALL
                })],
                compilation_options: PipelineCompilationOptions::default()
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: Some(Face::Back),
                polygon_mode: PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false
            },
            depth_stencil: None,
            multisample: MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false
            },
            multiview: None,
            cache: None
        });

        self.wgpu_fragment_buffer = Some(wgpu_fragment_buffer);
        self.wgpu_bind_group = Some(wgpu_bind_group);
        self.wgpu_pipeline = Some(wgpu_pipeline);

        self.wgpu_configured = true;
        // re-draw the frame
        self.draw(qh);
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
        let texture = self.wgpu_surface.get_current_texture().expect("Failed to get next swapchain texture");
        let texture_view = texture.texture.create_view(&TextureViewDescriptor::default());

        self.frame += 1;

        let mut encoder = self.wgpu_device.create_command_encoder(&Default::default());
        {
            let mut renderpass = encoder.begin_render_pass(&RenderPassDescriptor {
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

            if self.wgpu_configured {
                let pipeline = self.wgpu_pipeline.as_ref().expect("WGPU was configured but pipeline not set. Bug report this!");
                // let bind_group = self.wgpu_bind_group.as_ref().expect("WGPU was configured but bind group not set. Bug report this!");

                renderpass.set_pipeline(pipeline);
                renderpass.set_bind_group(0, &self.wgpu_bind_group, &[]);
                renderpass.draw(0..3, 0..1);
            }
        }

        if self.wgpu_configured {
            let frag_buffer = self.wgpu_fragment_buffer.as_ref().expect("WGPU was configured but fragment input buffer not set. Bug report this!");
            let fragment_input_buffer = FragmentInputBuffer {
                screen_size: [self.width, self.height],
                frame: self.frame,
                seed: self.rand.random(),
            };
            self.wgpu_queue.write_buffer(frag_buffer, 0, bytemuck::cast_slice(&[fragment_input_buffer]));
        }

        self.wgpu_queue.submit(Some(encoder.finish()));
        texture.present();

        self.layer.wl_surface().frame(qh, self.layer.wl_surface().clone());
        self.layer.commit();
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
        layer,
        rand: rand::rng(),
        close: false,
        frame: 0,

        wgpu_surface,
        wgpu_adapter,
        wgpu_device,
        wgpu_queue,
        // below gets made in the wayland configure call 
        wgpu_pipeline: None, 
        wgpu_fragment_buffer: None,
        wgpu_bind_group: None,
        wgpu_configured: false,
        
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
