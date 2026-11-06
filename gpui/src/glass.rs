// ERD 라이브 유리 — resin viewer/glass.ts 를 Metal 로. 셰이더는 glass.metal(실행할 때 컴파일한다).
// 한 프레임: 1) 무대를 반 크기 텍스처에(밉맵) 2) 화면 크기로 별, 격자, 결, 비네트 3) 카드마다 유리(겹쳐 칠하기)
// 4) GPUI 의 paint_surface 가 받는 YCbCr 4:2:0 두 판(IOSurface 를 품은 CVPixelBuffer)으로 옮긴다.
// 판은 풀에서 받는다 — GPUI 가 아직 들고 있는 판에 다시 그리지 않게. 다른 큐와 순서를 맞추려 끝날 때까지 기다린다.

use std::ffi::c_void;
use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_core_foundation::{CFBoolean, CFDictionary, CFNumber, CFRetained, CFType};
use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferGetIOSurface, CVPixelBufferPool, kCVPixelBufferHeightKey,
    kCVPixelBufferIOSurfacePropertiesKey, kCVPixelBufferMetalCompatibilityKey,
    kCVPixelBufferPixelFormatTypeKey, kCVPixelBufferWidthKey,
    kCVPixelFormatType_420YpCbCr8BiPlanarFullRange, kCVReturnSuccess,
};
use objc2_foundation::NSString;
use objc2_metal::{
    MTLBlendFactor, MTLBlitCommandEncoder, MTLBuffer, MTLClearColor, MTLCommandBuffer,
    MTLCommandEncoder, MTLCommandQueue, MTLCompileOptions, MTLCreateSystemDefaultDevice, MTLDevice,
    MTLLibrary, MTLLoadAction, MTLPixelFormat, MTLPrimitiveType, MTLRenderCommandEncoder,
    MTLRenderPassDescriptor, MTLRenderPipelineDescriptor, MTLRenderPipelineState,
    MTLResourceOptions, MTLStorageMode, MTLStoreAction, MTLTexture, MTLTextureDescriptor,
    MTLTextureUsage,
};

use amber_gpui::resin::svg::{LiveGlass, LiveStage};

const SHADERS: &str = include_str!("glass.metal");
/// 반짝이는 고리의 수(glass.ts RIPPLES)
pub const RIPPLES: usize = 4;

type Device = Retained<ProtocolObject<dyn MTLDevice>>;
type Pipe = Retained<ProtocolObject<dyn MTLRenderPipelineState>>;
type Tex = Retained<ProtocolObject<dyn MTLTexture>>;

/// 한 프레임에 필요한 것 — 크기는 CSS 픽셀(논리), 카드는 그림 좌표 [x, y, w, h, 흐림]
pub struct Frame<'a> {
    pub size: (f32, f32),
    pub dpr: f32,
    pub time: f32,
    /// 보이는 자리와 배율(그림 왼쪽 위의 판 안 자리)
    pub view: (f32, f32, f32),
    pub stage: &'a LiveStage,
    pub glass: &'a LiveGlass,
    /// 손 빛 — x, y, 밝기, 반지름
    pub light: [f32; 4],
    /// 고리 — x, y, 나이(초), 켜짐
    pub ripples: [[f32; 4]; RIPPLES],
    pub grid: bool,
    pub cards: &'a [[f32; 5]],
}

#[repr(C)]
struct StageU {
    size_pan: [f32; 4],
    time: [f32; 4],
    base: [f32; 4],
    c0: [f32; 4],
    c1: [f32; 4],
    c2: [f32; 4],
    k: [f32; 4],
    spot0: [f32; 4],
    spot1: [f32; 4],
    spot2: [f32; 4],
    light: [f32; 4],
    light_color: [f32; 4],
    ripple: [[f32; 4]; RIPPLES],
}

#[repr(C)]
struct ScreenU {
    size: [f32; 4],
    view: [f32; 4],
    grid: [f32; 4],
    light: [f32; 4],
    stars: [f32; 4],
    misc: [f32; 4],
}

#[repr(C)]
struct GlassU {
    size: [f32; 4],
    view: [f32; 4],
    veil: [f32; 4],
    tint: [f32; 4],
    rim: [f32; 4],
    shadow: [f32; 4],
    misc: [f32; 4],
    grid: [f32; 4],
}

fn v4(rgb: [f32; 3], a: f32) -> [f32; 4] {
    [rgb[0], rgb[1], rgb[2], a]
}

pub struct Renderer {
    device: Device,
    queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    stage_pipe: Pipe,
    screen_pipe: Pipe,
    glass_pipe: Pipe,
    luma_pipe: Pipe,
    chroma_pipe: Pipe,
    /// 무대(반 크기, 밉맵), 화면 색(장치 픽셀), 판 풀 — 크기가 바뀌면 다시
    stage: Option<((usize, usize), Tex)>,
    color: Option<((usize, usize), Tex)>,
    pool: Option<((usize, usize), CFRetained<CVPixelBufferPool>)>,
}

