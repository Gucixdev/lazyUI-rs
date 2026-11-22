//! Pixels to the screen. `Cpu` copies into a shared-memory buffer (softbuffer: wl_shm / X11 / DRM, no GPU, works
//! everywhere); `Gpu` uploads a texture and draws it with wgpu. Both take the same `Pixels`.

use super::raster::Pixels;
use std::num::NonZeroU32;
use std::sync::Arc;
use winit::window::Window;

pub trait Present {
    fn path(&self) -> super::Path;
    fn resize(&mut self, w: u32, h: u32);
    fn present(&mut self, px: &Pixels);
}

pub struct Cpu {
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
}

impl Cpu {
    pub fn new(window: Arc<Window>, w: u32, h: u32) -> Result<Cpu, String> {
        let ctx = softbuffer::Context::new(window.clone()).map_err(|e| format!("softbuffer context: {e}"))?;
        let surface = softbuffer::Surface::new(&ctx, window).map_err(|e| format!("softbuffer surface: {e}"))?;
        let mut c = Cpu { surface };
        c.resize(w, h);
        Ok(c)
    }
}

impl Present for Cpu {
    fn path(&self) -> super::Path {
        super::Path::Cpu
    }

    fn resize(&mut self, w: u32, h: u32) {
        if let (Some(w), Some(h)) = (NonZeroU32::new(w), NonZeroU32::new(h)) {
            let _ = self.surface.resize(w, h);
        }
    }

    fn present(&mut self, px: &Pixels) {
        let Ok(mut buf) = self.surface.buffer_mut() else { return };
        // the buffer is 0x00RRGGBB; if the window size and `px` disagree for a moment, show nothing rather than garbage
        if buf.len() != px.w * px.h {
            return;
        }
        for (d, s) in buf.iter_mut().zip(px.rgba.chunks_exact(4)) {
            *d = (s[0] as u32) << 16 | (s[1] as u32) << 8 | s[2] as u32;
        }
        let _ = buf.present();
    }
}

#[cfg(feature = "gpu")]
pub use gpu::Gpu;

#[cfg(feature = "gpu")]
mod gpu {
    use super::*;

    const SHADER: &str = r#"
struct VOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs(@builtin(vertex_index) i: u32) -> VOut {
    var p = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    var o: VOut;
    o.pos = vec4<f32>(p[i], 0.0, 1.0);
    o.uv = vec2<f32>((p[i].x + 1.0) * 0.5, (1.0 - p[i].y) * 0.5);
    return o;
}
@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var smp: sampler;
@fragment fn fs(i: VOut) -> @location(0) vec4<f32> { return textureSample(tex, smp, i.uv); }
"#;

    pub struct Gpu {
        surface: wgpu::Surface<'static>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        config: wgpu::SurfaceConfiguration,
        pipeline: wgpu::RenderPipeline,
        layout: wgpu::BindGroupLayout,
        sampler: wgpu::Sampler,
        texture: wgpu::Texture,
        bind: wgpu::BindGroup,
    }

    impl Gpu {
        pub fn new(window: Arc<Window>, w: u32, h: u32) -> Result<Gpu, String> {
            let instance = wgpu::Instance::default();
            let surface = instance.create_surface(window).map_err(|e| format!("surface: {e}"))?;
            let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { compatible_surface: Some(&surface), ..Default::default() }))
                .map_err(|e| format!("no GPU adapter: {e}"))?;
            let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).map_err(|e| format!("device: {e}"))?;
            let caps = surface.get_capabilities(&adapter);
            if caps.formats.is_empty() {
                return Err("the surface has no usable format".into());
            }
            // the frame is already in display colours: an sRGB surface would brighten it a second time
            let format = caps.formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(caps.formats[0]);
            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width: w.max(1),
                height: h.max(1),
                present_mode: wgpu::PresentMode::Fifo,
                alpha_mode: caps.alpha_modes[0],
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
                color_space: Default::default(),
            };
            surface.configure(&device, &config);
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: None, source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
                ],
            });
            let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&pl),
                vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
                fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("fs"), compilation_options: Default::default(), targets: &[Some(format.into())] }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });
            let sampler = device.create_sampler(&wgpu::SamplerDescriptor { mag_filter: wgpu::FilterMode::Nearest, min_filter: wgpu::FilterMode::Nearest, ..Default::default() });
            let (texture, bind) = Gpu::texture(&device, &layout, &sampler, config.width, config.height);
            Ok(Gpu { surface, device, queue, config, pipeline, layout, sampler, texture, bind })
        }

        fn texture(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, sampler: &wgpu::Sampler, w: u32, h: u32) -> (wgpu::Texture, wgpu::BindGroup) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) }],
            });
            (texture, bind)
        }
    }

    impl Present for Gpu {
        fn path(&self) -> super::super::Path {
            super::super::Path::Gpu
        }

        fn resize(&mut self, w: u32, h: u32) {
            self.config.width = w.max(1);
            self.config.height = h.max(1);
            self.surface.configure(&self.device, &self.config);
            let (t, b) = Gpu::texture(&self.device, &self.layout, &self.sampler, self.config.width, self.config.height);
            self.texture = t;
            self.bind = b;
        }

        fn present(&mut self, px: &Pixels) {
            if px.w as u32 != self.config.width || px.h as u32 != self.config.height {
                return;
            }
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &self.texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                &px.rgba,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(px.w as u32 * 4), rows_per_image: Some(px.h as u32) },
                wgpu::Extent3d { width: px.w as u32, height: px.h as u32, depth_or_array_layers: 1 },
            );
            let frame = match self.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(t) => t,
                _ => {
                    self.surface.configure(&self.device, &self.config);
                    return;
                }
            };
            let view = frame.texture.create_view(&Default::default());
            let mut enc = self.device.create_command_encoder(&Default::default());
            {
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind, &[]);
                pass.draw(0..3, 0..1);
            }
            self.queue.submit(Some(enc.finish()));
            self.queue.present(frame);
        }
    }
}
