#!/usr/bin/env python3
"""
Live plot 2048 float32 samples coming in as binary WebSocket messages.

Requirements:
  pip install websockets matplotlib numpy

Usage:
  python ws_plot.py ws://localhost:8765 --ylim -1 1
"""

import argparse
import asyncio
import signal
import sys
import threading
from queue import Queue, Empty

import numpy as np
import matplotlib.pyplot as plt

try:
    import websockets  # type: ignore
except ImportError:
    print("Missing dependency: pip install websockets", file=sys.stderr)
    sys.exit(1)


SAMPLES = 2048
BYTES_NEEDED = SAMPLES * 4  # float32


async def _recv_loop(url: str, out_q: Queue, stop_flag: threading.Event):
    """
    Async loop: connect and push latest arrays (np.float32, len 2048) to out_q.
    """
    # Reconnect loop for resilience
    while not stop_flag.is_set():
        try:
            async with websockets.connect(url, max_size=None) as ws:
                # Receive until asked to stop
                while not stop_flag.is_set():
                    msg = await ws.recv()  # bytes or str
                    if isinstance(msg, str):
                        # Ignore text frames; we expect binary frames only
                        continue
                    if len(msg) < BYTES_NEEDED:
                        # Skip incomplete frames
                        continue

                    # Interpret first 2048 float32 samples (little-endian by default).
                    # If your sender uses big-endian, change dtype to '>f4'.
                    arr = np.frombuffer(msg, dtype='<f4', count=SAMPLES)
                    # Put latest data; if queue is full, drop the oldest
                    try:
                        out_q.put_nowait(arr)
                    except Exception:
                        # Queue might be full; clear and put latest
                        try:
                            while True:
                                out_q.get_nowait()
                        except Empty:
                            pass
                        out_q.put_nowait(arr)
        except asyncio.CancelledError:
            break
        except Exception as e:
            # Brief backoff before reconnect
            print(f"[recv_loop] connection error: {e}", file=sys.stderr)
            await asyncio.sleep(1.0)

    # Done
    return


def _start_asyncio_consumer(url: str, out_q: Queue, stop_flag: threading.Event) -> threading.Thread:
    """
    Run the asyncio receiver in a dedicated thread so Matplotlib can own the main thread.
    """
    def runner():
        try:
            asyncio.run(_recv_loop(url, out_q, stop_flag))
        except RuntimeError as e:
            # In case an event loop already exists (rare), fall back to manual loop
            loop = asyncio.new_event_loop()
            asyncio.set_event_loop(loop)
            loop.run_until_complete(_recv_loop(url, out_q, stop_flag))
            loop.close()

    t = threading.Thread(target=runner, name="ws-recv", daemon=True)
    t.start()
    return t


def main():
    ap = argparse.ArgumentParser(description="Plot 2048 float32 samples from a WebSocket (binary frames).")
    ap.add_argument("url", help="WebSocket URL, e.g. ws://127.0.0.1:8765/stream")
    ap.add_argument("--ylim", nargs=2, type=float, metavar=("YMIN", "YMAX"),
                    help="Lock Y-axis limits (default: autoscale each update).")
    ap.add_argument("--title", default="WebSocket Stream (2048 × f32)",
                    help="Figure title.")
    ap.add_argument("--fps", type=float, default=30.0,
                    help="Target UI update rate (frames/sec).")
    args = ap.parse_args()

    # Shared queue for latest arrays
    q: Queue[np.ndarray] = Queue(maxsize=4)
    stop_flag = threading.Event()

    # Start async consumer thread
    recv_thread = _start_asyncio_consumer(args.url, q, stop_flag)

    # Prepare plot (main thread)
    x = np.arange(SAMPLES)
    y = np.zeros(SAMPLES, dtype=np.float32)

    fig, ax = plt.subplots()
    line, = ax.plot(x, y, lw=1.0)
    ax.set_xlim(0, SAMPLES - 1)
    ax.set_xlabel("Sample")
    ax.set_ylabel("Amplitude")
    ax.set_title(args.title)

    if args.ylim:
        ax.set_ylim(args.ylim[0], args.ylim[1])
        autoscale_y = False
    else:
        autoscale_y = True

    # Update function for animation timer
    def on_timer(_event=None):
        updated = False
        # Drain the queue; use the most recent frame
        while True:
            try:
                arr = q.get_nowait()
            except Empty:
                break
            else:
                y[:] = arr[:SAMPLES]
                updated = True

        if updated:
            line.set_ydata(y)
            if autoscale_y:
                # Simple autoscale with margin
                ymin = float(np.min(y))
                ymax = float(np.max(y))
                if ymin == ymax:
                    ymin -= 1.0
                    ymax += 1.0
                margin = 0.05 * (ymax - ymin)
                ax.set_ylim(ymin - margin, ymax + margin)
            fig.canvas.draw_idle()

    # Timer for UI refresh (kept independent from incoming rate)
    interval_ms = max(1, int(1000.0 / args.fps))
    timer = fig.canvas.new_timer(interval=interval_ms)
    timer.add_callback(on_timer)
    timer.start()

    # Graceful shutdown on Ctrl-C / window close
    def _graceful_exit(*_a):
        stop_flag.set()
        plt.close(fig)

    signal.signal(signal.SIGINT, _graceful_exit)

    try:
        plt.show()
    finally:
        stop_flag.set()
        recv_thread.join(timeout=2.0)


if __name__ == "__main__":
    main()