fn pipeline(
    device: &Device,
    library: &ProtocolObject<dyn MTLLibrary>,
    vs: &str,
    fs: &str,
    format: MTLPixelFormat,
    blend: bool,
) -> Result<Pipe, String> {
    let f = |name: &str| {
        library
            .newFunctionWithName(&NSString::from_str(name))
            .ok_or_else(|| format!("셰이더 함수 {name} 이 없다"))
    };
    let d = MTLRenderPipelineDescriptor::new();
    let (vf, ff) = (f(vs)?, f(fs)?);
    d.setVertexFunction(Some(&vf));
    d.setFragmentFunction(Some(&ff));
    // SAFETY: 0 번 색 칸은 늘 있다
    let att = unsafe { d.colorAttachments().objectAtIndexedSubscript(0) };
    att.setPixelFormat(format);
    if blend {
        // gl.blendFunc(ONE, ONE_MINUS_SRC_ALPHA) — 색과 알파 모두
        att.setBlendingEnabled(true);
        att.setSourceRGBBlendFactor(MTLBlendFactor::One);
        att.setSourceAlphaBlendFactor(MTLBlendFactor::One);
        att.setDestinationRGBBlendFactor(MTLBlendFactor::OneMinusSourceAlpha);
        att.setDestinationAlphaBlendFactor(MTLBlendFactor::OneMinusSourceAlpha);
    }
    device
        .newRenderPipelineStateWithDescriptor_error(&d)
        .map_err(|e| format!("파이프라인 {fs}: {e}"))
}

fn texture(
    device: &Device,
    format: MTLPixelFormat,
    w: usize,
    h: usize,
    mip: bool,
) -> Result<Tex, String> {
    // SAFETY: 크기는 0 이 아니다(부르는 쪽이 1 이상으로 맞춘다)
    let d = unsafe {
        MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
            format, w, h, mip,
        )
    };
    d.setUsage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
    d.setStorageMode(MTLStorageMode::Private);
    device
        .newTextureWithDescriptor(&d)
        .ok_or_else(|| "텍스처를 만들지 못했다".to_string())
}

/// 풀 — 420f, IOSurface, Metal 이 읽고 쓸 수 있게
fn make_pool(w: usize, h: usize) -> Result<CFRetained<CVPixelBufferPool>, String> {
    let empty: CFRetained<CFDictionary<CFType, CFType>> = CFDictionary::from_slices(&[], &[]);
    let fmt = CFNumber::new_i32(kCVPixelFormatType_420YpCbCr8BiPlanarFullRange as i32);
    let (wn, hn) = (CFNumber::new_i64(w as i64), CFNumber::new_i64(h as i64));
    // SAFETY: CoreVideo 의 키 상수는 프로세스 내내 살아 있다
    let keys: [&CFType; 5] = unsafe {
        [
            kCVPixelBufferPixelFormatTypeKey.as_ref(),
            kCVPixelBufferWidthKey.as_ref(),
            kCVPixelBufferHeightKey.as_ref(),
            kCVPixelBufferIOSurfacePropertiesKey.as_ref(),
            kCVPixelBufferMetalCompatibilityKey.as_ref(),
        ]
    };
    let values: [&CFType; 5] = [
        fmt.as_ref(),
        wn.as_ref(),
        hn.as_ref(),
        empty.as_ref(),
        CFBoolean::new(true).as_ref(),
    ];
    let attrs: CFRetained<CFDictionary<CFType, CFType>> = CFDictionary::from_slices(&keys, &values);
    let mut out: *mut CVPixelBufferPool = std::ptr::null_mut();
    // SAFETY: 속성은 CFString → CFType 사전이고 out 은 쓸 수 있는 자리다
    let r = unsafe {
        CVPixelBufferPool::create(None, None, Some(attrs.as_opaque()), NonNull::from(&mut out))
    };
    if r != kCVReturnSuccess || out.is_null() {
        return Err(format!("CVPixelBufferPool: {r}"));
    }
    // SAFETY: 방금 만든(+1) 풀이다
    Ok(unsafe { CFRetained::from_raw(NonNull::new_unchecked(out)) })
}

