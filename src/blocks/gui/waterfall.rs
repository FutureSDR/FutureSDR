use std::collections::VecDeque;
use std::time::Duration;

use rustfft::num_complex::Complex32;
use tokio::sync::mpsc::channel;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;

use crate::blocks::gui::SpectrumPlot;
use crate::gui::GuiWidget;
use crate::runtime::Block;

#[cfg(feature = "egui")]
const COLORGRAD_LOOKUP_SIZE: usize = 128;

/// GUI handle for a waterfall plot. The corresponding flowgraph block is a
/// [SpectrumPlot].
pub struct WaterfallHandle {
    receiver: Receiver<Vec<f64>>,
    center_freq_receiver: Receiver<f64>,
    /// Sender for sending center frequency updates to the block
    pub drag_freq_sender: Sender<f64>,
    /// Raw lines of values received from the flowgraph. Drained and
    /// processed by GUI implementations
    pub lines: VecDeque<Vec<f64>>,
    /// The plot's title, to be painted above the plot
    pub title: Option<String>,
    /// The current center frequency
    pub center_frequency: f64,
    /// The stream's sample rate
    pub sample_rate: f64,
    /// The used FFT size
    pub fft_size: usize,
    /// The current minimum value. IIR filtered for smoothness
    pub min: f64,
    /// The current maximum value. IIR filtered for smoothness
    pub max: f64,
    /// The amount of history to keep
    pub history: Duration,
    #[cfg(feature = "egui")]
    textures: VecDeque<(std::time::Instant, egui::TextureHandle)>,
    #[cfg(feature = "egui")]
    colorgrad_lookup: [egui::Color32; COLORGRAD_LOOKUP_SIZE],
    #[cfg(feature = "textplots")]
    canvas_lines: VecDeque<String>,
}

impl WaterfallHandle {
    /// Process changes received from the flowgraph block. Called by GUI
    /// implementations before drawing the UI.
    pub fn process_updates(&mut self) {
        while let Ok(center_freq) = self.center_freq_receiver.try_recv() {
            self.center_frequency = center_freq;
        }

        while let Ok(line) = self.receiver.try_recv() {
            self.fft_size = line.len();
            self.lines.push_back(line);
        }
    }

    /// Update the limits with the given data. Due to differences between
    /// GUI frontends, the chunk sizes and value downsampling is up to the
    /// widget implementation.
    pub fn update_limits(&mut self, data: &Vec<Vec<f64>>) {
        let mut tex_min = f64::INFINITY;
        let mut tex_max = f64::NEG_INFINITY;

        for x in 0..data[0].len() {
            for y in 0..data.len() {
                let val = data[y][x];
                if !val.is_nan() {
                    tex_min = f64::min(tex_min, data[y][x]);
                    tex_max = f64::max(tex_max, data[y][x]);
                }
            }
        }

        if tex_min.is_normal() && tex_max.is_normal() {
            const ALPHA: f64 = 0.05;
            self.min = self.min * (1.0 - ALPHA) + tex_min * ALPHA;
            self.max = self.max * (1.0 - ALPHA) + tex_max * ALPHA;
        }
    }
}

#[cfg(feature = "egui")]
impl egui::Widget for &mut WaterfallHandle {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        self.process_updates();

