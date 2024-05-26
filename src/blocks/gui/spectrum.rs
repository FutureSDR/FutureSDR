use std::fmt::Debug;
use std::sync::Arc;

use rustfft::num_complex::Complex32;
use rustfft::num_traits::FromPrimitive;
use rustfft::{self, FftPlanner};
use tokio::sync::mpsc::channel;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;

use crate::anyhow::Result;
use crate::gui::{Color, GuiWidget};
use crate::runtime::{
    Block, BlockMeta, BlockMetaBuilder, Kernel, MessageIo, MessageIoBuilder, Pmt, StreamIo,
    StreamIoBuilder, WorkIo,
};

/// Handle for a single line in a spectrum plot
pub struct SpectrumPlotLineHandle {
    receiver: Receiver<Vec<f64>>,
    /// Currently plotted values
    pub values: Vec<f64>,
    /// Label for the line
    pub label: String,
    /// Color for the label
    pub color: Color,
}

/// GUI handle for a spectrum plot.
pub struct SpectrumPlotHandle {
    center_freq_receiver: Receiver<f64>,
    /// A handle for each line being plotted
    pub lines: Vec<SpectrumPlotLineHandle>,
    /// Sender for sending center frequency updates to the block
    pub drag_freq_sender: Sender<f64>,
    /// The plot's title, to be painted above the plot
    pub title: Option<String>,
    /// The current center frequency
    pub center_frequency: f64,
    /// The stream's sample rate
    pub sample_rate: f64,
    /// The current minimum value. IIR filtered for smoothness
    pub min: f64,
    /// The current maximum value. IIR filtered for smoothness
    pub max: f64,
}

/// Block that plots Fourier transforms of incoming samples.
///
/// Multiple input streams are supported. Center frequency is only used for
/// displaying the correct X-axis values in the UI, and can be updated from
/// somewhere else in the flowgraph and via user interaction like dragging.
pub struct SpectrumPlot<T> {
    senders: Vec<Sender<Vec<f64>>>,
    center_freq_sender: Sender<f64>,
    drag_freq_receiver: Receiver<f64>,
    plan: Arc<dyn rustfft::Fft<f32>>,
    scratch: Box<[T]>,
    fft_size: usize,
}

impl SpectrumPlotHandle {
    /// Process changes received from the flowgraph block. Called by GUI
    /// implementations before drawing the UI.
    pub fn process_updates(&mut self) {
        while let Ok(center_freq) = self.center_freq_receiver.try_recv() {
            self.center_frequency = center_freq;
        }

        for line in self.lines.iter_mut() {
            let mut new = Vec::new();
            while let Ok(buffer) = line.receiver.try_recv() {
                new.push(buffer);
            }

            if new.len() == 0 {
                continue;
            }

            // If we received multiple FFT buffers since the last frame,
            // take the maximum of all of them for each frequency. This
            // way, we can't miss anything.
            line.values = new
                .iter()
                .fold(vec![f64::NEG_INFINITY; new[0].len()], |a, b| {
                    a.iter()
                        .zip(b.iter())
                        .map(|(a, b)| f64::max(*a, *b))
                        .collect()
                });

            let new_min = line
                .values
                .iter()
                .fold(f64::INFINITY, |a, b| f64::min(a, *b));
            let new_max = line
                .values
                .iter()
                .fold(f64::NEG_INFINITY, |a, b| f64::max(a, *b));

            const ALPHA: f64 = 0.002;
            self.min = f64::min(new_min, (1.0 - ALPHA) * self.min + ALPHA * new_min);
            self.max = f64::max(new_max, (1.0 - ALPHA) * self.max + ALPHA * new_max);
        }
    }
}

