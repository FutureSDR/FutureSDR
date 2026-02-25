Custom Routes Example (custom-routes)
========================

## Introduction

This example demonstrates how to integrate a custom web server directly into the FutureSDR runtime. It shows the ability to trigger and manage flowgraphs through standard HTTP requests.

## How it works:
* Two routes are defined:
1. `GET /my_route/`: Serves a simple, static HTML page.
2. `GET /start_fg/`: A dynamic route that creates and starts a new flowgraph consisting of a `MessageSource`.

## How to run:
* Go to the path of the example and run it:

```sh
cargo run --release
  ```

* Open your browser and visit `http://127.0.0.1:1337/my_route/`. You will see a "My Custom Route" heading.
* To trigger a flowgraph, visit `http://127.0.0.1:1337/start_fg/`. The browser sends a request to the server and the web handler inject a new flowgraph into the running system. You will see debug information about the new flowgraph appearing in your terminal.