impl Renderer {
    /// Metal 이 없거나 셰이더가 깨지면 Err — 그러면 정지 그림을 쓴다(웹의 WebGL2 없음과 같다)
    pub fn new() -> Result<Renderer, String> {
        let device = MTLCreateSystemDefaultDevice().ok_or("Metal 장치가 없다")?;
        let queue = device.newCommandQueue().ok_or("명령 큐를 만들지 못했다")?;
        let library = device
            .newLibraryWithSource_options_error(
                &NSString::from_str(SHADERS),
                Some(&MTLCompileOptions::new()),
            )
            .map_err(|e| format!("셰이더: {e}"))?;
        let rgba = MTLPixelFormat::RGBA8Unorm;
        Ok(Renderer {
            stage_pipe: pipeline(&device, &library, "vs_screen", "fs_stage", rgba, false)?,
            screen_pipe: pipeline(&device, &library, "vs_screen", "fs_screen", rgba, false)?,
            glass_pipe: pipeline(&device, &library, "vs_glass", "fs_glass", rgba, true)?,
            luma_pipe: pipeline(
                &device,
                &library,
                "vs_screen",
                "fs_luma",
                MTLPixelFormat::R8Unorm,
                false,
            )?,
            chroma_pipe: pipeline(
                &device,
                &library,
                "vs_screen",
                "fs_chroma",
                MTLPixelFormat::RG8Unorm,
                false,
            )?,
            device,
            queue,
            stage: None,
            color: None,
            pool: None,
        })
    }

    fn sized(
        &mut self,
        stage: (usize, usize),
        color: (usize, usize),
    ) -> Result<(Tex, Tex), String> {
        if self.stage.as_ref().is_none_or(|(s, _)| *s != stage) {
            self.stage = Some((
                stage,
                texture(
                    &self.device,
                    MTLPixelFormat::RGBA8Unorm,
                    stage.0,
                    stage.1,
                    true,
                )?,
            ));
        }
        if self.color.as_ref().is_none_or(|(s, _)| *s != color) {
            self.color = Some((
                color,
                texture(
                    &self.device,
                    MTLPixelFormat::RGBA8Unorm,
                    color.0,
                    color.1,
                    false,
                )?,
            ));
            self.pool = Some((color, make_pool(color.0, color.1)?));
        }
        Ok((
            self.stage.as_ref().unwrap().1.clone(),
            self.color.as_ref().unwrap().1.clone(),
        ))
    }

    /// 한 프레임을 그려 판을 돌려준다
    pub fn render(&mut self, f: &Frame) -> Result<CFRetained<CVPixelBuffer>, String> {
        let (w, h, dpr, color_size, stage_size) = sizes(f);
        let (stage, color) = self.sized(stage_size, color_size)?;
        let pool = &self.pool.as_ref().unwrap().1;
        let mut out: *mut CVPixelBuffer = std::ptr::null_mut();
        // SAFETY: 풀은 살아 있고 out 은 쓸 수 있는 자리다
        let r =
            unsafe { CVPixelBufferPool::create_pixel_buffer(None, pool, NonNull::from(&mut out)) };
        if r != kCVReturnSuccess || out.is_null() {
            return Err(format!("CVPixelBufferPoolCreatePixelBuffer: {r}"));
        }
        // SAFETY: 방금 받은(+1) 판이다
        let buffer: CFRetained<CVPixelBuffer> =
            unsafe { CFRetained::from_raw(NonNull::new_unchecked(out)) };
        let surface = CVPixelBufferGetIOSurface(Some(&buffer)).ok_or("판에 IOSurface 가 없다")?;
        let plane =
            |format: MTLPixelFormat, pw: usize, ph: usize, index: usize| -> Result<Tex, String> {
                // SAFETY: 판의 크기와 꼴이 descriptor 와 같다
                let d = unsafe {
                    MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
                        format, pw, ph, false,
                    )
                };
                d.setUsage(MTLTextureUsage::RenderTarget);
                d.setStorageMode(MTLStorageMode::Shared);
                self.device
                    .newTextureWithDescriptor_iosurface_plane(&d, &surface, index)
                    .ok_or_else(|| format!("IOSurface 판 {index}"))
            };
        let luma = plane(MTLPixelFormat::R8Unorm, color_size.0, color_size.1, 0)?;
        let chroma = plane(
            MTLPixelFormat::RG8Unorm,
            color_size.0 / 2,
            color_size.1 / 2,
            1,
        )?;

        let cmd = self.queue.commandBuffer().ok_or("명령 버퍼")?;
        self.encode_color(&cmd, f, &stage, &color, (w, h, dpr))?;

