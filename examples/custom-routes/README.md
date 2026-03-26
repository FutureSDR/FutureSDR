# Custom Routes Example

## Introduction

This example demonstrates how to add custom handlers for the integrated web server. You can use this to add custom REST endpoints to serve pages or trigger actions. In this example, a flowgraph is spawned on the runtime.

If you only want to serve a custom web frontend, you can set `frontend_path` in your `config.toml`. See the [documentation](https://www.futuresdr.org/learn/flowgraph_interaction.html#web-ui) for more information.

## How It Works

Two routes are defined:
1. `GET /my_route/`: Serves a simple static HTML page.
2. `GET /start_fg/`: A dynamic route that creates and starts a new flowgraph on the runtime.

## How to Run

Go to the example directory and run:

```sh
cargo run --release
```

Open your browser and visit `http://127.0.0.1:1337/my_route/`. You will see a "My Custom Route" heading.

To trigger a flowgraph, visit `http://127.0.0.1:1337/start_fg/`. The browser sends a request to the server, and the handler starts a new flowgraph on the runtime. You will see debug information about the new flowgraph in your terminal.
