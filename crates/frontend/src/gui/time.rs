//! Time domain plot
use futures::StreamExt;
use gloo_render::request_animation_frame;
use gloo_render::AnimationFrame;
use rbl_circular_buffer::CircularBuffer;
use reqwasm::websocket::futures::WebSocket;
use reqwasm::websocket::Message;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlCanvasElement;
use web_sys::WebGlRenderingContext as GL;
use yew::prelude::*;

#[doc(hidden)]
pub enum Msg {
    Data(Vec<u8>),
    Status(String),
    Render(f64),
}

/// Mount a time domain plot to the website
///
/// ## Parameter
/// - `id`: HTML ID of component
/// - `url`: URL of websocket that streams data
/// - `min`: min value for scaling the y-axis
/// - `max`: max value for scaling the y-axis
#[wasm_bindgen]
pub fn add_time(id: String, url: String, min: f32, max: f32) {
    let document = gloo_utils::document();
    let div = document.query_selector(&id).unwrap().unwrap();
    yew::start_app_with_props_in_element::<Time>(div, Props { url, min, max });
}

#[doc(hidden)]
#[derive(Clone, Properties, Default, PartialEq)]
pub struct Props {
    pub url: String,
    pub min: f32,
    pub max: f32,
}

/// Time domain plot
pub struct Time {
    canvas_ref: NodeRef,
    _canvas: Option<HtmlCanvasElement>,
    gl: Option<GL>,
    _render_loop: Option<AnimationFrame>,
    buff: CircularBuffer<f32>,
}

impl Component for Time {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let link = ctx.link().clone();
        let url = ctx.props().url.clone();

        spawn_local(async move {
            let websocket = WebSocket::open(&url).unwrap();
            let (_, mut rx) = websocket.split();

            while let Some(msg) = rx.next().await {
                match msg {
                    Ok(Message::Text(s)) => link.send_message(Msg::Status(s)),
                    Ok(Message::Bytes(v)) => link.send_message(Msg::Data(v)),
                    _ => break,
                }
            }
        });

        Self {
            canvas_ref: NodeRef::default(),
            _canvas: None,
            gl: None,
            _render_loop: None,
            buff: CircularBuffer::new(2048),
        }
    }

    fn rendered(&mut self, ctx: &Context<Self>, first_render: bool) {
        let canvas = self.canvas_ref.cast::<HtmlCanvasElement>().unwrap();

        let gl: GL = canvas
            .get_context("webgl")
            .unwrap()
            .unwrap()
            .dyn_into()
            .unwrap();

        self._canvas = Some(canvas);
        self.gl = Some(gl);

        if first_render {
            let handle = {
                let link = ctx.link().clone();
                request_animation_frame(move |time| link.send_message(Msg::Render(time)))
            };
            self._render_loop = Some(handle);
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Render(timestamp) => {
                self.render_gl(timestamp, ctx);
            }
            Msg::Data(b) => {
                let v;
                unsafe {
                    let s = b.len() / 4;
                    let p = b.as_ptr();
                    v = std::slice::from_raw_parts(p as *const f32, s);
                }
                for i in v {
                    self.buff.push(*i);
                }
            }
            Msg::Status(s) => {
                gloo_console::log!(format!("socket status {:?}", &s));
            }
        }
        true
    }

    fn view(&self, _ctx: &Context<Self>) -> Html {
        html! {
            <canvas ref={self.canvas_ref.clone()} />
        }
    }
}

impl Time {
    fn render_gl(&mut self, timestamp: f64, ctx: &Context<Self>) {
        let gl = self.gl.as_ref().expect("GL Context not initialized!");

        let l = self.buff.len();
        let min = ctx.props().min;
        let max = ctx.props().max;
        let vertices: Vec<f32> = (self.buff)
            .enumerate()
            .flat_map(|(i, v)| {
                vec![
                    -1.0 + 2.0 * i as f32 / l as f32,
                    (2.0 * (v - min) / (max - min)) - 1.0,
                ]
            })
            .collect();

        let vertex_buffer = gl.create_buffer().unwrap();
        let verts = js_sys::Float32Array::from(vertices.as_slice());

        gl.bind_buffer(GL::ARRAY_BUFFER, Some(&vertex_buffer));
        gl.buffer_data_with_array_buffer_view(GL::ARRAY_BUFFER, &verts, GL::STATIC_DRAW);

        let vert_code = r#"
            attribute vec2 coordinates;
            void main(void) {
                gl_Position = vec4(coordinates, 0.0, 1.0);
            }
        "#;
        let vert_shader = gl.create_shader(GL::VERTEX_SHADER).unwrap();
        gl.shader_source(&vert_shader, vert_code);
        gl.compile_shader(&vert_shader);

        let frag_code = r#"
            void main(void) {
                gl_FragColor = vec4(1.0, 0.0, 0.0, 0.8);
            }
        "#;
        let frag_shader = gl.create_shader(GL::FRAGMENT_SHADER).unwrap();
        gl.shader_source(&frag_shader, frag_code);
        gl.compile_shader(&frag_shader);

        let shader_program = gl.create_program().unwrap();
        gl.attach_shader(&shader_program, &vert_shader);
        gl.attach_shader(&shader_program, &frag_shader);
        gl.link_program(&shader_program);

        gl.use_program(Some(&shader_program));

        // Attach the position vector as an attribute for the GL context.
        let position = gl.get_attrib_location(&shader_program, "coordinates") as u32;
        gl.vertex_attrib_pointer_with_i32(position, 2, GL::FLOAT, false, 0, 0);
        gl.enable_vertex_attrib_array(position);

        // Attach the time as a uniform for the GL context.
        let time = gl.get_uniform_location(&shader_program, "u_time");
        gl.uniform1f(time.as_ref(), timestamp as f32);

        gl.draw_arrays(GL::LINE_STRIP, 0, l as i32);

        let handle = {
            let link = ctx.link().clone();
            request_animation_frame(move |time| link.send_message(Msg::Render(time)))
        };
        self._render_loop = Some(handle);
    }
}
