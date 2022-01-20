use rbl_circular_buffer::CircularBuffer;
use wasm_bindgen::JsCast;
use web_sys::HtmlCanvasElement;
use web_sys::WebGlRenderingContext as GL;
use yew::format::Binary;
use yew::prelude::*;
use yew::services::websocket::{WebSocketStatus, WebSocketTask};
use yew::services::ConsoleService;
use yew::services::WebSocketService;
use yew::services::{RenderService, Task};
use yew::prelude::*;

pub enum Msg {
    Data(Binary),
    Status(WebSocketStatus),
    Render(f64),
}

#[derive(Clone, Properties, Default, PartialEq)]
pub struct Props {
    pub url: String,
}

pub struct Time {
    link: ComponentLink<Self>,
    props: Props,
    canvas_ref: NodeRef,
    _canvas: Option<HtmlCanvasElement>,
    gl: Option<GL>,
    _render_loop: Option<Box<dyn Task>>,
    buff: CircularBuffer<f32>,
    _websocket_task: WebSocketTask,
}

impl Component for Time {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let cb = link.callback(Msg::Data);
        let notification = link.callback(Msg::Status);
        let _websocket_task =
            WebSocketService::connect_binary(&props.url, cb, notification).unwrap();

        Self {
            link,
            props,
            canvas_ref: NodeRef::default(),
            _canvas: None,
            gl: None,
            _render_loop: None,
            buff: CircularBuffer::new(2048),
            _websocket_task,
        }
    }

    fn rendered(&mut self, _ctx: &Context<Self>, first_render: bool) {
        // Once rendered, store references for the canvas and GL context. These can be used for
        // resizing the rendering area when the window or canvas element are resized, as well as
        // for making GL calls.

        let canvas = self.canvas_ref.cast::<HtmlCanvasElement>().unwrap();

        let gl: GL = canvas
            .get_context("webgl")
            .unwrap()
            .unwrap()
            .dyn_into()
            .unwrap();

        self._canvas = Some(canvas);
        self.gl = Some(gl);

        // In a more complex use-case, there will be additional WebGL initialization that should be
        // done here, such as enabling or disabling depth testing, depth functions, face
        // culling etc.

        if first_render {
            // The callback to request animation frame is passed a time value which can be used for
            // rendering motion independent of the framerate which may vary.
            let render_frame = self.link.callback(Msg::Render);
            let handle = RenderService::request_animation_frame(render_frame);

            // A reference to the handle must be stored, otherwise it is dropped and the render won't
            // occur.
            self._render_loop = Some(Box::new(handle));
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Render(timestamp) => {
                // Render functions are likely to get quite large, so it is good practice to split
                // it into it's own function rather than keeping it inline in the update match
                // case. This also allows for updating other UI elements that may be rendered in
                // the DOM like a framerate counter, or other overlaid textual elements.
                self.render_gl(timestamp);
            }
            Msg::Data(b) => {
                if let Ok(b) = b {
                    let v;
                    unsafe {
                        let s = b.len() / 4;
                        let p = b.as_ptr();
                        v = std::slice::from_raw_parts(p as *const f32, s);
                    }
                    ConsoleService::log(&format!("received bytes: {:?}", b.len()));
                    for i in v {
                        self.buff.push(*i);
                    }
                    ConsoleService::log(&format!("buf: {:?}", &self.buff));
                }
            }
            Msg::Status(s) => {
                ConsoleService::log(&format!("socket status {:?}", &s));
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
    fn render_gl(&mut self, timestamp: f64) {
        let gl = self.gl.as_ref().expect("GL Context not initialized!");

        let l = self.buff.len();
        let vertices: Vec<f32> = (&self.buff)
            .enumerate()
            .flat_map(|(i, v)| vec![-1.0 + 2.0 * i as f32 / l as f32, -1.0 + v as f32 / 255.0])
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

        let render_frame = self.link.callback(Msg::Render);
        let handle = RenderService::request_animation_frame(render_frame);

        // A reference to the new handle must be retained for the next render to run.
        self._render_loop = Some(Box::new(handle));
    }
}
