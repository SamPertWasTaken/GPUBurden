use std::{fs::File, io::Read, num::NonZero};

use bytemuck::NoUninit;
use rand::{rngs::ThreadRng, Rng};
use wgpu::{include_wgsl, rwh::{RawDisplayHandle, RawWindowHandle}, util::{BufferInitDescriptor, DeviceExt}, Adapter, Backends, BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, BlendState, Buffer, ColorTargetState, ColorWrites, Device, Face, FragmentState, FrontFace, Instance, InstanceDescriptor, LoadOp, MultisampleState, PipelineCompilationOptions, PipelineLayoutDescriptor, PolygonMode, PrimitiveState, PrimitiveTopology, Queue, RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline, RenderPipelineDescriptor, RequestAdapterOptions, ShaderModuleDescriptor, ShaderStages, StoreOp, Surface, SurfaceConfiguration, SurfaceTargetUnsafe, TextureViewDescriptor};

use crate::configuration::MonitorConfig;

const DEFAULT_SHADER: ShaderModuleDescriptor<'_> = include_wgsl!("shaders/frag.wgsl");

#[repr(C)]
#[derive(Copy, Clone, NoUninit)]
struct FragmentInputBuffer {
    screen_size: [u32; 2],
    frame: u32,
    seed: u32
}

// Most of the rendering code is based off of https://github.com/Smithay/client-toolkit/blob/master/examples/wgpu.rs
// I couldn't find examples on how to re-use the wgpu instance/device to render to two different
// surfaces *with different shaders*. 
// If that exists, please link me there because I understand this way of doing things where there's
// an entirely different GPU instance for each surface is terrible.
pub struct Renderer {
    surface: Surface<'static>,
    surface_config: Option<SurfaceConfiguration>,
    adapter: Adapter,
    device: Device,
    queue: Queue,
    fragment_buffer: Option<Buffer>,
    bind_group: Option<BindGroup>,
    pipeline: Option<RenderPipeline>,
    shader_code: ShaderModuleDescriptor<'static>,

    pub width: u32,
    pub height: u32,
    surface_configured: bool,
    frame: u32,
    rand: ThreadRng
}
impl Renderer {
    pub fn for_layer(raw_display_handle: RawDisplayHandle, raw_window_handle: RawWindowHandle, config: &Option<MonitorConfig>) -> Self {
        let instance = Instance::new(&InstanceDescriptor {
            backends: Backends::all(),
            ..Default::default()
        });

        let surface = unsafe { 
            // TODO not sure why this has to be unsafe?
            instance.create_surface_unsafe(SurfaceTargetUnsafe::RawHandle {
                raw_display_handle,
                raw_window_handle
            }).expect("Failed to create wgpu surface.")
        };

        let adapter = pollster::block_on(instance.request_adapter(&RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        })).expect("Wgpu failed to find a compatible adapter.");

        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).expect("Failed to request a wgpu device.");

        let mut shader_code: ShaderModuleDescriptor = DEFAULT_SHADER;
        if let Some(config) = config {
            // read from the file 
            match File::open(&config.shader) {
                Ok(mut file) => {
                    let mut contents = String::new();
                    match file.read_to_string(&mut contents) {
                        Ok(_) => shader_code = ShaderModuleDescriptor { 
                            label: None,
                            source: wgpu::ShaderSource::Wgsl(contents.into())
                        },
                        Err(e) => println!("failed to read {}: {e}", config.shader),
                    };
                },
                Err(e) => println!("shader file {} not found: {e}", config.shader)
            };
        };

        Self {
            surface,
            surface_config: None,
            adapter,
            device,
            queue,
            fragment_buffer: None,
            bind_group: None,
            pipeline: None,
            shader_code,

            width: 0,
            height: 0,
            surface_configured: false,
            frame: 0,
            rand: rand::rng()
        }
    }

    pub fn configure_surface(&mut self, width: u32, height: u32) {
        let surface_capabilities = self.surface.get_capabilities(&self.adapter);
        let surface_config = SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_capabilities.formats[0],
            view_formats: vec![surface_capabilities.formats[0]],
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            width,
            height,
            desired_maximum_frame_latency: 2,
            present_mode: wgpu::PresentMode::AutoVsync
        };
        self.surface.configure(&self.device, &surface_config);
        self.surface_config = Some(surface_config);
        self.reconfigure_pipeline();
        self.surface_configured = true;
    }

    fn reconfigure_pipeline(&mut self) {
        // Credit for teaching me this part goes to https://sotrh.github.io/learn-wgpu/beginner/tutorial3-pipeline
        let vertex_shader = self.device.create_shader_module(include_wgsl!("shaders/vertex.wgsl"));
        let fragment_shader = self.device.create_shader_module(self.shader_code.clone());

        // deal with the buffers first
        let fragment_input_buffer = FragmentInputBuffer {
            screen_size: [self.width, self.height],
            frame: self.frame,
            seed: self.rand.random_range(0..1000000)
        };
        let wgpu_fragment_buffer = self.device.create_buffer_init(&BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&[fragment_input_buffer]),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
        });
        // bind groups 
        // thanks to the wgpu matrix server for making me realize these can pass into to the fragment shader
        let wgpu_bind_group_layout = self.device.create_bind_group_layout(&BindGroupLayoutDescriptor {
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
        let wgpu_bind_group = self.device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &wgpu_bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: wgpu_fragment_buffer.as_entire_binding(),
                },
            ],
        });

        // pipeline now
        let pipeline_layout = self.device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&wgpu_bind_group_layout],
            push_constant_ranges: &[]
        });

        let wgpu_pipeline = self.device.create_render_pipeline(&RenderPipelineDescriptor {
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
                    format: self.surface_config.clone().expect("Pipeline called to reconfigure without a surface config being set.").format,
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

        self.fragment_buffer = Some(wgpu_fragment_buffer);
        self.bind_group = Some(wgpu_bind_group);
        self.pipeline = Some(wgpu_pipeline);
    }

    pub fn draw(&mut self) {
        let texture = self.surface.get_current_texture().expect("Failed to get next swapchain texture");
        let texture_view = texture.texture.create_view(&TextureViewDescriptor::default());
        self.frame += 1;
        let mut encoder = self.device.create_command_encoder(&Default::default());
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

            if self.surface_configured {
                let pipeline = self.pipeline.as_ref().expect("WGPU was configured but pipeline not set. Bug report this!");
                renderpass.set_pipeline(pipeline);
                renderpass.set_bind_group(0, &self.bind_group, &[]);
                renderpass.draw(0..3, 0..1);
            }
        }

        if self.surface_configured {
            let frag_buffer = self.fragment_buffer.as_ref().expect("WGPU was configured but fragment input buffer not set. Bug report this!");
            let fragment_input_buffer = FragmentInputBuffer {
                screen_size: [self.width, self.height],
                frame: self.frame,
                seed: self.rand.random_range(0..1000000)
            };
            self.queue.write_buffer(frag_buffer, 0, bytemuck::cast_slice(&[fragment_input_buffer]));
        }

        self.queue.submit(Some(encoder.finish()));
        texture.present();
    }

    pub fn free_surface(self) {
        drop(self.surface);
    }
}
