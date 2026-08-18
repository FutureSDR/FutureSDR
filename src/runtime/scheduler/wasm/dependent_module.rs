use js_sys::Array;
use js_sys::JsString;
use wasm_bindgen::prelude::*;
use web_sys::Blob;
use web_sys::BlobPropertyBag;
use web_sys::Url;

#[wasm_bindgen]
extern "C" {
    type ImportMeta;

    #[wasm_bindgen(method, getter)]
    fn url(this: &ImportMeta) -> JsString;

    #[wasm_bindgen(thread_local_v2, js_namespace = import, js_name = meta)]
    static IMPORT_META: ImportMeta;
}

/// Create an ES module that imports the current wasm-bindgen module before
/// evaluating `code` and return it as a Blob URL.
pub(super) fn create(code: &str) -> Result<String, JsValue> {
    let header = format!(
        "import * as bindgen from '{}';\n\n",
        IMPORT_META.with(ImportMeta::url),
    );
    let options = BlobPropertyBag::new();
    options.set_type("text/javascript");
    let blob = Blob::new_with_str_sequence_and_options(
        &Array::of2(&JsValue::from_str(&header), &JsValue::from_str(code)),
        &options,
    )?;
    Url::create_object_url_with_blob(&blob)
}