        // 4. YCbCr 두 판
        draw_screen(
            &cmd,
            &luma,
            &self.luma_pipe,
            Some(&color),
            (NonNull::dangling(), 0),
        )?;
        draw_screen(
            &cmd,
            &chroma,
            &self.chroma_pipe,
            Some(&color),
            (NonNull::dangling(), 0),
        )?;
        cmd.commit();
        cmd.waitUntilCompleted();
        Ok(buffer)
    }

    /// 1~3(무대, 화면, 유리)을 color 에 — 판으로 옮기기 전의 RGB
    fn encode_color(
        &self,
        cmd: &ProtocolObject<dyn MTLCommandBuffer>,
        f: &Frame,
        stage: &Tex,
        color: &Tex,
        (w, h, dpr): (f32, f32, f32),
    ) -> Result<(), String> {
        let (x, y, scale) = f.view;
        let s = f.stage;
        let g = f.glass;
        let light_color = v4(s.light.0, 0.);

        // 1. 무대
        let stage_u = StageU {
            size_pan: [w, h, x * 0.3, y * 0.3],
            time: [f.time, s.style as f32, s.light.1, 0.],
            base: v4(s.base, 1.),
            c0: v4(s.colors[0].0, 1.),
            c1: v4(s.colors[1].0, 1.),
            c2: v4(s.colors[2].0, 1.),
            k: [s.colors[0].1, s.colors[1].1, s.colors[2].1, 0.],
            spot0: v4(s.spots[0], 0.),
            spot1: v4(s.spots[1], 0.),
            spot2: v4(s.spots[2], 0.),
            light: f.light,
            light_color,
            ripple: f.ripples,
        };
        draw_screen(&cmd, &stage, &self.stage_pipe, None, bytes(&stage_u))?;
        let blit = cmd.blitCommandEncoder().ok_or("블릿")?;
        blit.generateMipmapsForTexture(&stage);
        blit.endEncoding();

        // 2. 화면
        let grid = if f.grid {
            v4(s.grid.0, s.grid.1)
        } else {
            [0.; 4]
        };
        let screen_u = ScreenU {
            size: [w, h, dpr, f.time],
            view: [x, y, scale, if s.dark { 1. } else { 0. }],
            grid,
            light: f.light,
            stars: s.stars.map_or([0.; 4], |(c, a)| v4(c, a)),
            // 별은 멀다 — 패닝의 10분의 1 만 따라간다
            misc: [s.grain, s.vignette, x * 0.1, y * 0.1],
        };
        let rpd = pass(&color, MTLLoadAction::DontCare);
        let enc = cmd
            .renderCommandEncoderWithDescriptor(&rpd)
            .ok_or("화면 패스")?;
        enc.setRenderPipelineState(&self.screen_pipe);
        set_fragment_bytes(&enc, bytes(&screen_u));
        // SAFETY: 0 번 텍스처 자리
        unsafe { enc.setFragmentTexture_atIndex(Some(&stage), 0) };
        // SAFETY: 꼭짓점 셋은 vs_screen 이 만든다
        unsafe { enc.drawPrimitives_vertexStart_vertexCount(MTLPrimitiveType::Triangle, 0, 3) };

        // 3. 카드 밑 유리
        if !f.cards.is_empty() {
            let glass_u = GlassU {
                size: [
                    w,
                    h,
                    30. * scale + 26.,
                    amber_gpui::resin::svg::CARD_RADIUS as f32 * scale,
                ],
                view: [x, y, scale, g.lens * scale.sqrt()],
                veil: v4(g.veil.0, g.veil.1),
                tint: v4(g.tint.0, g.tint.1),
                rim: v4(g.rim.0, g.rim.1),
                shadow: v4(g.shadow.0, g.shadow.1),
                // 흐림 반지름(반 크기 무대의 텍셀) → 밉 단계
                misc: [
                    (g.frost * scale.sqrt() / 2.).max(1.).log2(),
                    if s.dark { 0.2 } else { 0. },
                    if s.dark { 1. } else { 0. },
                    0.,
                ],
                grid,
            };
            let cards: Vec<[f32; 8]> = f
                .cards
                .iter()
                .map(|c| [c[0], c[1], c[2], c[3], c[4], 0., 0., 0.])
                .collect();
            // SAFETY: 카드 배열을 그대로 복사한다
            let buf = unsafe {
                self.device.newBufferWithBytes_length_options(
                    NonNull::new(cards.as_ptr() as *mut c_void).unwrap(),
                    std::mem::size_of_val(cards.as_slice()),
                    MTLResourceOptions::StorageModeShared,
                )
            }
            .ok_or("카드 버퍼")?;
            enc.setRenderPipelineState(&self.glass_pipe);
            let gb = bytes(&glass_u);
            // SAFETY: 크기와 자리가 셰이더의 GlassU, Card 와 같다
            unsafe {
                enc.setVertexBytes_length_atIndex(gb.0, gb.1, 0);
                enc.setVertexBuffer_offset_atIndex(Some(&buf), 0, 1);
                enc.setFragmentBytes_length_atIndex(gb.0, gb.1, 0);
                enc.setFragmentTexture_atIndex(Some(&stage), 0);
                enc.drawPrimitives_vertexStart_vertexCount_instanceCount(
                    MTLPrimitiveType::TriangleStrip,
                    0,
                    4,
                    f.cards.len(),
                );
            }
        }
        enc.endEncoding();
        Ok(())
    }

    /// 판으로 옮기기 전의 RGBA(장치 픽셀, 위에서 아래로) — WebGL 판과 견주는 시험이 읽는다
    pub fn render_rgba(&mut self, f: &Frame) -> Result<(usize, usize, Vec<u8>), String> {
        let (w, h, dpr, color_size, stage_size) = sizes(f);
        let (stage, color) = self.sized(stage_size, color_size)?;
        let cmd = self.queue.commandBuffer().ok_or("명령 버퍼")?;
        self.encode_color(&cmd, f, &stage, &color, (w, h, dpr))?;
        let (cw, ch) = color_size;
        let row = cw * 4;
        let buf = self
            .device
            .newBufferWithLength_options(row * ch, MTLResourceOptions::StorageModeShared)
            .ok_or("읽기 버퍼")?;
        let blit = cmd.blitCommandEncoder().ok_or("블릿")?;
        // SAFETY: 크기와 자리가 텍스처와 버퍼 안이다
        unsafe {
            blit.copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toBuffer_destinationOffset_destinationBytesPerRow_destinationBytesPerImage(
                &color,
                0,
                0,
                objc2_metal::MTLOrigin { x: 0, y: 0, z: 0 },
                objc2_metal::MTLSize { width: cw, height: ch, depth: 1 },
                &buf,
                0,
                row,
                row * ch,
            );
        }
        blit.endEncoding();
        cmd.commit();
        cmd.waitUntilCompleted();
        // SAFETY: 끝까지 기다렸고 버퍼는 공유 메모리다
        let bytes =
            unsafe { std::slice::from_raw_parts(buf.contents().as_ptr() as *const u8, row * ch) };
        Ok((cw, ch, bytes.to_vec()))
    }
}

