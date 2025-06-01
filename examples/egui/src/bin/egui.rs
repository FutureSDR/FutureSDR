#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use eframe::egui::mutex::Mutex;
use eframe::egui::widgets::SliderClamping;
use eframe::egui_glow;
use eframe::glow;
use futuresdr::runtime::Pmt;
use futuresdr_remote::Handler;
use futuresdr_remote::Remote;
use std::net::TcpStream;
use std::sync::Arc;
use std::thread;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::mpsc::unbounded_channel;
use tungstenite::Message;
use tungstenite::connect;
use tungstenite::protocol::WebSocket;
use tungstenite::stream::MaybeTlsStream;

use futuresdr_egui::FFT_SIZE;

fn main() -> Result<(), eframe::Error> {
    futuresdr::runtime::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([900.0, 600.0]),
        multisampling: 4,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "FutureSDR + egui",
        options,
        Box::new(|cc| Ok(Box::new(MyApp::new(cc)))),
    )
}

enum GuiAction {
    SetFreq(u64),
}

async fn process_gui_actions(mut rx: UnboundedReceiver<GuiAction>) -> anyhow::Result<()> {
    let remote = Remote::new("http://127.0.0.1:1337");
    let fgs = remote.flowgraphs().await?;
    println!("sdr {:?}", fgs[0].blocks());
    let sdr = fgs[0]
        .blocks()
        .into_iter()
        .find(|b| b.description().type_name == "SeifySource")
        .unwrap();

    while let Some(m) = rx.recv().await {
        match m {
            GuiAction::SetFreq(f) => {
                println!("setting frequency to {f}MHz");
                sdr.callback(Handler::Name("freq".to_string()), Pmt::U64(f * 1000000))
                    .await?
            }
        };
    }

    Ok(())
}

struct MyApp {
    freq: u64,
    min: f32,
    max: f32,
    actions: UnboundedSender<GuiAction>,
    spectrum: Arc<Mutex<Spectrum>>,
}

impl MyApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (tx, rx) = unbounded_channel();
        thread::spawn(move || {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(process_gui_actions(rx))
                .unwrap();
        });

        let (socket, response) = connect("ws://127.0.0.1:9001").expect("Can't connect");
        match socket.get_ref() {
            MaybeTlsStream::Plain(s) => s.set_nonblocking(true).unwrap(),
            MaybeTlsStream::Rustls(s) => s.sock.set_nonblocking(true).unwrap(),
            _ => {}
        }

        println!("Connected to the server");
        println!("Response HTTP code: {}", response.status());

        let gl = cc
            .gl
            .as_ref()
            .expect("You need to run eframe with the glow backend");

        Self {
            min: -50.0,
            max: 50.0,
            freq: 100,
            actions: tx,
            spectrum: Arc::new(Mutex::new(Spectrum::new(gl, socket))),
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("FutureSDR + egui");
            ui.columns(3, |columns| {
                if columns[0]
                    .add(
                        egui::Slider::new(&mut self.freq, 80..=200)
                            .clamping(SliderClamping::Never)
                            .suffix("MHz")
                            .text("frequency"),
                    )
                    .changed()
                {
                    let _ = self.actions.send(GuiAction::SetFreq(self.freq));
                }
                if columns[1]
                    .add(
                        egui::Slider::new(&mut self.min, -50.0..=0.0)
                            .clamping(SliderClamping::Never)
                            .suffix("dB")
                            .text("min"),
                    )
                    .changed()
                {
                    self.spectrum.lock().set_min(self.min);
                }
                if columns[2]
                    .add(
                        egui::Slider::new(&mut self.max, -20.0..=50.0)
                            .clamping(SliderClamping::Never)
                            .suffix("dB")
                            .text("max"),
                    )
                    .changed()
                {
                    self.spectrum.lock().set_max(self.max);
                }
            });
            egui::Frame::canvas(ui.style()).show(ui, |ui| {
                let (rect, _response) =
                    ui.allocate_exact_size(ui.available_size(), egui::Sense::drag());
                let spectrum = self.spectrum.clone();
                let callback = egui::PaintCallback {
                    rect,
                    callback: std::sync::Arc::new(egui_glow::CallbackFn::new(
                        move |_info, painter| {
                            spectrum.lock().paint(painter.gl());
                        },
                    )),
                };
                ui.painter().add(callback);
            });
        });
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }

    fn on_exit(&mut self, gl: Option<&glow::Context>) {
        if let Some(gl) = gl {
            self.spectrum.lock().destroy(gl);
        }
    }
}

struct Spectrum {
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
    program: glow::Program,
    array_buffer: glow::NativeBuffer,
    vertex_array: glow::VertexArray,
    coordinates: [f32; FFT_SIZE * 2],
    new_min: Option<f32>,
    new_max: Option<f32>,
}

