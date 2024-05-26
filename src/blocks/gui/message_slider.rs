// We don't draw the sliders with the textplots backend, so disable the warnings
#![cfg_attr(feature = "textplots", allow(unused_imports))]
#![cfg_attr(feature = "textplots", allow(dead_code))]

use std::marker::PhantomData;
use std::ops::{Div, Mul};
use std::time::Duration;

use tokio::sync::mpsc::channel;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;

use crate::anyhow::Result;
use crate::async_io::Timer;
use crate::futures::select;
use crate::futures::FutureExt;
use crate::gui::GuiWidget;
use crate::runtime::{
    Block, BlockMeta, BlockMetaBuilder, Kernel, MessageIo, MessageIoBuilder, Pmt, StreamIo,
    StreamIoBuilder, WorkIo,
};

/// Handle for the slider block that is passed to the GUI implementation.
///
/// Primarily, the handle's sender is used to update the flowgraph with new
/// values from user interaction. If the value is changed by something else
/// inside the flowgraph, the receiver can be used to update the value in
/// the interface.
pub struct MessageSliderHandle<T> {
    /// The current value to be displayed in the UI
    pub value: T,
    /// The range of valid input values
    pub range: std::ops::RangeInclusive<T>,
    /// Minimum step size when dragging the slider
    pub step_size: Option<f64>,
    /// Multiplier that is applied to values input in the UI before they are
    /// emitted as messages
    pub multiplier: Option<T>,
    /// Suffix to be added to the value in the UI, like a unit
    pub suffix: Option<String>,
    /// Label for the slider
    pub label: Option<String>,
    /// Sender for newly changed values
    pub sender: Sender<Pmt>,
    /// Receiver for changes to the value made inside the flowgraph, via the
    /// message input
    pub receiver: Receiver<Pmt>,
}

#[cfg(feature = "egui")]
impl<T: Send + Mul<Output = T> + Div<Output = T> + egui::emath::Numeric> egui::Widget
    for &mut MessageSliderHandle<T>
where
    Pmt: From<T> + TryInto<T>,
{
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        while let Ok(pmt) = self.receiver.try_recv() {
            if let Ok(val) = pmt.try_into() {
                self.value = self.multiplier.map(|mul| val / mul).unwrap_or(val);
            }
        }

        let mut slider =
            egui::Slider::new(&mut self.value, self.range.clone()).clamp_to_range(false);

        if let Some(step_size) = &self.step_size {
            slider = slider.step_by(*step_size).drag_value_speed(*step_size);
        }

        if let Some(suffix) = &self.suffix {
            slider = slider.suffix(suffix);
        }

        if let Some(label) = &self.label {
            slider = slider.text(label);
        }

        let response = ui.add(slider);
        if response.changed() {
            let val = self
                .multiplier
                .map(|mul| self.value * mul)
                .unwrap_or(self.value);
            let _ = self.sender.try_send(val.into());
        }

        response
    }
}

#[cfg(feature = "egui")]
impl<T> GuiWidget for MessageSliderHandle<T>
where
    for<'a> &'a mut MessageSliderHandle<T>: egui::Widget,
    T: Send,
{
    fn widget_type(&self) -> crate::gui::GuiWidgetType {
        crate::gui::GuiWidgetType::Control
    }

    fn egui_ui(&mut self, ui: &mut egui::Ui) -> egui::Response {
        ui.add(self)
    }
}

#[cfg(feature = "textplots")]
impl<T: Send> GuiWidget for MessageSliderHandle<T> {
    fn widget_type(&self) -> crate::gui::GuiWidgetType {
        crate::gui::GuiWidgetType::Control
    }

    // We don't actually display these in textplots mode
}

/// Block that emits values configured via a GUI slider.
///
/// New values from user interaction are emitted as messages. If the value can
/// be changed from somewhere else in the flowgraph, the UI can be synchronized
/// using the input port.
pub struct MessageSlider<T> {
    sender: Sender<Pmt>,
    receiver: Receiver<Pmt>,
    _type: PhantomData<T>,
}

