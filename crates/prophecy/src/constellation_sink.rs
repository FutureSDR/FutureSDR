use futures::StreamExt;
use gloo_net::websocket::{futures::WebSocket, Message};
use leptos::html::Canvas;
use leptos::logging::*;
use leptos::*;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys::WebGlProgram;
use web_sys::WebGlRenderingContext as GL;

struct RenderState {
    canvas: HtmlElement<Canvas>,
    gl: GL,
    shader: WebGlProgram,
    vertex_len: i32,
}

#[component]
pub fn ConstellationSink(
    #[prop(into)] width: MaybeSignal<f32>,
    #[prop(optional, into, default = "ws://127.0.0.1:9002".to_string())] websocket: String,
) -> impl IntoView {
    let data = Rc::new(RefCell::new(None));
    {
        let data = data.clone();
        spawn_local(async move {
            let mut ws = WebSocket::open(&websocket).unwrap();
            while let Some(msg) = ws.next().await {
                match msg {
                    Ok(Message::Bytes(b)) => {
                        *data.borrow_mut() = Some(b);
                    }
                    _ => {
                        log!("ConstellationSink: WebSocket {:?}", msg);
                    }
                }
            }
            log!("ConstellationSink: WebSocket Closed");
        });
    }

    let canvas_ref = create_node_ref::<Canvas>();
    canvas_ref.on_load(move |canvas_ref| {
        let _ = canvas_ref.on_mount(move |canvas| {
            let gl: GL = canvas
                .get_context("webgl")
                .unwrap()
                .unwrap()
                .dyn_into()
                .unwrap();

            let vert_code = r"
                attribute vec2 coordinates;
                uniform float u_width;

                void main(void) {
                    float x = coordinates.x / u_width;
                    float y = coordinates.y / u_width;
                    gl_Position = vec4(x, y, 0.0, 1.0);
                    gl_PointSize = 10.0;
                }
            ";

            let vert_shader = gl.create_shader(GL::VERTEX_SHADER).unwrap();
            gl.shader_source(&vert_shader, vert_code);
            gl.compile_shader(&vert_shader);

            let frag_code = r"
                precision mediump float;

                void main(void) {
                    gl_FragColor = vec4(0, 0.5, 0.5, 0.4);
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

            {
                let gl = gl.clone();
                let shader = shader.clone();
                create_render_effect(move |_| {
                    let u_min = gl.get_uniform_location(&shader, "u_width");
                    gl.uniform1f(u_min.as_ref(), width());
                });
            }

            let vertex_buffer = gl.create_buffer().unwrap();
            gl.bind_buffer(GL::ARRAY_BUFFER, Some(&vertex_buffer));

            let state = Rc::new(RefCell::new(RenderState {
                canvas, gl, shader, vertex_len: 0
            }));
            request_animation_frame(render(state, data))
        });
    });

    view! {
        <canvas node_ref=canvas_ref style="width: 100%; height: 100%" />
    }
}

fn render(
    state: Rc<RefCell<RenderState>>,
    data: Rc<RefCell<Option<Vec<u8>>>>,
) -> impl FnOnce() + 'static {
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

            if let Some(bytes) = data.borrow_mut().take() {
                gl.buffer_data_with_u8_array(GL::ARRAY_BUFFER, &bytes, GL::DYNAMIC_DRAW);

                *vertex_len = bytes.len() as i32 / 8;
            };

            let position = gl.get_attrib_location(shader, "coordinates") as u32;
            gl.vertex_attrib_pointer_with_i32(position, 2, GL::FLOAT, false, 0, 0);
            gl.enable_vertex_attrib_array(position);
            gl.draw_arrays(GL::POINTS, 0, *vertex_len);
        }
        request_animation_frame(render(state, data))
    }
}