/// 판 크기 — CSS 픽셀, dpr, 색 판(장치 픽셀, 4:2:0 이라 짝수), 무대(반 크기)
fn sizes(f: &Frame) -> (f32, f32, f32, (usize, usize), (usize, usize)) {
    let (w, h) = (f.size.0.max(1.), f.size.1.max(1.));
    let dpr = f.dpr.clamp(1., 2.);
    let even = |v: f32| (((v * dpr).round() as usize).max(2) + 1) & !1;
    let stage = (
        (w / 2.).ceil().max(1.) as usize,
        (h / 2.).ceil().max(1.) as usize,
    );
    (w, h, dpr, (even(w), even(h)), stage)
}

fn bytes<T>(v: &T) -> (NonNull<c_void>, usize) {
    (NonNull::from(v).cast(), std::mem::size_of::<T>())
}

fn pass(
    target: &ProtocolObject<dyn MTLTexture>,
    load: MTLLoadAction,
) -> Retained<MTLRenderPassDescriptor> {
    let rpd = MTLRenderPassDescriptor::new();
    // SAFETY: 0 번 색 칸
    let att = unsafe { rpd.colorAttachments().objectAtIndexedSubscript(0) };
    att.setTexture(Some(target));
    att.setLoadAction(load);
    att.setClearColor(MTLClearColor {
        red: 0.,
        green: 0.,
        blue: 0.,
        alpha: 1.,
    });
    att.setStoreAction(MTLStoreAction::Store);
    rpd
}

fn set_fragment_bytes(
    enc: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    b: (NonNull<c_void>, usize),
) {
    if b.1 > 0 {
        // SAFETY: b 는 이 패스가 끝날 때까지 살아 있는 값을 가리킨다(Metal 이 바로 복사한다)
        unsafe { enc.setFragmentBytes_length_atIndex(b.0, b.1, 0) };
    }
}

/// 화면을 덮는 삼각형 하나로 target 을 그린다(src 는 0 번 텍스처)
fn draw_screen(
    cmd: &ProtocolObject<dyn MTLCommandBuffer>,
    target: &ProtocolObject<dyn MTLTexture>,
    pipe: &ProtocolObject<dyn MTLRenderPipelineState>,
    src: Option<&ProtocolObject<dyn MTLTexture>>,
    uniforms: (NonNull<c_void>, usize),
) -> Result<(), String> {
    let rpd = pass(target, MTLLoadAction::DontCare);
    let enc = cmd.renderCommandEncoderWithDescriptor(&rpd).ok_or("패스")?;
    enc.setRenderPipelineState(pipe);
    set_fragment_bytes(&enc, uniforms);
    if let Some(src) = src {
        // SAFETY: 0 번 텍스처 자리
        unsafe { enc.setFragmentTexture_atIndex(Some(src), 0) };
    }
    // SAFETY: 꼭짓점 셋은 vs_screen 이 만든다
    unsafe { enc.drawPrimitives_vertexStart_vertexCount(MTLPrimitiveType::Triangle, 0, 3) };
    enc.endEncoding();
    Ok(())
}

// ---- 움직임(glass.ts 의 LiveGlass 에서 DOM 과 GL 을 뺀 것) ----

