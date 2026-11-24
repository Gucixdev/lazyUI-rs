//! Pixels to the screen. `Cpu` copies into a shared-memory buffer (softbuffer: wl_shm / X11 / DRM, no GPU, works
//! everywhere); `Gpu` uploads a texture and draws it with wgpu. Both take the same `Pixels`.

use super::raster::{Pixels, PxRect};
use std::num::NonZeroU32;
use std::sync::Arc;
use winit::window::Window;

pub trait Present {
    fn path(&self) -> super::Path;
    fn resize(&mut self, w: u32, h: u32);
    /// Show `px`. `damage` is what changed since the previous call (`None`: everything); a presenter may copy only that.
    fn present(&mut self, px: &Pixels, damage: Option<PxRect>);
}

/// How many past frames' damage the CPU path remembers: a buffer of age N lacks the last N-1 frames, so it needs their union.
const HISTORY: usize = 4;

/// What a buffer of this `age` must be given to equal the newest frame: the union of the damage of the frames it missed (the
/// newest included), or everything when its contents are unknown (age 0), older than what is remembered, or one of those frames
/// was a whole one.
fn copy_region(history: &std::collections::VecDeque<Option<PxRect>>, age: u8, whole: PxRect) -> PxRect {
    let age = age as usize;
    if age == 0 || age > history.len() {
        return whole;
    }
    let mut acc: Option<PxRect> = None;
    for d in history.iter().take(age) {
        let Some(d) = d else { return whole };
        acc = Some(acc.map_or(*d, |a| a.union(*d)));
    }
    acc.unwrap_or(whole)
}

pub struct Cpu {
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    /// damage of the last frames, newest first; `None` = a whole frame
    history: std::collections::VecDeque<Option<PxRect>>,
}

impl Cpu {
    pub fn new(window: Arc<Window>, w: u32, h: u32) -> Result<Cpu, String> {
        let ctx = softbuffer::Context::new(window.clone()).map_err(|e| format!("softbuffer context: {e}"))?;
        let surface = softbuffer::Surface::new(&ctx, window).map_err(|e| format!("softbuffer surface: {e}"))?;
        let mut c = Cpu { surface, history: Default::default() };
        c.resize(w, h);
        Ok(c)
    }
}

impl Present for Cpu {
    fn path(&self) -> super::Path {
        super::Path::Cpu
    }

    fn resize(&mut self, w: u32, h: u32) {
        self.history.clear();
        if let (Some(w), Some(h)) = (NonZeroU32::new(w), NonZeroU32::new(h)) {
            let _ = self.surface.resize(w, h);
        }
    }

    fn present(&mut self, px: &Pixels, damage: Option<PxRect>) {
        let Ok(mut buf) = self.surface.buffer_mut() else { return };
        // the buffer is 0x00RRGGBB; if the window size and `px` disagree for a moment, show nothing rather than garbage
        if buf.len() != px.w * px.h {
            return;
        }
        self.history.push_front(damage);
        self.history.truncate(HISTORY);
        let whole = PxRect { x: 0, y: 0, w: px.w, h: px.h };
        let r = copy_region(&self.history, buf.age(), whole);
        for y in r.y..r.y + r.h {
            let row = y * px.w;
            for x in r.x..r.x + r.w {
                let s = &px.rgba[(row + x) * 4..(row + x) * 4 + 3];
                buf[row + x] = (s[0] as u32) << 16 | (s[1] as u32) << 8 | s[2] as u32;
            }
        }
        match damage.filter(|d| d.w > 0 && d.h > 0).and_then(|d| Some((d, NonZeroU32::new(d.w as u32)?, NonZeroU32::new(d.h as u32)?))) {
            Some((d, w, h)) => {
                let _ = buf.present_with_damage(&[softbuffer::Rect { x: d.x as u32, y: d.y as u32, width: w, height: h }]);
            }
            None => {
                let _ = buf.present();
            }
        }
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
        /// the texture was just made and holds nothing yet
        fresh: bool,
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
            Ok(Gpu { surface, device, queue, config, pipeline, layout, sampler, texture, bind, fresh: true })
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
            self.fresh = true;
        }

        fn present(&mut self, px: &Pixels, damage: Option<PxRect>) {
            if px.w as u32 != self.config.width || px.h as u32 != self.config.height {
                return;
            }
            // a texture that was just made has nothing in it: the first upload is always whole
            let region = if std::mem::take(&mut self.fresh) { None } else { damage };
            let r = region.filter(|r| r.w > 0 && r.h > 0 && r.x + r.w <= px.w && r.y + r.h <= px.h).unwrap_or(PxRect { x: 0, y: 0, w: px.w, h: px.h });
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &self.texture, mip_level: 0, origin: wgpu::Origin3d { x: r.x as u32, y: r.y as u32, z: 0 }, aspect: wgpu::TextureAspect::All },
                &px.rgba,
                wgpu::TexelCopyBufferLayout { offset: ((r.y * px.w + r.x) * 4) as u64, bytes_per_row: Some(px.w as u32 * 4), rows_per_image: Some(r.h as u32) },
                wgpu::Extent3d { width: r.w as u32, height: r.h as u32, depth_or_array_layers: 1 },
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    fn r(x: usize, y: usize, w: usize, h: usize) -> PxRect {
        PxRect { x, y, w, h }
    }

    #[test]
    fn a_buffer_gets_the_damage_of_every_frame_it_missed() {
        let whole = r(0, 0, 100, 50);
        let hist: VecDeque<_> = [Some(r(10, 10, 5, 5)), Some(r(40, 2, 5, 5)), Some(r(0, 30, 3, 3))].into();
        assert_eq!(copy_region(&hist, 1, whole), r(10, 10, 5, 5), "same as the last frame: just the new damage");
        assert_eq!(copy_region(&hist, 2, whole), r(10, 2, 35, 13), "double buffering: this frame and the one before");
        assert_eq!(copy_region(&hist, 3, whole), r(0, 2, 45, 31));
        assert_eq!(copy_region(&hist, 0, whole), whole, "unknown contents: everything");
        assert_eq!(copy_region(&hist, 4, whole), whole, "older than the memory: everything");
        let with_whole: VecDeque<_> = [Some(r(1, 1, 1, 1)), None].into();
        assert_eq!(copy_region(&with_whole, 1, whole), r(1, 1, 1, 1));
        assert_eq!(copy_region(&with_whole, 2, whole), whole, "a whole frame among the missed ones");
        let still: VecDeque<_> = [Some(r(0, 0, 0, 0)), Some(r(5, 5, 2, 2))].into();
        assert_eq!(copy_region(&still, 1, whole).w, 0, "nothing changed: nothing to copy");
        assert_eq!(copy_region(&still, 2, whole), r(5, 5, 2, 2), "an empty frame adds nothing to the union");
    }
}
