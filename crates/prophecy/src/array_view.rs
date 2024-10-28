//! Transformation of Rust arrays to JS.
// This Code is from maia-sdr
use js_sys::Float32Array;
use js_sys::Object;
use js_sys::Uint16Array;
use js_sys::Uint8Array;
use std::ops::Deref;
use web_sys::WebGl2RenderingContext;

/// Idiomatic transformation of Rust arrays to JS.
///
/// This trait gives an idiomatic way to transform Rust arrays into a JS typed
/// array (such as [`Float32Array`]) specifically for the use case of filling
/// WebGL2 buffers and textures.
///
/// For this, each Rust numeric type is associated with a JS typed array type
/// and with a constant that gives the corresponding WebGL2 data type (such as
/// `WebGL2RenderingContext::FLOAT`).
pub trait ArrayView: Sized {
    /// The associated JS typed array.
    type JS: Deref<Target = Object>;
    /// The associated WebGL2 data type.
    const GL_TYPE: u32;
    /// Creates a JS typed array which is a view into wasm's linear memory at
    /// the slice specified.
    ///
    /// This function uses the `view` method (such as [`Float32Array::view`]) of
    /// the JS typed array.
    ///
    /// # Safety
    ///
    /// The same safety considerations as with [`Float32Array::view`] apply.
    unsafe fn view(rust: &[Self]) -> Self::JS;
}

macro_rules! impl_array_view {
    ($rust:ty, $js:ty, $gl:expr) => {
        impl ArrayView for $rust {
            type JS = $js;
            const GL_TYPE: u32 = $gl;
            unsafe fn view(rust: &[$rust]) -> $js {
                <$js>::view(rust)
            }
        }
    };
}

impl_array_view!(f32, Float32Array, WebGl2RenderingContext::FLOAT);
impl_array_view!(u16, Uint16Array, WebGl2RenderingContext::UNSIGNED_INT);
impl_array_view!(u8, Uint8Array, WebGl2RenderingContext::UNSIGNED_BYTE);