/// 빛이 손을 따라잡는 빠르기, 밝아지고 어두워지는 빠르기(초당) — 프레임이 아니라 시간으로 재서 60, 120Hz 가 같다
const FOLLOW: f32 = 9.;
const GLOW: f32 = 5.;
/// 흐려진 표의 유리는 카드와 같이 흐려진다 — 같은 150ms, CSS 의 ease
const DIM_MS: f32 = 150.;
pub const DIMMED: f32 = 0.14;
/// 고리가 사는 시간
const RIPPLE_MS: u128 = 2600;

/// CSS `ease`(0.25, 0.1, 0.25, 1) — 카드가 흐려지는 곡선
fn ease(t: f32) -> f32 {
    let (x1, y1, x2, y2) = (0.25_f32, 0.1_f32, 0.25_f32, 1_f32);
    let b = |a: f32, c: f32, s: f32| {
        let u = 1. - s;
        3. * u * u * s * a + 3. * u * s * s * c + s * s * s
    };
    let (mut lo, mut hi) = (0_f32, 1_f32);
    for _ in 0..24 {
        let m = (lo + hi) / 2.;
        if b(x1, x2, m) < t {
            lo = m;
        } else {
            hi = m;
        }
    }
    b(y1, y2, (lo + hi) / 2.)
}

pub struct Live {
    renderer: Renderer,
    /// 마지막으로 그린 판 — 판의 paint_surface 가 쓴다
    pub buffer: Option<CFRetained<CVPixelBuffer>>,
    time: f32,
    last: Option<std::time::Instant>,
    last_input: std::time::Instant,
    target: (f32, f32),
    pos: (f32, f32),
    glow: f32,
    glow_target: f32,
    /// 손을 떼면 빛이 위 가운데로 — 손이 판 위에 있나
    pointed: bool,
    ripples: Vec<(f32, f32, std::time::Instant)>,
    dims: Vec<f32>,
    dim_from: Vec<f32>,
    dim_to: Vec<f32>,
    dim_start: std::time::Instant,
    skip: bool,
}

/// 한 프레임의 바깥 사정
pub struct Scene<'a> {
    pub size: (f32, f32),
    pub dpr: f32,
    pub view: (f32, f32, f32),
    /// 보기가 움직이는 중(맞춤, 확대 단추 등의 움직임)
    pub moving: bool,
    pub look: &'a str,
    pub grid: bool,
    /// 그림 좌표의 카드 [x, y, w, h]
    pub cards: &'a [[f32; 4]],
    /// 카드마다 흐려야 하나(초점) — None 이면 아무것도 흐리지 않는다
    pub dimmed: Option<&'a [bool]>,
    /// 움직임 줄이기
    pub still: bool,
}

impl Live {
    pub fn new() -> Result<Live, String> {
        let now = std::time::Instant::now();
        Ok(Live {
            renderer: Renderer::new()?,
            buffer: None,
            time: 0.,
            last: None,
            last_input: now,
            target: (0., 0.),
            pos: (0., 0.),
            glow: 0.5,
            glow_target: 0.5,
            pointed: false,
            ripples: Vec::new(),
            dims: Vec::new(),
            dim_from: Vec::new(),
            dim_to: Vec::new(),
            dim_start: now,
            skip: false,
        })
    }

    /// 손이 판 위에서 움직였다(판 좌표)
    pub fn point(&mut self, x: f32, y: f32) {
        self.target = (x, y);
        self.glow_target = 1.;
        self.pointed = true;
        self.last_input = std::time::Instant::now();
    }

    /// 손이 떠났다 — 빛은 위 가운데에 머물고 조금 어두워진다
    pub fn rest(&mut self) {
        self.pointed = false;
    }

    /// 누른 자리에서 무대로 퍼지는 고리(움직임 줄이기면 없다)
    pub fn ripple(&mut self, x: f32, y: f32, still: bool) {
        if still {
            return;
        }
        let now = std::time::Instant::now();
        if self.ripples.len() >= RIPPLES {
            self.ripples.remove(0);
        }
        self.ripples.push((x, y, now));
        self.last_input = now;
    }

    /// 보기를 움직였다 — 잠깐 쉬던 무대가 다시 제 빠르기로
    pub fn touched(&mut self) {
        self.last_input = std::time::Instant::now();
    }

    /// 흐릴 표가 바뀌었으면 150ms 에 걸쳐(새 그림이면 바로)
    fn upload(&mut self, to: Vec<f32>, still: bool, now: std::time::Instant) {
        if to == self.dim_to {
            return;
        }
        let fresh = to.len() != self.dims.len();
        self.dim_from = if fresh || still {
            to.clone()
        } else {
            self.dims.clone()
        };
        self.dims = self.dim_from.clone();
        self.dim_to = to;
        self.dim_start = now;
    }

    /// 흐림을 한 걸음 — 아직 바뀌는 중이면 true
    fn fade(&mut self, now: std::time::Instant) -> bool {
        if self.dims == self.dim_to {
            return false;
        }
        let t = ((now - self.dim_start).as_secs_f32() * 1000. / DIM_MS).min(1.);
        let e = ease(t);
        self.dims = self
            .dim_to
            .iter()
            .zip(&self.dim_from)
            .map(|(to, from)| from + (to - from) * e)
            .collect();
        if t >= 1. {
            self.dims = self.dim_to.clone();
        }
        t < 1.
    }

