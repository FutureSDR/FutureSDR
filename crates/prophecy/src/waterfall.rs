use futures::StreamExt;
use gloo_net::websocket::futures::WebSocket;
use gloo_net::websocket::Message;
use leptos::html::Canvas;
use leptos::logging::*;
use leptos::*;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys::WebGl2RenderingContext as GL;
use web_sys::WebGlProgram;

use crate::ArrayView;

pub enum WaterfallMode {
    Websocket(String),
    Data(Rc<RefCell<Option<Vec<u8>>>>),
}

impl Default for WaterfallMode {
    fn default() -> Self {
        Self::Websocket("ws://127.0.0.1:9001".to_string())
    }
}

struct RenderState {
    canvas: HtmlElement<Canvas>,
    gl: GL,
    shader: WebGlProgram,
    texture_offset: i32,
}

const SHADER_HEIGHT: usize = 256;

#[component]
/// Waterfall Sink
pub fn Waterfall(
    #[prop(into)] min: MaybeSignal<f32>,
    #[prop(into)] max: MaybeSignal<f32>,
    #[prop(optional)] mode: WaterfallMode,
) -> impl IntoView {
    let data = match mode {
        WaterfallMode::Data(d) => d,
        WaterfallMode::Websocket(s) => {
            let data = Rc::new(RefCell::new(None));
            {
                let data = data.clone();
                spawn_local(async move {
                    let mut ws = WebSocket::open(&s).unwrap();
                    while let Some(msg) = ws.next().await {
                        match msg {
                            Ok(Message::Bytes(b)) => {
                                *data.borrow_mut() = Some(b);
                            }
                            _ => {
                                log!("TimeSink: WebSocket {:?}", msg);
                            }
                        }
                    }
                    log!("TimeSink: WebSocket Closed");
                });
            }
            data
        }
    };

    let canvas_ref = create_node_ref::<Canvas>();
    canvas_ref.on_load(move |canvas_ref| {
        let _ = canvas_ref.on_mount(move |canvas| {
            let gl: GL = canvas
                .get_context("webgl2")
                .unwrap()
                .unwrap()
                .dyn_into()
                .unwrap();

            let vert_code = r"
                attribute vec2 gTexCoord0;
                varying vec2 coord;

                void main()
                {
                    gl_Position = vec4(gTexCoord0, 0, 1);
                    coord = gTexCoord0;
                }
            ";
            let vert_shader = gl.create_shader(GL::VERTEX_SHADER).unwrap();
            gl.shader_source(&vert_shader, vert_code);
            gl.compile_shader(&vert_shader);

            let frag_code = r"
                precision mediump float;

                varying vec2 coord;
                uniform float u_min;
                uniform float u_max;
                uniform float yoffset;
                uniform sampler2D frequency_data;

                vec3 color_map(float t) {
                    const vec3 c0 = vec3(0.2777273272234177, 0.005407344544966578, 0.3340998053353061);
                    const vec3 c1 = vec3(0.1050930431085774, 1.404613529898575, 1.384590162594685);
                    const vec3 c2 = vec3(-0.3308618287255563, 0.214847559468213, 0.09509516302823659);
                    const vec3 c3 = vec3(-4.634230498983486, -5.799100973351585, -19.33244095627987);
                    const vec3 c4 = vec3(6.228269936347081, 14.17993336680509, 56.69055260068105);
                    const vec3 c5 = vec3(4.776384997670288, -13.74514537774601, -65.35303263337234);
                    const vec3 c6 = vec3(-5.435455855934631, 4.645852612178535, 26.3124352495832);

                    return c0+t*(c1+t*(c2+t*(c3+t*(c4+t*(c5+t*c6)))));
                }

                void main()
                {
                    vec4 sample = texture2D(frequency_data, vec2(coord.x * 0.5 + 0.5, coord.y * 0.5 - 0.5 + yoffset));
                    float power = (10.0 * log(sample.r) / log(10.0) - u_min) / (u_max - u_min);
                    gl_FragColor = vec4(color_map(clamp(power, 0.0, 1.0)), 1.0);
                }
            ";

            let frag_shader = gl.create_shader(GL::FRAGMENT_SHADER).unwrap();
            gl.shader_source(&frag_shader, frag_code);
            gl.compile_shader(&frag_shader);

            let shader = gl.create_program().unwrap();
            gl.attach_shader(&shader, &vert_shader);
            gl.attach_shader(&shader, &frag_shader);
            gl.link_program(&shader);
            gl.use_program(Some(&shader));

            let texture = gl.create_texture().unwrap();
            gl.bind_texture(GL::TEXTURE_2D, Some(&texture));
            gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_WRAP_S, GL::REPEAT as i32);
            gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_WRAP_T, GL::REPEAT as i32);
            gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_MIN_FILTER, GL::NEAREST as i32);
            gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_MAG_FILTER, GL::NEAREST as i32);

            let fft_size = 1024_usize;
            initialize_texture(&gl, fft_size);

            let vertexes = [-1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0];
            let vertex_buffer = gl.create_buffer().unwrap();
            gl.bind_buffer(GL::ARRAY_BUFFER, Some(&vertex_buffer));
            let view = unsafe { f32::view(&vertexes) };
            gl.buffer_data_with_array_buffer_view(GL::ARRAY_BUFFER, &view, GL::STATIC_DRAW);

            let indices = [0, 1, 2, 0, 2, 3];
            let indices_buffer = gl.create_buffer().unwrap();
            gl.bind_buffer(GL::ELEMENT_ARRAY_BUFFER, Some(&indices_buffer));
            let view = unsafe { u16::view(&indices) };
            gl.buffer_data_with_array_buffer_view(GL::ELEMENT_ARRAY_BUFFER, &view, GL::STATIC_DRAW);

            let loc = gl.get_attrib_location(&shader, "gTexCoord0") as u32;
            gl.enable_vertex_attrib_array(loc);
            gl.vertex_attrib_pointer_with_i32(loc, 2, GL::FLOAT, false, 0, 0);

            {
                let gl = gl.clone();
                let shader = shader.clone();
                create_render_effect(move |_| {
                    let u_min = gl.get_uniform_location(&shader, "u_min");
                    gl.uniform1f(u_min.as_ref(), min.get());
                    let u_max = gl.get_uniform_location(&shader, "u_max");
                    gl.uniform1f(u_max.as_ref(), max.get());
                });
            }

            let state = RenderState {
                canvas,
                gl,
                shader,
                texture_offset: 0,
            };
            request_animation_frame(render(Rc::new(RefCell::new(state)), data, fft_size))
        });
    });

    view! { <canvas node_ref=canvas_ref style="width: 100%; height: 100%" /> }
}

