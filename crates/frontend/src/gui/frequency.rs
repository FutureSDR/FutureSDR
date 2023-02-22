//! Frequency plot (Waterfall)
use futures::StreamExt;
use gloo_render::request_animation_frame;
use gloo_render::AnimationFrame;
use reqwasm::websocket::futures::WebSocket;
use reqwasm::websocket::Message;
use std::convert::TryInto;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlCanvasElement;
use web_sys::WebGlBuffer;
use web_sys::WebGlProgram;
use web_sys::WebGlRenderingContext as GL;
use web_sys::WebGlTexture;
use yew::prelude::*;

#[wasm_bindgen]
extern "C" {
    fn get_samples() -> Vec<f32>;
}

/// Mount a frequency plot to the website
///
/// ## Parameter
/// - `id`: HTML ID of component
/// - `url`: URL of websocket that streams data
/// - `min`: min value for scaling the color palette
/// - `max`: max value for scaling the color palette
#[wasm_bindgen]
pub fn add_freq(id: String, url: String, min: f32, max: f32) {
    let document = gloo_utils::document();
    let div = document.query_selector(&id).unwrap().unwrap();
    yew::start_app_with_props_in_element::<Frequency>(div, Props { url, min, max });
}

#[doc(hidden)]
pub enum Msg {
    Data(Vec<u8>),
    Status(String),
    Render(f64),
}

#[derive(Clone, Properties, Default, PartialEq)]
#[doc(hidden)]
pub struct Props {
    pub url: String,
    pub min: f32,
    pub max: f32,
}

/// Frequency plot (Waterfall)
pub struct Frequency {
    canvas_ref: NodeRef,
    gl: Option<GL>,
    _render_loop: Option<AnimationFrame>,
    last_data: [f32; 2048],
    vertex_buffer: Option<WebGlBuffer>,
    prog: Option<WebGlProgram>,
    num_indices: i32,
    texture_offset: i32,
    texture: Option<WebGlTexture>,
    uses_websocket: bool,
}

const HEIGHT: usize = 256;
const CANVAS_HEIGHT: usize = 256;
const CANVAS_WIDTH: usize = 256;

impl Component for Frequency {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let uses_websocket = if !ctx.props().url.is_empty() {
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

            true
        } else {
            false
        };

