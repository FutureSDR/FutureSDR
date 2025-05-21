use futures::StreamExt;
use gloo_net::websocket::futures::WebSocket;
use gloo_net::websocket::Message;
use leptos::html::Canvas;
use leptos::logging::*;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::wasm_bindgen::JsCast;
use std::cell::RefCell;
use std::rc::Rc;
use web_sys::HtmlCanvasElement;
use web_sys::WebGl2RenderingContext as GL;
use web_sys::WebGlProgram;

use crate::ArrayView;

const MAX_SAMPLES: usize = 4096;

pub enum TimeSinkMode {
    Websocket(String),
    Data(ReadSignal<Vec<u8>>),
}

impl Default for TimeSinkMode {
    fn default() -> Self {
        Self::Websocket("ws://127.0.0.1:9001".to_string())
    }
}

struct RenderState {
    canvas: HtmlCanvasElement,
    gl: GL,
    shader: WebGlProgram,
    vertex_len: i32,
}

#[component]
/// Time Sink
pub fn TimeSink(
    #[prop(into)] min: Signal<f32>,
    #[prop(into)] max: Signal<f32>,
    #[prop(optional)] mode: TimeSinkMode,
) -> impl IntoView {
    let data = match mode {
        TimeSinkMode::Data(d) => d,
        TimeSinkMode::Websocket(s) => {
            let (data, set_data) = signal(vec![]);
            {
                spawn_local(async move {
                    let mut ws = WebSocket::open(&s).unwrap();
                    while let Some(msg) = ws.next().await {
                        match msg {
                            Ok(Message::Bytes(b)) => {
                                set_data(b);
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

    let canvas_ref = NodeRef::<Canvas>::new();
    Effect::new(move || {
        if let Some(canvas) = canvas_ref.get() {
            let gl: GL = canvas
                .get_context("webgl2")
                .unwrap()
                .unwrap()
                .dyn_into()
                .unwrap();

            let vert_code = r"
                attribute vec2 coordinates;
                uniform float u_nsamples;
                uniform float u_min;
                uniform float u_max;
                varying float power;

                void main(void) {
                    float x = -1.0 + 2.0 * coordinates.x / u_nsamples;
                    power = (10.0 * log(coordinates.y) / log(10.0) - u_min) / (u_max - u_min);
                    float y = 2.0 * power - 1.0;
                    gl_Position = vec4(x, y, 0.0, 1.0);
                }
            ";

            let vert_shader = gl.create_shader(GL::VERTEX_SHADER).unwrap();
            gl.shader_source(&vert_shader, vert_code);
            gl.compile_shader(&vert_shader);

            let frag_code = r"
                precision mediump float;
                varying float power;

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

                void main(void) {
                    gl_FragColor = vec4(color_map(clamp(power, 0.0, 1.0)), 0.9);
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

            let u_min = gl.get_uniform_location(&shader, "u_min");
            gl.uniform1f(u_min.as_ref(), min.get());
            let u_max = gl.get_uniform_location(&shader, "u_max");
            gl.uniform1f(u_max.as_ref(), max.get());

            let vertex_buffer = gl.create_buffer().unwrap();
            let init_data = [0.0f32; MAX_SAMPLES * 2];
            let view = unsafe { f32::view(&init_data) };
            gl.bind_buffer(GL::ARRAY_BUFFER, Some(&vertex_buffer));
            gl.buffer_data_with_array_buffer_view(GL::ARRAY_BUFFER, &view, GL::DYNAMIC_DRAW);

            let position = gl.get_attrib_location(&shader, "coordinates") as u32;
            gl.vertex_attrib_pointer_with_i32(position, 2, GL::FLOAT, false, 0, 0);
            gl.enable_vertex_attrib_array(position);

            let state = Rc::new(RefCell::new(RenderState {
                canvas,
                gl,
                shader,
                vertex_len: 0,
            }));
            request_animation_frame(render(state, data))
        }
    });

    view! { <canvas node_ref=canvas_ref style="width: 100%; height: 100%" /> }
}

fn render(state: Rc<RefCell<RenderState>>, data: ReadSignal<Vec<u8>>) -> impl FnOnce() + 'static {
    move || {
        {
            let RenderState {
                canvas,
                gl,
                shader,
                vertex_len,
            } = &mut (*state.borrow_mut());

            let display_width = canvas.client_width() as u32;
            let display_height = canvas.client_height() as u32;

            let need_resize = canvas.width() != display_width || canvas.height() != display_height;

            if need_resize {
                canvas.set_width(display_width);
                canvas.set_height(display_height);
                gl.viewport(0, 0, display_width as i32, display_height as i32);
            }

            let bytes = &*data.read_untracked();
            if !bytes.is_empty() {
                let samples = unsafe {
                    let s = std::cmp::min(bytes.len() / 4, MAX_SAMPLES);
                    let p = bytes.as_ptr();
                    std::slice::from_raw_parts(p as *const f32, s)
                };

                let l = samples.len();
                let vertices: Vec<f32> = samples
                    .iter()
                    .enumerate()
                    .flat_map(|(i, v)| vec![i as f32, *v])
                    .collect();

                let view = unsafe { f32::view(&vertices) };
                gl.buffer_sub_data_with_i32_and_array_buffer_view(GL::ARRAY_BUFFER, 0, &view);

                let u_nsamples = gl.get_uniform_location(shader, "u_nsamples");
                gl.uniform1f(u_nsamples.as_ref(), l as f32);

                *vertex_len = l as i32;

                gl.draw_arrays(GL::LINE_STRIP, 0, *vertex_len);
            }
        }
        request_animation_frame(render(state, data))
    }
}