        const WATERFALL_TEX_HEIGHT: usize = 256;

        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(16));

        let width = ui.available_width();

        if let Some(title) = &self.title {
            ui.heading(title);
        }

        let t_per_texture = (WATERFALL_TEX_HEIGHT * self.fft_size) as f64 / self.sample_rate;
        while self.textures.len() > ((self.history.as_secs_f64() / t_per_texture) as usize) + 1 {
            let _ = self.textures.pop_back();
        }

        if self.lines.len() > WATERFALL_TEX_HEIGHT {
            let texture_data = self.lines.drain(..WATERFALL_TEX_HEIGHT).collect::<Vec<_>>();
            let now = std::time::Instant::now();
            let mut image = egui::ColorImage::new(
                [texture_data[0].len(), texture_data.len()],
                egui::Color32::TRANSPARENT,
            );

            self.update_limits(&texture_data);

            for x in 0..texture_data[0].len() {
                for y in 0..texture_data.len() {
                    let val = texture_data[texture_data.len() - y - 1][x];
                    let f = f64::max(0.0, (val - self.min) / (self.max - self.min));
                    let i = (f * (COLORGRAD_LOOKUP_SIZE as f64)) as usize;
                    let i = usize::min(i, COLORGRAD_LOOKUP_SIZE - 1);
                    image[(x, y)] = self.colorgrad_lookup[i];
                }
            }

            let tex_name = format!("waterfall_{:?}", now);
            let tex_handle = ui.ctx().load_texture(tex_name, image, Default::default());
            self.textures.push_front((now, tex_handle));
        }

        let response = egui_plot::Plot::new(ui.next_auto_id())
            .legend(egui_plot::Legend::default())
            .set_margin_fraction(egui::Vec2::new(0.0, 0.0))
            .show_grid(false)
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .include_x(self.center_frequency - self.sample_rate / 2.0)
            .include_x(self.center_frequency + self.sample_rate / 2.0)
            .include_y(0.0)
            .include_y(-self.history.as_secs_f64())
            .show_axes(egui::Vec2b::new(true, false))
            .height(ui.available_height())
            .x_axis_formatter(|x, _len, _range| {
                if x.value.abs() >= 1e9 {
                    format!("{}GHz", (x.value / 1e6).round() / 1000.0)
                } else if x.value.abs() >= 1e6 {
                    format!("{}MHz", (x.value / 1e3).round() / 1000.0)
                } else if x.value.abs() >= 1e3 {
                    format!("{}kHz", x.value.round() / 1000.0)
                } else {
                    format!("{}Hz", x.value)
                }
            })
            .show(ui, |plot_ui| {
                for (i, (_t, texture)) in self.textures.iter().enumerate() {
                    let plot_image = egui_plot::PlotImage::new(
                        texture,
                        egui_plot::PlotPoint::new(
                            self.center_frequency,
                            -((i as f64) + 0.5) * t_per_texture,
                        ),
                        egui::Vec2::new(self.sample_rate as f32, t_per_texture as f32),
                    );

                    plot_ui.image(plot_image);
                }
            })
            .response;

        if response.dragged_by(egui::PointerButton::Primary) {
            let drag_delta = (response.drag_motion().x / width) as f64;
            self.center_frequency -= drag_delta * self.sample_rate;
            let _ = self.drag_freq_sender.try_send(self.center_frequency);
        }

        response
    }
}

impl GuiWidget for WaterfallHandle {
    #[cfg(feature = "egui")]
    fn egui_ui(&mut self, ui: &mut egui::Ui) -> egui::Response {
        ui.add(self)
    }

    #[cfg(feature = "textplots")]
    fn textplots_ui(&mut self, size: (u16, u16)) {
        self.process_updates();

        const COLOR: drawille::PixelColor = drawille::PixelColor::TrueColor {
            r: 0xfa,
            g: 0xbd,
            b: 0x2f,
        };

        let fft_rate = (self.sample_rate as f32) / (self.fft_size as f32);
        let per_canvas_line = ((fft_rate * self.history.as_secs_f32()) / (size.1 as f32)) as usize;
        if self.lines.len() > per_canvas_line && per_canvas_line > 0 {
            let texture_data = self.lines.drain(..per_canvas_line).collect::<Vec<_>>();

            let canvas_width = (size.0 as usize - 10) * 2;
            let mut downsampled: Vec<Vec<f64>> = (0..4)
                .map(|_i| vec![f64::NEG_INFINITY; canvas_width])
                .collect();

            for x in 0..texture_data[0].len() {
                for y in 0..texture_data.len() {
                    let val = texture_data[y][x];
                    if !val.is_nan() {
                        let x_canvas = (((x as f32) / (texture_data[y].len() as f32))
                            * (canvas_width as f32))
                            as usize;
                        let y_canvas = (4.0 * (y as f32) / (per_canvas_line as f32)) as usize;
                        downsampled[y_canvas][x_canvas] =
                            f64::max(downsampled[y_canvas][x_canvas], val);
                    }
                }
            }

            self.update_limits(&downsampled);

            let threshold = (self.min + self.max) / 2.0;
            let mut canvas = drawille::Canvas::new(canvas_width as u32, 4);
            for x in 0..downsampled[0].len() {
                for y in 0..downsampled.len() {
                    if downsampled[y][x] > threshold {
                        canvas.set_colored(x as u32, y as u32, COLOR);
                    }
                }
            }

            self.canvas_lines.push_front(canvas.rows()[0].clone());
        }

        while self.canvas_lines.len() >= (size.1 as usize) {
            let _ = self.canvas_lines.pop_back();
        }

        for line in &self.canvas_lines {
            println!("{}", line);
        }
    }
}

