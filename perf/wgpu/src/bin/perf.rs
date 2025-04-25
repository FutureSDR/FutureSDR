use anyhow::Result;

#[cfg(not(target_arch = "wasm32"))]
mod foo {
    use anyhow::Result;
    use clap::Parser;

    #[derive(Parser, Debug)]
    struct Args {
        #[clap(short, long, default_value_t = 0)]
        run: u64,
        #[clap(short = 'S', long, default_value = "smol1")]
        scheduler: String,
        #[clap(short, long, default_value_t = 1000000)]
        samples: u64,
        #[clap(short, long, default_value_t = 4096)]
        buffer_size: u64,
    }

    pub fn main() -> Result<()> {
        let Args {
            run,
            scheduler,
            samples,
            buffer_size,
        } = Args::parse();

        futuresdr::async_io::block_on(perf_wgpu::run(run, scheduler, samples, buffer_size))?;
        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
mod foo {
    use anyhow::Result;
    use leptos::prelude::*;
    use leptos::task::spawn_local;

    #[component]
    /// Main GUI
    fn Gui() -> impl IntoView {
        let start = move |_| {
            spawn_local(async {
                perf_wgpu::run(0, "wasm".to_string(), 1000000, 32768)
                    .await
                    .unwrap();
            });
        };
        view! {
            <h1>"FutureSDR WGPU Perf"</h1>
            <button on:click=start type="button" class="bg-fs-blue hover:brightness-75 text-slate-200 font-bold py-2 px-4 rounded">Start</button>
        }
    }

    pub fn main() -> Result<()> {
        console_error_panic_hook::set_once();
        mount_to_body(|| view! { <Gui /> });
        Ok(())
    }
}

fn main() -> Result<()> {
    foo::main()
}