fn render(
    state: Rc<RefCell<RenderState>>,
    data: Rc<RefCell<Option<Vec<u8>>>>,
    last_fft_size: usize,
) -> impl FnOnce() + 'static {
    move || {
        let mut fft_size_val = last_fft_size;
        {
            let RenderState {
                canvas,
                gl,
                shader,
                texture_offset,
            } = &mut (*state.borrow_mut());

            let display_width = canvas.client_width() as u32;
            let display_height = canvas.client_height() as u32;

            let need_resize = canvas.width() != display_width || canvas.height() != display_height;

            if need_resize {
                canvas.set_width(display_width);
                canvas.set_height(display_height);
                gl.viewport(0, 0, display_width as i32, display_height as i32);
            }

            if let Some(bytes) = data.borrow_mut().take() {
                // assert_eq!(bytes.len(), fft_size_val * 4);
                fft_size_val = bytes.len() / 4;
                if fft_size_val != last_fft_size {
                    initialize_texture(gl, fft_size_val);
                }

                let samples = unsafe {
                    let s = fft_size_val;
                    let p = bytes.as_ptr();
                    std::slice::from_raw_parts(p as *const f32, s)
                };

                let view = unsafe { f32::view(samples) };
                gl.tex_sub_image_2d_with_i32_and_i32_and_u32_and_type_and_array_buffer_view_and_src_offset(
                    GL::TEXTURE_2D,
                    0,
                    0,
                    *texture_offset,
                    fft_size_val as i32,
                    1,
                    GL::RED,
                    GL::FLOAT,
                    &view,
                    0,
                )
                .unwrap();

                let loc = gl.get_uniform_location(shader, "yoffset");
                gl.uniform1f(loc.as_ref(), *texture_offset as f32 / SHADER_HEIGHT as f32);
                *texture_offset = (*texture_offset + 1) % SHADER_HEIGHT as i32;

                gl.draw_elements_with_i32(GL::TRIANGLES, 6, GL::UNSIGNED_SHORT, 0);
            }
        }
        request_animation_frame(render(state, data, fft_size_val))
    }
}

fn initialize_texture(gl: &GL, fft_size: usize) {
    let texture = vec![0.0f32; fft_size * SHADER_HEIGHT];
    let view = unsafe { f32::view(&texture) };
    gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_array_buffer_view_and_src_offset(
        GL::TEXTURE_2D,
        0,
        GL::R32F as i32,
        fft_size as i32,
        SHADER_HEIGHT as i32,
        0,
        GL::RED,
        GL::FLOAT,
        &view,
        0,
    ).unwrap();
}
