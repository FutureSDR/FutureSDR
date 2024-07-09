use futuresdr::anyhow::Result;

#[cfg(not(target_arch = "wasm32"))]
mod foo {
    use clap::Parser;
    use futuresdr::anyhow::Result;
    use futuresdr::async_io::block_on;

    use cw::run_fg;

    #[derive(Parser, Debug)]
    struct Args {
        /// Sets the message to convert.
        #[arg(short, long, default_value = "CQ CQ CQ FUTURESDR")]
        message: String,
    }

    pub fn main() -> Result<()> {
        let args = Args::parse();
        let msg: String = args.message;

        block_on(run_fg(msg))?;
        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
mod foo {
    use futuresdr::anyhow::Result;
    use leptos::html::Input;
    use leptos::*;

    const ENTER_KEY: u32 = 13;

    #[component]
    fn Gui() -> impl IntoView {
        let input_ref = create_node_ref::<Input>();
        let tx_cw = move || {
            let input = input_ref().unwrap();
            let v = input.value();
            leptos::spawn_local(async move { cw::run_fg(v).await.unwrap() });
        };

        let on_input = move |ev: web_sys::KeyboardEvent| {
            ev.stop_propagation();
            let key_code = ev.key_code();
            if key_code == ENTER_KEY {
                tx_cw();
            }
        };

        view! {
            <h1 class="p-4 text-4xl font-extrabold text-gray-900">"FutureSDR CW Transmitter"</h1>
            <input class="p-4 m-4" node_ref=input_ref on:keydown=on_input></input>
            <button on:click=move |_| tx_cw() type="button" class="bg-fs-blue hover:brightness-75 text-slate-200 font-bold p-4 m-4 rounded">Start</button>
        }
    }

    pub fn main() -> Result<()> {
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
        mount_to_body(|| view! { <Gui /> });
        Ok(())
    }
}

fn main() -> Result<()> {
    foo::main()
}