#[cfg(feature = "egui")]
impl egui::Widget for &mut SpectrumPlotHandle {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        self.process_updates();

        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(16));

        let width = ui.available_width();

        if let Some(title) = &self.title {
            ui.heading(title);
        }

        let mut plot = egui_plot::Plot::new(ui.next_auto_id())
            .set_margin_fraction(egui::Vec2::new(0.0, 0.1))
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .include_x(self.center_frequency - self.sample_rate / 2.0)
            .include_x(self.center_frequency + self.sample_rate / 2.0)
            .include_y(self.min)
            .include_y(self.max)
            .show_axes(egui::Vec2b::new(true, false))
            .height(ui.available_height())
            .allow_double_click_reset(true)
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
            .label_formatter(|name, value| {
                let x = value.x;
                let y = (value.y * 100.0).round() / 100.0;
                if x.abs() >= 1e9 {
                    format!("{}: {}dB @ {}GHz", name, y, (x / 1e7).round() / 100.0)
                } else if x.abs() >= 1e6 {
                    format!("{}: {}dB @ {}MHz", name, y, (x / 1e4).round() / 100.0)
                } else if x.abs() >= 1e3 {
                    format!("{}: {}dB @ {}kHz", name, y, (x / 1e1).round() / 100.0)
                } else {
                    format!("{}: {}dB @ {}Hz", name, y, x)
                }
            })
            .reset();

        if self.lines.len() > 1 || self.lines[0].label != "in" {
            plot = plot.legend(egui_plot::Legend::default().position(egui_plot::Corner::LeftTop));
        }

        let response = plot
            .show(ui, |plot_ui| {
                for line in self.lines.iter_mut() {
                    if line.values.len() > 0 {
                        let points: Vec<_> = line
                            .values
                            .iter()
                            .enumerate()
                            .map(|(i, y)| {
                                [
                                    self.center_frequency
                                        + ((i as f64 / line.values.len() as f64) - 0.5)
                                            * self.sample_rate,
                                    *y,
                                ]
                            })
                            .collect();
                        let line_color: egui::Color32 = line.color.into();
                        let egui_line = egui_plot::Line::new(points)
                            .name(&line.label)
                            .color(line_color);
                        plot_ui.line(egui_line);
                    }
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

impl GuiWidget for SpectrumPlotHandle {
    #[cfg(feature = "egui")]
    fn egui_ui(&mut self, ui: &mut egui::Ui) -> egui::Response {
        ui.add(self)
    }

    #[cfg(feature = "textplots")]
    fn textplots_ui(&mut self, size: (u16, u16)) {
        use textplots::ColorPlot;

        self.process_updates();

        let min_freq = (self.center_frequency - self.sample_rate / 2.0) as f32;
        let max_freq = (self.center_frequency + self.sample_rate / 2.0) as f32;

        let line_points: Vec<Vec<_>> = self
            .lines
            .iter()
            .map(|line| {
                line.values
                    .iter()
                    .enumerate()
                    .map(|(i, y)| {
                        (
                            (self.center_frequency
                                + ((i as f64 / line.values.len() as f64) - 0.5) * self.sample_rate)
                                as f32,
                            *y as f32,
                        )
                    })
                    .collect()
            })
            .collect();

        let line_shapes: Vec<_> = line_points
            .iter()
            .map(|points| textplots::Shape::Lines(&points))
            .collect();

        let mut chart = textplots::Chart::new_with_y_range(
            (size.0 as u32 - 10) * 2,
            (size.1 as u32 - 2) * 4,
            min_freq,
            max_freq,
            (self.min - (self.max - self.min) * 0.1) as f32,
            (self.max + (self.max - self.min) * 0.1) as f32,
        );
        let mut chart_ref = &mut chart;
        for (line, shape) in self.lines.iter_mut().zip(line_shapes.iter()) {
            chart_ref = chart_ref.linecolorplot(&shape, line.color.into());
        }

        chart_ref.display();
    }
}

impl<T: FromPrimitive + Copy + Clone + Debug + Default + Send + Sync + 'static> SpectrumPlot<T>
where
    SpectrumPlot<T>: Kernel,
{
    /// Construct a new spectrum block
    pub fn new(
        fft_size: usize,
        senders: Vec<Sender<Vec<f64>>>,
        center_freq_sender: Sender<f64>,
        drag_freq_receiver: Receiver<f64>,
        inputs: Vec<String>,
    ) -> Block {
        let mut stream_io = StreamIoBuilder::new();
        for l in &inputs {
            stream_io = stream_io.add_input::<T>(l);
        }

        let mut planner = FftPlanner::<f32>::new();
        let plan = planner.plan_fft_forward(fft_size);

        Block::new(
            BlockMetaBuilder::new("SpectrumPlot").build(),
            stream_io.build(),
            MessageIoBuilder::new()
                .add_input("center_freq", Self::center_freq_handler)
                .add_output("drag_freq")
                .build(),
            SpectrumPlot {
                senders,
                center_freq_sender,
                drag_freq_receiver,
                fft_size,
                plan,
                scratch: vec![T::default(); fft_size * 10].into_boxed_slice(),
            },
        )
    }

    #[message_handler]
    async fn center_freq_handler(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        let freq = match &p {
            Pmt::F32(v) => Some(*v as f64),
            Pmt::F64(v) => Some(*v),
            Pmt::U32(v) => Some(*v as f64),
            Pmt::U64(v) => Some(*v as f64),
            _ => None,
        };

        if let Some(freq) = freq {
            let _ = self.center_freq_sender.try_send(freq);
        }

        Ok(Pmt::Ok)
    }
}

// TODO: implementation for f32/f64

#[doc(hidden)]
#[async_trait]
impl Kernel for SpectrumPlot<Complex32> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        if let Ok(frequency) = self.drag_freq_receiver.try_recv() {
            mio.post(0, Pmt::F64(frequency)).await;
        }

        for (i, sender) in self.senders.iter_mut().enumerate() {
            let input = unsafe { sio.input(i).slice_mut::<Complex32>() };
            let m = (input.len() / self.fft_size) * self.fft_size;

            if m == 0 {
                continue;
            }

            let mut output = vec![Complex32::default(); m];
            self.plan.process_outofplace_with_scratch(
                &mut input[0..m],
                &mut output[0..m],
                &mut self.scratch,
            );

            sio.input(i).consume(m);

            for chunk in output.chunks(self.fft_size) {
                let db: Vec<_> = chunk
                    .iter()
                    .map(|c| 10.0 * c.norm_sqr().log10() as f64)
                    .collect();
                let reordered = [&db[self.fft_size / 2..], &db[..self.fft_size / 2]].concat();
                let _ = sender.try_send(reordered);
            }
        }

        io.finished = self
            .senders
            .iter()
            .enumerate()
            .all(|(i, _)| sio.input(i).finished());

        Ok(())
    }
}

/// Builder for a [SpectrumPlot]
///
/// If no lines are added manually, a single line is added with the default
/// input port name 'in'.
#[derive(Default)]
pub struct SpectrumPlotBuilder {
    lines: Vec<(String, Color)>,
    sample_rate: f64,
    fft_size: usize,
    center_frequency: f64,
    title: Option<String>,
}

impl SpectrumPlotBuilder {
    /// Start building a new spectrum plot
    pub fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            fft_size: 1024,
            center_frequency: 0.0,
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

    /// Add a new line to the plot with the given color
    pub fn line(mut self, label: impl ToString, color: Color) -> Self {
        self.lines.push((label.to_string(), color));
        self
    }

    /// Build the block and return both the block and a handle
    /// for the corresponding GUI widget.
    ///
    /// Use if you want to handle drawing the UI yourself.
    pub fn build_detached(mut self) -> (Block, SpectrumPlotHandle) {
        if self.lines.len() == 0 {
            self.lines.push((
                "in".to_string(),
                Color {
                    r: 0xfa,
                    g: 0xbd,
                    b: 0x2f,
                },
            ));
        }

        let mut senders = Vec::new();
        let mut lines = Vec::new();
        let mut labels = Vec::new();
        for (label, color) in self.lines.into_iter() {
            let (sender, receiver) = channel(256);
            let line = SpectrumPlotLineHandle {
                receiver,
                values: vec![],
                label: label.clone(),
                color,
            };

            senders.push(sender);
            lines.push(line);
            labels.push(label);
        }

        let (center_freq_sender, center_freq_receiver) = channel(10);
        let (drag_freq_sender, drag_freq_receiver) = channel(10);

        let block = SpectrumPlot::<Complex32>::new(
            self.fft_size,
            senders,
            center_freq_sender,
            drag_freq_receiver,
            labels,
        );

        let handle = SpectrumPlotHandle {
            lines,
            center_freq_receiver,
            drag_freq_sender,
            title: self.title,
            center_frequency: self.center_frequency,
            sample_rate: self.sample_rate,
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
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