    /// 한 프레임(glass.ts tick) — 시간, 빛, 흐림, 고리를 나아가게 하고 필요하면 그린다. 더 그려야 하면 true
    pub fn tick(&mut self, s: &Scene) -> bool {
        let Some((stage, glass)) = amber_gpui::resin::svg::live_of(s.look) else {
            return false;
        };
        let now = std::time::Instant::now();
        let dt = self.last.map_or(0., |l| (now - l).as_secs_f32().min(0.05));
        self.last = Some(now);
        if !self.pointed {
            self.target = (s.size.0 * 0.5, s.size.1 * 0.12);
            self.glow_target = 0.5;
        }
        // 한동안 손대지 않으면 무대는 계속 흐르되 반 빠르기로 그린다
        let idle = (now - self.last_input).as_millis() > 4000;
        self.skip = if idle && !s.still { !self.skip } else { false };
        if !s.still {
            self.time += dt;
        }
        let k = if s.still {
            1.
        } else {
            1. - (-FOLLOW * dt).exp()
        };
        self.pos.0 += (self.target.0 - self.pos.0) * k;
        self.pos.1 += (self.target.1 - self.pos.1) * k;
        let kg = if s.still { 1. } else { 1. - (-GLOW * dt).exp() };
        self.glow += (self.glow_target - self.glow) * kg;
        let to: Vec<f32> = (0..s.cards.len())
            .map(|i| match s.dimmed {
                Some(d) if d.get(i) == Some(&true) => DIMMED,
                _ => 1.,
            })
            .collect();
        self.upload(to, s.still, now);
        let fading = self.fade(now);
        self.ripples.retain(|r| (now - r.2).as_millis() < RIPPLE_MS);
        if !self.skip || s.moving || fading || self.buffer.is_none() {
            let mut ripples = [[0.; 4]; RIPPLES];
            for (i, r) in self.ripples.iter().enumerate() {
                ripples[i] = [r.0, r.1, (now - r.2).as_secs_f32(), 1.];
            }
            let cards: Vec<[f32; 5]> = s
                .cards
                .iter()
                .zip(self.dims.iter().chain(std::iter::repeat(&1.)))
                .map(|(c, d)| [c[0], c[1], c[2], c[3], *d])
                .collect();
            match self.renderer.render(&Frame {
                size: s.size,
                dpr: s.dpr,
                time: self.time,
                view: s.view,
                stage: &stage,
                glass: &glass,
                light: [self.pos.0, self.pos.1, self.glow, stage.light.2],
                ripples,
                grid: s.grid,
                cards: &cards,
            }) {
                Ok(b) => self.buffer = Some(b),
                Err(e) => eprintln!("유리: {e}"),
            }
        }
        // 무대는 혼자 흐른다. 움직임 줄이기면 무엇이 바뀔 때만 다시 그린다
        let settling = (self.target.0 - self.pos.0).abs() > 0.3
            || (self.target.1 - self.pos.1).abs() > 0.3
            || (self.glow_target - self.glow).abs() > 0.003;
        !s.still || settling || fading || s.moving || !self.ripples.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::{Frame, RIPPLES, Renderer};
    use objc2_core_video::{CVPixelBufferGetHeight, CVPixelBufferGetWidth};

    /// 셰이더는 실행할 때 컴파일된다 — 여섯 룩 모두 한 프레임씩 그려 본다(판 크기는 장치 픽셀, 짝수)
    #[test]
    fn shaders_compile_and_every_look_draws() {
        let mut r = Renderer::new().expect("Metal");
        for look in [
            "aurora-dark",
            "aurora-light",
            "silk-dark",
            "silk-light",
            "caustic-dark",
            "caustic-light",
        ] {
            let (stage, glass) = amber_gpui::resin::svg::live_of(look).unwrap();
            let buf = r
                .render(&Frame {
                    size: (301., 200.),
                    dpr: 2.,
                    time: 3.5,
                    view: (20., 10., 1.),
                    stage: &stage,
                    glass: &glass,
                    light: [150., 40., 1., 420.],
                    ripples: [[100., 100., 0.4, 1.]; RIPPLES],
                    grid: true,
                    cards: &[[10., 10., 120., 80., 1.], [160., 30., 100., 90., 0.14]],
                })
                .expect(look);
            assert_eq!(
                (CVPixelBufferGetWidth(&buf), CVPixelBufferGetHeight(&buf)),
                (602, 400)
            );
        }
        assert!(amber_gpui::resin::svg::live_of("graphite").is_none());
    }

    /// resin 의 WebGL 판(tools/glass-ref.mjs 가 브라우저에서 그린 8x8 칸 평균)과 견준다 — 룩 값도, 그림도
    #[test]
    fn matches_webgl_reference() {
        use serde_json::Value;
        let frames: Vec<Value> =
            serde_json::from_str(include_str!("../tests/glass_ref.json")).unwrap();
        let num = |v: &Value| v.as_f64().unwrap() as f32;
        let nums = |v: &Value| v.as_array().unwrap().iter().map(num).collect::<Vec<_>>();
        let hex = |s: &str| {
            let n = u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap();
            [n >> 16, (n >> 8) & 255, n & 255].map(|c| c as f32 / 255.)
        };
        let ink = |v: &Value| (hex(v[0].as_str().unwrap()), num(&v[1]));
        let mut r = Renderer::new().expect("Metal");
        let mut report = Vec::new();
        for f in &frames {
            let look = f["look"].as_str().unwrap();
            let (stage, glass) = amber_gpui::resin::svg::live_of(look).unwrap();
            // 룩 값이 resin 의 stageOf/glassOf 와 같다
            let (s, g) = (&f["stage"], &f["glass"]);
            assert_eq!(stage.base, hex(s["base"].as_str().unwrap()), "{look} base");
            for i in 0..3 {
                assert_eq!(stage.colors[i], ink(&s["colors"][i]), "{look} colors");
                assert_eq!(
                    stage.spots[i].to_vec(),
                    nums(&s["spots"][i]),
                    "{look} spots"
                );
            }
            let l = &s["light"];
            assert_eq!(
                stage.light,
                (hex(l[0].as_str().unwrap()), num(&l[1]), num(&l[2])),
                "{look} light"
            );
            assert_eq!(stage.grid, ink(&s["grid"]), "{look} grid");
            assert_eq!(stage.stars, s["stars"].as_array().map(|_| ink(&s["stars"])));
            assert_eq!(
                (stage.grain, stage.vignette),
                (num(&s["grain"]), num(&s["vignette"]))
            );
            assert_eq!(
                (glass.frost, glass.lens),
                (num(&g["frost"]), num(&g["lens"])),
                "{look} frost"
            );
            for k in ["veil", "tint", "rim", "shadow"] {
                let have = match k {
                    "veil" => glass.veil,
                    "tint" => glass.tint,
                    "rim" => glass.rim,
                    _ => glass.shadow,
                };
                assert_eq!(have, ink(&g[k]), "{look} {k}");
            }

            let size = nums(&f["size"]);
            let view = nums(&f["view"]);
            let light = nums(&f["light"]);
            let mut ripples = [[0.; 4]; RIPPLES];
            for (i, rp) in f["ripples"].as_array().unwrap().iter().enumerate() {
                let rp = nums(rp);
                ripples[i] = [rp[0], rp[1], rp[2], 1.];
            }
            let cards: Vec<[f32; 5]> = f["cards"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| nums(c).try_into().unwrap())
                .collect();
            let (w, h, px) = r
                .render_rgba(&Frame {
                    size: (size[0], size[1]),
                    dpr: 1.,
                    time: num(&f["time"]),
                    view: (view[0], view[1], view[2]),
                    stage: &stage,
                    glass: &glass,
                    light: [light[0], light[1], light[2], stage.light.2],
                    ripples,
                    grid: f["grid"].as_bool().unwrap(),
                    cards: &cards,
                })
                .expect(look);
            assert_eq!((w, h), (size[0] as usize, size[1] as usize));
            let want: Vec<f32> = f["blocks"]
                .as_str()
                .unwrap()
                .as_bytes()
                .chunks(2)
                .map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap() as f32)
                .collect();
            const B: usize = 8;
            let (mut worst, mut sum, mut n) = (0f32, 0f32, 0usize);
            let mut at = (0, 0, 0);
            for by in 0..h / B {
                for bx in 0..w / B {
                    for c in 0..3 {
                        let mut acc = 0u32;
                        for yy in 0..B {
                            for xx in 0..B {
                                acc += px[((by * B + yy) * w + bx * B + xx) * 4 + c] as u32;
                            }
                        }
                        let have = acc as f32 / (B * B) as f32;
                        let d = (have - want[(by * (w / B) + bx) * 3 + c]).abs();
                        if d > worst {
                            (worst, at) = (d, (bx, by, c));
                        }
                        sum += d;
                        n += 1;
                    }
                }
            }
            report.push((look, sum / n as f32, worst, at));
        }
        for (look, mean, worst, at) in &report {
            eprintln!("{look:14} mean {mean:5.2}  worst {worst:5.1} at {at:?}");
        }
        for (look, mean, worst, _) in &report {
            // 기준 칸은 정수로 반올림돼 있다 — 0.5 안팎이면 같은 그림이다
            assert!(
                *mean < 0.5 && *worst < 2.,
                "{look}: mean {mean}, worst {worst}"
            );
        }
    }
}
