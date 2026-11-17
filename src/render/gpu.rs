use {
    super::{
        DrawList, Layer, PlaneVertex, RenderConfig, RenderError, Viewport, sphere::SphereRenderer,
        text::TextRenderer, tube::TubeRenderer,
    },
    std::ops::Range,
    wgpu::util::DeviceExt,
};

pub(super) struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    color_format: wgpu::TextureFormat,
    viewport: Viewport,
    depth: wgpu::TextureView,
    plane_pipeline: wgpu::RenderPipeline,
    overlay_pipeline: wgpu::RenderPipeline,
    spheres: SphereRenderer,
    tubes: TubeRenderer,
    text: TextRenderer,
    clear_color: wgpu::Color,
}

pub(crate) struct Target(wgpu::SurfaceTarget<'static>);

impl Gpu {
    pub(super) async fn new(target: Target, config: RenderConfig) -> Result<Self, RenderError> {
        // Detect a usable WebGPU adapter, falling back to WebGL2 where necessary.
        let descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        #[cfg(target_arch = "wasm32")]
        let descriptor = descriptor.with_display_handle(Box::new(BrowserDisplay));
        let instance = wgpu::util::new_instance_with_webgpu_detection(descriptor).await;
        let surface = instance
            .create_surface(target.0)
            .map_err(|error| RenderError::Initialization(error.to_string()))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(|error| RenderError::Initialization(error.to_string()))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Strandscape"),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await
            .map_err(|error| RenderError::Initialization(error.to_string()))?;
        let viewport = config.viewport;
        let mut surface_config = surface
            .get_default_config(&adapter, viewport.width.max(1), viewport.height.max(1))
            .ok_or_else(|| RenderError::Initialization("No supported surface format".into()))?;
        surface_config.present_mode = wgpu::PresentMode::Fifo;
        // Use an sRGB view so linear scene colors match across browser backends.
        let color_format = surface_config.format.add_srgb_suffix();
        if color_format != surface_config.format {
            surface_config.view_formats.push(color_format);
        }
        surface.configure(&device, &surface_config);
        let depth = create_depth(&device, surface_config.width, surface_config.height);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Plane"),
            source: wgpu::ShaderSource::Wgsl(include_str!("plane.wgsl").into()),
        });
        let [plane_pipeline, overlay_pipeline] = [Layer::World, Layer::Overlay].map(|space| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Plane"),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<PlaneVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4],
                    }],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: color_format,
                        blend: (space == Layer::Overlay)
                            .then_some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(space == Layer::World),
                    depth_compare: Some(if space == Layer::World {
                        wgpu::CompareFunction::LessEqual
                    } else {
                        wgpu::CompareFunction::Always
                    }),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        });
        let text = TextRenderer::new(&device, &queue, color_format, DEPTH_FORMAT);
        let spheres = SphereRenderer::new(&device, color_format, DEPTH_FORMAT);
        let tubes = TubeRenderer::new(&device, color_format, DEPTH_FORMAT);
        let [r, g, b, a] = config.clear_color;
        Ok(Self {
            surface,
            device,
            queue,
            config: surface_config,
            color_format,
            viewport,
            depth,
            plane_pipeline,
            overlay_pipeline,
            spheres,
            tubes,
            text,
            clear_color: wgpu::Color { r, g, b, a },
        })
    }

    pub(super) fn draw(&mut self, list: &DrawList) -> Result<(), RenderError> {
        if self.viewport.width == 0 || self.viewport.height == 0 {
            return Ok(());
        }

        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                return Err(RenderError::Surface(
                    "Surface changed; redraw required".into(),
                ));
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            error => return Err(RenderError::Surface(format!("{error:?}"))),
        };
        self.text.prepare(&self.device, &self.queue, &list.text)?;
        self.spheres
            .prepare(&self.device, &self.queue, &list.spheres, &list.world_view);
        self.tubes
            .prepare(&self.device, &self.queue, &list.tubes, &list.world_view);
        let mut end = 0;
        let ranges = list.planes.each_ref().map(|vertices| {
            let start = end;
            end += vertices.len() as u32;
            start..end
        });
        let vertices: Vec<_> = list.planes.iter().flatten().copied().collect();
        let plane_vertices = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Plane vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.color_format),
            ..Default::default()
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            self.draw_planes(
                &mut pass,
                &plane_vertices,
                ranges[Layer::World as usize].clone(),
                Layer::World,
            );
            self.spheres.draw(&mut pass);
            // Blend tubes over opaque geometry without blocking later transparent tubes.
            self.tubes.draw(&mut pass);
            self.text.draw(&mut pass, Layer::World);

            // Alpha-blend places above the world, then cover them with the HUD.
            self.draw_planes(
                &mut pass,
                &plane_vertices,
                ranges[Layer::Overlay as usize].clone(),
                Layer::Overlay,
            );
            self.text.draw(&mut pass, Layer::Overlay);
            self.draw_planes(
                &mut pass,
                &plane_vertices,
                ranges[Layer::Hud as usize].clone(),
                Layer::Hud,
            );
            self.text.draw(&mut pass, Layer::Hud);
        }
        self.queue.submit([encoder.finish()]);
        frame.present();
        Ok(())
    }

    pub(super) fn resize(&mut self, viewport: Viewport) {
        self.viewport = viewport;
        if viewport.width == 0 || viewport.height == 0 {
            return;
        }

        self.config.width = viewport.width;
        self.config.height = viewport.height;
        self.surface.configure(&self.device, &self.config);
        self.depth = create_depth(&self.device, viewport.width, viewport.height);
    }

    pub(super) fn set_background(&mut self, color: [f64; 4]) {
        let [r, g, b, a] = color;
        self.clear_color = wgpu::Color { r, g, b, a };
    }

    fn draw_planes(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        vertices: &wgpu::Buffer,
        range: Range<u32>,
        space: Layer,
    ) {
        if range.is_empty() {
            return;
        }
        pass.set_pipeline(if space == Layer::World {
            &self.plane_pipeline
        } else {
            &self.overlay_pipeline
        });
        pass.set_vertex_buffer(0, vertices.slice(..));
        pass.draw(range, 0..1);
    }
}

#[cfg(target_arch = "wasm32")]
impl From<web_sys::HtmlCanvasElement> for Target {
    fn from(canvas: web_sys::HtmlCanvasElement) -> Self {
        Self(wgpu::SurfaceTarget::Canvas(canvas))
    }
}

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;

#[cfg(target_arch = "wasm32")]
#[derive(Debug)]
struct BrowserDisplay;

#[cfg(target_arch = "wasm32")]
impl raw_window_handle::HasDisplayHandle for BrowserDisplay {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        Ok(raw_window_handle::DisplayHandle::web())
    }
}

fn create_depth(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("Depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default())
}