/// Builder for a waterfall plot. The flowgraph block is a [SpectrumPlot],
/// only the GUI handle differs.
#[derive(Default)]
pub struct WaterfallBuilder {
    sample_rate: f64,
    fft_size: usize,
    center_frequency: f64,
    title: Option<String>,
    history: Duration,
}

impl WaterfallBuilder {
    /// Start building a new waterfall plot
    pub fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            fft_size: 1024,
            center_frequency: 0.0,
            history: Duration::from_secs_f32(1.0),
            ..Default::default()
        }
    }

    /// Set the FFT size
    pub fn fft_size(mut self, fft_size: usize) -> Self {
        self.fft_size = fft_size;
        self
    }

    /// Set the initial center frequency
    pub fn center_frequency(mut self, frequency: f64) -> Self {
        self.center_frequency = frequency;
        self
    }

    /// Set the plot's title
    pub fn title(mut self, title: impl ToString) -> Self {
        self.title = Some(title.to_string());
        self
    }

    /// Set the amount of history to keep, i.e. the height of the Y axis
    pub fn history(mut self, history: Duration) -> Self {
        self.history = history;
        self
    }

    /// Build the block and return both the block and a handle
    /// for the corresponding GUI widget.
    ///
    /// Use if you want to handle drawing the UI yourself.
    pub fn build_detached(self) -> (Block, WaterfallHandle) {
        let (sender, receiver) = channel(256);
        let (center_freq_sender, center_freq_receiver) = channel(10);
        let (drag_freq_sender, drag_freq_receiver) = channel(10);

        let block = SpectrumPlot::<Complex32>::new(
            self.fft_size,
            vec![sender],
            center_freq_sender,
            drag_freq_receiver,
            vec!["in".to_string()],
        );

        let handle = WaterfallHandle {
            receiver,
            center_freq_receiver,
            drag_freq_sender,
            title: self.title,
            center_frequency: self.center_frequency,
            sample_rate: self.sample_rate,
            fft_size: self.fft_size,
            history: self.history,
            min: -50.0,
            max: 50.0,
            lines: VecDeque::new(),
            #[cfg(feature = "egui")]
            textures: VecDeque::new(),
            #[cfg(feature = "egui")]
            colorgrad_lookup: (0..COLORGRAD_LOOKUP_SIZE)
                .map(move |i| {
                    let f = (i as f64) / (COLORGRAD_LOOKUP_SIZE as f64);
                    let rgba = colorgrad::inferno().at(f).to_rgba8();
                    egui::Color32::from_rgb(rgba[0], rgba[1], rgba[2])
                })
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
            #[cfg(feature = "textplots")]
            canvas_lines: VecDeque::new(),
        };

        (block, handle)
    }

    /// Build the block, leaving the GUI widget attached. In order to
    /// draw the UI, pass the flowgraph to [crate::gui::Gui::run].
    pub fn build(self) -> Block {
        let (mut block, handle) = self.build_detached();
        block.attach_gui_handle(Box::new(handle));
        block
    }
}