        Self {
            canvas_ref: NodeRef::default(),
            texture: None,
            vertex_buffer: None,
            texture_offset: 0,
            num_indices: 0,
            gl: None,
            prog: None,
            _render_loop: None,
            last_data: [0f32; 2048],
            uses_websocket,
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

        let display_width = canvas.client_width() as u32;
        let display_height = canvas.client_height() as u32;

        let need_resize = canvas.width() != display_width || canvas.height() != display_height;

        if need_resize {
            canvas.set_width(display_width);
            canvas.set_height(display_height);
        }

        gl.viewport(0, 0, display_width as i32, display_height as i32);

        let vert_code = r#"
attribute vec2 gTexCoord0;

uniform sampler2D frequency_data;
uniform float yoffset;

varying float power;

void main()
{
    vec4 sample = texture2D(frequency_data, vec2(gTexCoord0.x, gTexCoord0.y + yoffset));
    gl_Position = vec4((gTexCoord0 - 0.5) * 2.0, 0, 1);

    power = sample.a;
}
        "#;
        let vert_shader = gl.create_shader(GL::VERTEX_SHADER).unwrap();
        gl.shader_source(&vert_shader, vert_code);
        gl.compile_shader(&vert_shader);

        let frag_code = r#"
precision mediump float;

varying float power;

// All components are in the range [0…1], including hue.
vec3 hsv2rgb(vec3 c)
{
    vec4 K = vec4(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    vec3 p = abs(fract(c.xxx + K.xyz) * 6.0 - K.www);
    return c.z * mix(K.xxx, clamp(p - K.xxx, 0.0, 1.0), c.y);
}

void main()
{
    gl_FragColor = vec4(hsv2rgb(vec3(power, .7, 0.7)), power);
}
        "#;
        let frag_shader = gl.create_shader(GL::FRAGMENT_SHADER).unwrap();
        gl.shader_source(&frag_shader, frag_code);
        gl.compile_shader(&frag_shader);

        self.prog = Some(gl.create_program().unwrap());
        gl.attach_shader(self.prog.as_ref().unwrap(), &vert_shader);
        gl.attach_shader(self.prog.as_ref().unwrap(), &frag_shader);
        gl.link_program(self.prog.as_ref().unwrap());

        gl.use_program(self.prog.as_ref());

        // ===== prepare texture
        self.texture = Some(gl.create_texture().unwrap());
        gl.bind_texture(GL::TEXTURE_2D, Some(self.texture.as_ref().unwrap()));
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_WRAP_S, GL::REPEAT as i32);
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_WRAP_T, GL::REPEAT as i32);
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_MIN_FILTER, GL::NEAREST as i32);
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_MAG_FILTER, GL::NEAREST as i32);

        let d = vec![0u8; 2048 * HEIGHT];
        gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
            GL::TEXTURE_2D,
            0,
            GL::ALPHA as i32,
            2048,
            HEIGHT as i32,
            0,
            GL::ALPHA,
            GL::UNSIGNED_BYTE,
            Some(&d),
        )
        .unwrap();

        // ===== prepare vertex
        let mut vertexes = Vec::new();
        let s = 1.0 / (2.0 * CANVAS_HEIGHT as f32);
        for h in 0..CANVAS_HEIGHT {
            for w in 0..CANVAS_WIDTH {
                vertexes.push(w as f32 / (CANVAS_WIDTH) as f32 + s);
                vertexes.push(h as f32 / (CANVAS_HEIGHT) as f32 + s);
            }
        }

        self.vertex_buffer = Some(gl.create_buffer().unwrap());
        gl.bind_buffer(GL::ARRAY_BUFFER, self.vertex_buffer.as_ref());
        let array_buffer = js_sys::Float32Array::from(vertexes.as_slice()).buffer();
        gl.buffer_data_with_opt_array_buffer(
            GL::ARRAY_BUFFER,
            Some(&array_buffer),
            GL::STATIC_DRAW,
        );

        let mut indices: Vec<u16> = Vec::new();
        for h in 0..CANVAS_HEIGHT - 1 {
            for w in 0..CANVAS_WIDTH - 1 {
                let o = h * CANVAS_WIDTH;
                let o1 = (h + 1) * CANVAS_WIDTH;
                indices.push((o + w) as u16);
                indices.push((o + w + 1) as u16);
                indices.push((o1 + w + 1) as u16);

                indices.push((o + w) as u16);
                indices.push((o1 + w) as u16);
                indices.push((o1 + w + 1) as u16);
            }
        }
        self.num_indices = indices.len() as i32;

        let indices_buffer = gl.create_buffer().unwrap();
        gl.bind_buffer(GL::ELEMENT_ARRAY_BUFFER, Some(&indices_buffer));
        let array_buffer = js_sys::Uint16Array::from(indices.as_slice()).buffer();
        gl.buffer_data_with_opt_array_buffer(
            GL::ELEMENT_ARRAY_BUFFER,
            Some(&array_buffer),
            GL::STATIC_DRAW,
        );

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
                if !self.uses_websocket {
                    self.last_data = get_samples().try_into().expect("data has wrong size");
                }
                self.render_gl(timestamp, ctx);
            }
            Msg::Data(b) => {
                let v;
                unsafe {
                    let s = b.len() / 4;
                    let p = b.as_ptr();
                    v = std::slice::from_raw_parts(p as *const f32, s);
                }
                self.last_data = v.try_into().expect("data has wrong size");
            }
            Msg::Status(s) => {
                gloo_console::log!(format!("socket status {:?}", &s));
            }
        }
        false
    }

    fn view(&self, _ctx: &Context<Self>) -> Html {
        html! {
            <canvas ref={self.canvas_ref.clone()} />
        }
    }
}

impl Frequency {
    fn render_gl(&mut self, _timestamp: f64, ctx: &Context<Self>) {
        let gl = self.gl.as_ref().unwrap();

        gl.bind_texture(GL::TEXTURE_2D, self.texture.as_ref());
        gl.pixel_storei(GL::UNPACK_ALIGNMENT, 1);

        let props = ctx.props();

        let data: Vec<u8> = self
            .last_data
            .iter()
            .map(|v| {
                ((v.clamp(props.min, props.max) - props.min) / (props.max - props.min) * 255.0)
                    as u8
            })
            .collect();

        gl.tex_sub_image_2d_with_i32_and_i32_and_u32_and_type_and_opt_u8_array(
            GL::TEXTURE_2D,
            0,
            0,
            self.texture_offset,
            2048,
            1,
            GL::ALPHA,
            GL::UNSIGNED_BYTE,
            Some(&data),
        )
        .unwrap();

        gl.bind_buffer(GL::ARRAY_BUFFER, self.vertex_buffer.as_ref());

        let loc = gl.get_attrib_location(self.prog.as_ref().unwrap(), "gTexCoord0") as u32;
        gl.enable_vertex_attrib_array(loc);
        gl.vertex_attrib_pointer_with_i32(loc, 2, GL::FLOAT, false, 0, 0);

        let loc = gl.get_uniform_location(self.prog.as_ref().unwrap(), "yoffset");
        gl.uniform1f(loc.as_ref(), self.texture_offset as f32 / HEIGHT as f32);
        let loc = gl.get_uniform_location(self.prog.as_ref().unwrap(), "frequency_data");
        gl.uniform1i(loc.as_ref(), 0);

        gl.draw_elements_with_i32(GL::TRIANGLES, self.num_indices, GL::UNSIGNED_SHORT, 0);

        self.texture_offset = (self.texture_offset + 1) % HEIGHT as i32;

        let handle = {
            let link = ctx.link().clone();
            request_animation_frame(move |time| link.send_message(Msg::Render(time)))
        };
        self._render_loop = Some(handle);
    }
}