impl Spectrum {
    fn new(gl: &glow::Context, socket: WebSocket<MaybeTlsStream<TcpStream>>) -> Self {
        use glow::HasContext as _;

        let shader_version = if cfg!(target_arch = "wasm32") {
            "#version 300 es"
        } else {
            "#version 330"
        };

        unsafe {
            let program = gl.create_program().expect("Cannot create program");

            let (vertex_shader_source, fragment_shader_source) = (
                r#"
                in vec2 coordinates;
                uniform float u_nsamples;
                uniform float u_min;
                uniform float u_max;
                out float power;

                void main(void) {
                    float x = -1.0 + 2.0 * coordinates.x / u_nsamples;
                    power = (10.0 * log(coordinates.y) / log(10.0) - u_min) / (u_max - u_min);
                    float y = 2.0 * power - 1.0;
                    gl_Position = vec4(x, y, 0.0, 1.0);
                }
                "#,
                r#"
                precision mediump float;
                in float power;
                out vec4 FragColor;

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
                    FragColor = vec4(color_map(clamp(power, 0.0, 1.0)), 0.9);
                }


                "#,
            );

            let shader_sources = [
                (glow::VERTEX_SHADER, vertex_shader_source),
                (glow::FRAGMENT_SHADER, fragment_shader_source),
            ];

            let shaders: Vec<_> = shader_sources
                .iter()
                .map(|(shader_type, shader_source)| {
                    let shader = gl
                        .create_shader(*shader_type)
                        .expect("Cannot create shader");
                    gl.shader_source(shader, &format!("{shader_version}\n{shader_source}"));
                    gl.compile_shader(shader);
                    assert!(
                        gl.get_shader_compile_status(shader),
                        "Failed to compile {shader_type}: {}",
                        gl.get_shader_info_log(shader)
                    );
                    gl.attach_shader(program, shader);
                    shader
                })
                .collect();

            gl.link_program(program);
            assert!(
                gl.get_program_link_status(program),
                "{}",
                gl.get_program_info_log(program)
            );

            for shader in shaders {
                gl.detach_shader(program, shader);
                gl.delete_shader(shader);
            }

            gl.use_program(Some(program));

            gl.uniform_1_f32(
                gl.get_uniform_location(program, "u_nsamples").as_ref(),
                FFT_SIZE as f32,
            );
            gl.uniform_1_f32(gl.get_uniform_location(program, "u_min").as_ref(), -50.0);
            gl.uniform_1_f32(gl.get_uniform_location(program, "u_max").as_ref(), 50.0);

            let vertex_array = gl
                .create_vertex_array()
                .expect("Cannot create vertex array");

            Self {
                program,
                array_buffer: gl.create_buffer().unwrap(),
                vertex_array,
                socket,
                coordinates: [0.0; FFT_SIZE * 2],
                new_min: None,
                new_max: None,
            }
        }
    }

    fn destroy(&self, gl: &glow::Context) {
        use glow::HasContext as _;
        unsafe {
            gl.delete_program(self.program);
            gl.delete_vertex_array(self.vertex_array);
        }
    }

    fn set_min(&mut self, min: f32) {
        self.new_min = Some(min);
    }

    fn set_max(&mut self, max: f32) {
        self.new_max = Some(max);
    }

    fn paint(&mut self, gl: &glow::Context) {
        use glow::HasContext as _;

        unsafe {
            gl.use_program(Some(self.program));

            if let Some(m) = self.new_min.take() {
                gl.uniform_1_f32(gl.get_uniform_location(self.program, "u_min").as_ref(), m);
            }

            if let Some(m) = self.new_max.take() {
                gl.uniform_1_f32(gl.get_uniform_location(self.program, "u_max").as_ref(), m);
            }

            if let Ok(Message::Binary(v)) = self.socket.read() {
                let mut data = v;
                while let Ok(Message::Binary(v)) = self.socket.read() {
                    data = v;
                }

                let samples = {
                    let s = data.len() / std::mem::size_of::<f32>();
                    let p = data.as_ptr();
                    std::slice::from_raw_parts(p as *const f32, s)
                };

                for (a, (i, f)) in self
                    .coordinates
                    .chunks_exact_mut(2)
                    .zip(samples.iter().enumerate())
                {
                    a[0] = i as f32;
                    a[1] = *f;
                }

                let bytes = {
                    let s = self.coordinates.len() * std::mem::size_of::<f32>();
                    let p = self.coordinates.as_ptr();
                    std::slice::from_raw_parts(p as *const u8, s)
                };

                gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.array_buffer));
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STATIC_DRAW);

                gl.bind_vertex_array(Some(self.vertex_array));
                let coords = gl.get_attrib_location(self.program, "coordinates").unwrap();
                gl.enable_vertex_attrib_array(coords);
                gl.vertex_attrib_pointer_f32(coords, 2, glow::FLOAT, false, 0, 0);
                gl.draw_arrays(glow::LINE_STRIP, 0, FFT_SIZE as i32);
            }
        }
    }
}