impl<T: Send + Sync + 'static> MessageSlider<T> {
    /// Construct a new block that exchanges Pmts with the GUI handle
    pub fn new(sender: Sender<Pmt>, receiver: Receiver<Pmt>) -> Block {
        Block::new(
            BlockMetaBuilder::new("MessageSlider").build(),
            StreamIoBuilder::new().build(),
            MessageIoBuilder::new()
                .add_input("in", Self::receiver_handler)
                .add_output("out")
                .build(),
            MessageSlider {
                sender,
                receiver,
                _type: PhantomData::<T>,
            },
        )
    }

    #[message_handler]
    async fn receiver_handler(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        let _ = self.sender.try_send(p);
        Ok(Pmt::Ok)
    }
}

#[doc(hidden)]
#[async_trait]
impl<T: Send + Sync + 'static> Kernel for MessageSlider<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _sio: &mut StreamIo,
        mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let pmt = select! {
            pmt = self.receiver.recv().fuse() => pmt,
            _ = Timer::after(Duration::from_millis(100)).fuse() => None,
        };

        if let Some(pmt) = pmt {
            mio.post(0, pmt).await;
        }

        io.call_again = true;

        Ok(())
    }
}

/// Builder for a [MessageSlider]
pub struct MessageSliderBuilder<T> {
    range: std::ops::RangeInclusive<T>,
    initial_value: T,
    step_size: Option<f64>,
    multiplier: Option<T>,
    label: Option<String>,
    suffix: Option<String>,
}

impl<T: Copy + Clone + Send + Sync + 'static> MessageSliderBuilder<T> {
    /// Start building a new slider
    pub fn new(range: std::ops::RangeInclusive<T>) -> Self {
        let initial_value = *range.start();
        Self {
            range,
            initial_value,
            step_size: None,
            multiplier: None,
            label: None,
            suffix: None,
        }
    }

    /// Set the initial value for the UI element
    pub fn initial_value(mut self, value: T) -> Self {
        self.initial_value = value;
        self
    }

    /// Set the slider's step size
    pub fn step_size(mut self, step: f64) -> Self {
        self.step_size = Some(step);
        self
    }

    /// Set the slider's multiplier, applied to values before being emitted
    /// as messages
    pub fn multiplier(mut self, multiplier: T) -> Self {
        self.multiplier = Some(multiplier);
        self
    }

    /// Sets the label for the slider
    pub fn label<S: ToString>(mut self, label: S) -> Self {
        self.label = Some(label.to_string());
        self
    }

    /// Sets the suffix for the slider's value, for instance a unit
    pub fn suffix<S: ToString>(mut self, suffix: S) -> Self {
        self.suffix = Some(suffix.to_string());
        self
    }

    /// Build the block and return both the block and a handle
    /// for the corresponding GUI widget.
    ///
    /// Use if you want to handle drawing the UI yourself.
    pub fn build_detached(self) -> (Block, MessageSliderHandle<T>) {
        let (to_flowgraph_sender, to_flowgraph_receiver) = channel(10);
        let (from_flowgraph_sender, from_flowgraph_receiver) = channel(10);

        let handle = MessageSliderHandle {
            value: self.initial_value,
            range: self.range,
            step_size: self.step_size,
            multiplier: self.multiplier,
            label: self.label,
            suffix: self.suffix,
            sender: to_flowgraph_sender,
            receiver: from_flowgraph_receiver,
        };

        let block = MessageSlider::<T>::new(from_flowgraph_sender, to_flowgraph_receiver);

        (block, handle)
    }

    /// Build the block, leaving the GUI widget attached. In order to
    /// draw the UI, pass the flowgraph to [crate::gui::Gui::run].
    pub fn build(self) -> Block
    where
        MessageSliderHandle<T>: GuiWidget,
    {
        let (mut block, handle) = self.build_detached();
        block.attach_gui_handle(Box::new(handle));
        block
    }
}
