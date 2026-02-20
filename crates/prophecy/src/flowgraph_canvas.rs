use std::collections::{BTreeMap, HashMap};

use futuresdr_types::{BlockDescription, FlowgraphDescription};
use leptos::html::Div;
use leptos::prelude::*;

const BLOCK_WIDTH: f64 = 180.0;
const TITLE_HEIGHT: f64 = 44.0;
const PORT_HEIGHT: f64 = 24.0;
const COLUMN_GAP: f64 = 120.0;
const ROW_GAP: f64 = 40.0;
const CANVAS_PADDING: f64 = 40.0;
const BEZIER_OFFSET: f64 = 60.0;

struct BlockLayout {
    x: f64,
    y: f64,
    height: f64,
}

fn block_height(b: &BlockDescription) -> f64 {
    let stream_rows = b.stream_inputs.len().max(b.stream_outputs.len());
    let msg_rows = b.message_inputs.len().max(b.message_outputs.len());
    TITLE_HEIGHT + (stream_rows.max(1) as f64) * PORT_HEIGHT + (msg_rows as f64) * PORT_HEIGHT
}

fn assign_columns(fg: &FlowgraphDescription) -> HashMap<usize, usize> {
    let mut cols: HashMap<usize, usize> = fg.blocks.iter().map(|b| (b.id.0, 0)).collect();

    let mut has_stream: HashMap<usize, bool> = fg.blocks.iter().map(|b| (b.id.0, false)).collect();
    for e in &fg.stream_edges {
        has_stream.insert(e.0.0, true);
        has_stream.insert(e.2.0, true);
    }

    let n = fg.blocks.len();
    for _ in 0..n {
        for e in &fg.stream_edges {
            let src = e.0.0;
            let dst = e.2.0;
            let new_col = cols[&src] + 1;
            let entry = cols.entry(dst).or_insert(0);
            if new_col > *entry {
                *entry = new_col;
            }
        }
    }

    for b in &fg.blocks {
        if !has_stream[&b.id.0] {
            cols.insert(b.id.0, usize::MAX);
        }
    }

    cols
}

fn compute_layout(fg: &FlowgraphDescription) -> HashMap<usize, BlockLayout> {
    let cols = assign_columns(fg);
    let block_map: HashMap<usize, &BlockDescription> =
        fg.blocks.iter().map(|b| (b.id.0, b)).collect();

    let mut col_blocks: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut msg_only_blocks: Vec<usize> = Vec::new();

    for b in &fg.blocks {
        let col = cols[&b.id.0];
        if col == usize::MAX {
            msg_only_blocks.push(b.id.0);
        } else {
            col_blocks.entry(col).or_default().push(b.id.0);
        }
    }

    let mut layouts: HashMap<usize, BlockLayout> = HashMap::new();

    let mut col_x = CANVAS_PADDING;
    let mut col_xs: BTreeMap<usize, f64> = BTreeMap::new();
    for col in col_blocks.keys() {
        col_xs.insert(*col, col_x);
        col_x += BLOCK_WIDTH + COLUMN_GAP;
    }

    for (col, block_ids) in &col_blocks {
        let x = col_xs[col];
        let mut y = CANVAS_PADDING;
        for &bid in block_ids {
            let b = block_map[&bid];
            let h = block_height(b);
            layouts.insert(bid, BlockLayout { x, y, height: h });
            y += h + ROW_GAP;
        }
    }

    let msg_x = col_x;
    let mut y = CANVAS_PADDING;
    for bid in msg_only_blocks {
        let b = block_map[&bid];
        let h = block_height(b);
        layouts.insert(bid, BlockLayout { x: msg_x, y, height: h });
        y += h + ROW_GAP;
    }

    layouts
}

fn canvas_size(layouts: &HashMap<usize, BlockLayout>) -> (f64, f64) {
    if layouts.is_empty() {
        return (400.0, 200.0);
    }
    let w = layouts
        .values()
        .map(|l| l.x + BLOCK_WIDTH + CANVAS_PADDING)
        .fold(0.0f64, f64::max);
    let h = layouts
        .values()
        .map(|l| l.y + l.height + CANVAS_PADDING)
        .fold(0.0f64, f64::max);
    (w.max(400.0), h.max(200.0))
}

fn stream_row_count(b: &BlockDescription) -> usize {
    b.stream_inputs.len().max(b.stream_outputs.len()).max(1)
}

fn bezier_path(x1: f64, y1: f64, x2: f64, y2: f64) -> String {
    let cx1 = x1 + BEZIER_OFFSET;
    let cx2 = x2 - BEZIER_OFFSET;
    format!("M {x1},{y1} C {cx1},{y1} {cx2},{y2} {x2},{y2}")
}

// Drag state: (block_id, mouse_x0, mouse_y0, block_x0, block_y0)
type DragState = Option<(usize, f64, f64, f64, f64)>;

fn render_block_node(
    b: BlockDescription,
    pos: RwSignal<(f64, f64)>,
    dragging: RwSignal<DragState>,
) -> impl IntoView {
    let stream_rows = stream_row_count(&b);
    let has_msg = !b.message_inputs.is_empty() || !b.message_outputs.is_empty();

    let stream_port_rows: Vec<_> = (0..stream_rows)
        .map(|i| {
            let in_name = b.stream_inputs.get(i).cloned().unwrap_or_default();
            let out_name = b.stream_outputs.get(i).cloned().unwrap_or_default();
            let has_in = b.stream_inputs.get(i).is_some();
            let has_out = b.stream_outputs.get(i).is_some();
            let row_style = format!(
                "display: flex; align-items: center; height: {}px;",
                PORT_HEIGHT
            );
            view! {
                <div style=row_style>
                    {if has_in {
                        view! {
                            <div style="display: flex; align-items: center; flex: 1; min-width: 0;">
                                <div style="width: 10px; height: 10px; border-radius: 50%; background: #60a5fa; margin-left: -5px; flex-shrink: 0;"></div>
                                <span style="font-size: 11px; color: #94a3b8; padding-left: 4px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{in_name}</span>
                            </div>
                        }
                        .into_any()
                    } else {
                        view! { <div style="flex: 1;"></div> }.into_any()
                    }}
                    {if has_out {
                        view! {
                            <div style="display: flex; align-items: center; justify-content: flex-end; flex: 1; min-width: 0;">
                                <span style="font-size: 11px; color: #94a3b8; padding-right: 4px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{out_name}</span>
                                <div style="width: 10px; height: 10px; border-radius: 50%; background: #60a5fa; margin-right: -5px; flex-shrink: 0;"></div>
                            </div>
                        }
                        .into_any()
                    } else {
                        view! { <div style="flex: 1;"></div> }.into_any()
                    }}
                </div>
            }
        })
        .collect();

    let msg_rows = b.message_inputs.len().max(b.message_outputs.len());
    let msg_port_rows: Vec<_> = (0..msg_rows)
        .map(|i| {
            let in_name = b.message_inputs.get(i).cloned().unwrap_or_default();
            let out_name = b.message_outputs.get(i).cloned().unwrap_or_default();
            let has_in = b.message_inputs.get(i).is_some();
            let has_out = b.message_outputs.get(i).is_some();
            let row_style = format!(
                "display: flex; align-items: center; height: {}px;",
                PORT_HEIGHT
            );
            view! {
                <div style=row_style>
                    {if has_in {
                        view! {
                            <div style="display: flex; align-items: center; flex: 1; min-width: 0;">
                                <div style="width: 10px; height: 10px; background: #f59e0b; margin-left: -5px; flex-shrink: 0;"></div>
                                <span style="font-size: 11px; color: #94a3b8; padding-left: 4px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{in_name}</span>
                            </div>
                        }
                        .into_any()
                    } else {
                        view! { <div style="flex: 1;"></div> }.into_any()
                    }}
                    {if has_out {
                        view! {
                            <div style="display: flex; align-items: center; justify-content: flex-end; flex: 1; min-width: 0;">
                                <span style="font-size: 11px; color: #94a3b8; padding-right: 4px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{out_name}</span>
                                <div style="width: 10px; height: 10px; background: #f59e0b; margin-right: -5px; flex-shrink: 0;"></div>
                            </div>
                        }
                        .into_any()
                    } else {
                        view! { <div style="flex: 1;"></div> }.into_any()
                    }}
                </div>
            }
        })
        .collect();

    let blocking = b.blocking;
    let instance_name = b.instance_name.clone();
    let type_name = b.type_name.clone();
    let bid = b.id.0;

    let outer_style = move || {
        let (x, y) = pos.get();
        format!(
            "position: absolute; left: {x}px; top: {y}px; width: {BLOCK_WIDTH}px; \
             overflow: visible; border-radius: 6px; border: 1px solid #64748b; \
             background: #475569; cursor: grab; user-select: none;"
        )
    };

    let header_style = format!(
        "background: #334155; border-radius: 6px 6px 0 0; padding: 4px 8px; \
         height: {TITLE_HEIGHT}px; overflow: hidden;"
    );

    let on_mousedown = move |ev: web_sys::MouseEvent| {
        ev.prevent_default();
        ev.stop_propagation();
        let (bx, by) = pos.get_untracked();
        dragging.set(Some((
            bid,
            ev.client_x() as f64,
            ev.client_y() as f64,
            bx,
            by,
        )));
    };

    view! {
        <div style=outer_style on:mousedown=on_mousedown>
            <div style=header_style>
                <div style="display: flex; align-items: center; gap: 4px;">
                    <span style="color: #e2e8f0; font-size: 13px; font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1;">
                        {instance_name}
                    </span>
                    {if blocking {
                        view! {
                            <span style="background: #7c3aed; color: white; font-size: 10px; padding: 1px 4px; border-radius: 3px; flex-shrink: 0;">
                                "B"
                            </span>
                        }
                        .into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }}
                </div>
                <div style="color: #94a3b8; font-size: 11px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">
                    {type_name}
                </div>
            </div>
            <div style="padding: 0 8px;">
                {stream_port_rows}
            </div>
            {if has_msg {
                view! {
                    <div style="border-top: 1px solid #64748b; padding: 0 8px;">
                        {msg_port_rows}
                    </div>
                }
                .into_any()
            } else {
                view! { <span></span> }.into_any()
            }}
        </div>
    }
}

#[component]
/// Flowgraph Canvas
pub fn FlowgraphCanvas(fg: FlowgraphDescription) -> impl IntoView {
    let layouts = compute_layout(&fg);
    let (canvas_w, canvas_h) = canvas_size(&layouts);

    // Reactive position per block
    let positions: HashMap<usize, RwSignal<(f64, f64)>> = layouts
        .iter()
        .map(|(&id, l)| (id, RwSignal::new((l.x, l.y))))
        .collect();

    // Drag state: (block_id, mouse_x0, mouse_y0, block_x0, block_y0)
    let dragging: RwSignal<DragState> = RwSignal::new(None);

    // Zoom + pan state. Transform applied to inner div:
    //   transform: translate(pan_x, pan_y) scale(scale)  with transform-origin: 0 0
    // Canvas point (cx, cy) → viewport (pan_x + cx*scale, pan_y + cy*scale)
    let scale: RwSignal<f64> = RwSignal::new(1.0);
    let pan: RwSignal<(f64, f64)> = RwSignal::new((0.0, 0.0));

    let container_ref = NodeRef::<Div>::new();

    let block_info: HashMap<usize, BlockDescription> =
        fg.blocks.iter().map(|b| (b.id.0, b.clone())).collect();

    // Build block nodes
    let block_nodes: Vec<_> = fg
        .blocks
        .iter()
        .filter_map(|b| {
            let pos = *positions.get(&b.id.0)?;
            Some(render_block_node(b.clone(), pos, dragging))
        })
        .collect();

    // Stream edge paths — reactive closures read position signals
    let stream_paths: Vec<_> = fg
        .stream_edges
        .iter()
        .filter_map(|e| {
            let src_id = e.0.0;
            let src_port = e.1.name().to_string();
            let dst_id = e.2.0;
            let dst_port = e.3.name().to_string();

            let src_block = block_info.get(&src_id)?;
            let dst_block = block_info.get(&dst_id)?;

            let src_idx = src_block
                .stream_outputs
                .iter()
                .position(|p| p == &src_port)?;
            let dst_idx = dst_block
                .stream_inputs
                .iter()
                .position(|p| p == &dst_port)?;

            let src_pos = *positions.get(&src_id)?;
            let dst_pos = *positions.get(&dst_id)?;
            let si = src_idx as f64;
            let di = dst_idx as f64;

            let d = move || {
                let (sx, sy) = src_pos.get();
                let (dx, dy) = dst_pos.get();
                bezier_path(
                    sx + BLOCK_WIDTH,
                    sy + TITLE_HEIGHT + si * PORT_HEIGHT + PORT_HEIGHT / 2.0,
                    dx,
                    dy + TITLE_HEIGHT + di * PORT_HEIGHT + PORT_HEIGHT / 2.0,
                )
            };
            Some(view! { <path d=d stroke="#94a3b8" stroke-width="2" fill="none" /> })
        })
        .collect();

    // Message edge paths — reactive closures read position signals
    let message_paths: Vec<_> = fg
        .message_edges
        .iter()
        .filter_map(|e| {
            let src_id = e.0.0;
            let src_port = e.1.name().to_string();
            let dst_id = e.2.0;
            let dst_port = e.3.name().to_string();

            let src_block = block_info.get(&src_id)?;
            let dst_block = block_info.get(&dst_id)?;

            let src_idx = src_block
                .message_outputs
                .iter()
                .position(|p| p == &src_port)?;
            let dst_idx = dst_block
                .message_inputs
                .iter()
                .position(|p| p == &dst_port)?;

            let src_pos = *positions.get(&src_id)?;
            let dst_pos = *positions.get(&dst_id)?;
            let src_sr = stream_row_count(src_block) as f64;
            let dst_sr = stream_row_count(dst_block) as f64;
            let si = src_idx as f64;
            let di = dst_idx as f64;

            let d = move || {
                let (sx, sy) = src_pos.get();
                let (dx, dy) = dst_pos.get();
                bezier_path(
                    sx + BLOCK_WIDTH,
                    sy + TITLE_HEIGHT + src_sr * PORT_HEIGHT + si * PORT_HEIGHT + PORT_HEIGHT / 2.0,
                    dx,
                    dy + TITLE_HEIGHT + dst_sr * PORT_HEIGHT + di * PORT_HEIGHT + PORT_HEIGHT / 2.0,
                )
            };
            Some(view! {
                <path d=d stroke="#f59e0b" stroke-width="1.5" fill="none" stroke-dasharray="6,3" />
            })
        })
        .collect();

    // Clone positions map for the mousemove handler (RwSignal is Copy, so HashMap clone
    // produces independent copies of the keys/values pointing to the same signals)
    let positions_for_move = positions.clone();

    // Panning state: (mouse_x0, mouse_y0, pan_x0, pan_y0)
    let panning: RwSignal<Option<(f64, f64, f64, f64)>> = RwSignal::new(None);

    let on_container_mousedown = move |ev: web_sys::MouseEvent| {
        ev.prevent_default();
        let (ox, oy) = pan.get_untracked();
        panning.set(Some((ev.client_x() as f64, ev.client_y() as f64, ox, oy)));
    };

    let on_mousemove = move |ev: web_sys::MouseEvent| {
        // Block drag: divide viewport delta by scale to get canvas-space delta
        if let Some((bid, mx0, my0, bx0, by0)) = dragging.get_untracked() {
            let s = scale.get_untracked();
            let dx = (ev.client_x() as f64 - mx0) / s;
            let dy = (ev.client_y() as f64 - my0) / s;
            if let Some(&pos) = positions_for_move.get(&bid) {
                pos.set((bx0 + dx, by0 + dy));
            }
        }
        // Canvas pan: viewport delta applied directly to pan offset
        if let Some((mx0, my0, ox0, oy0)) = panning.get_untracked() {
            let dx = ev.client_x() as f64 - mx0;
            let dy = ev.client_y() as f64 - my0;
            pan.set((ox0 + dx, oy0 + dy));
        }
    };

    let on_mouseup = move |_: web_sys::MouseEvent| {
        dragging.set(None);
        panning.set(None);
    };
    let on_mouseleave = move |_: web_sys::MouseEvent| {
        dragging.set(None);
        panning.set(None);
    };

    let on_wheel = move |ev: web_sys::WheelEvent| {
        ev.prevent_default();
        let old_scale = scale.get_untracked();
        let factor = if ev.delta_y() > 0.0 { 1.0 / 1.1 } else { 1.1 };
        let new_scale = (old_scale * factor).clamp(0.1, 5.0);
        let ratio = new_scale / old_scale;

        // Zoom toward the cursor: keep the canvas point under the mouse fixed.
        // With transform translate(ox,oy) scale(s) and transform-origin 0 0:
        //   viewport pos = (ox + cx*s, oy + cy*s)
        // After zoom, same canvas point stays at same viewport pos:
        //   ox' = mx*(1 - ratio) + ox*ratio   (mx = mouse pos relative to container)
        if let Some(el) = container_ref.get() {
            let rect = el.get_bounding_client_rect();
            let mx = ev.client_x() as f64 - rect.left();
            let my = ev.client_y() as f64 - rect.top();
            let (ox, oy) = pan.get_untracked();
            pan.set((mx * (1.0 - ratio) + ox * ratio, my * (1.0 - ratio) + oy * ratio));
        }

        scale.set(new_scale);
    };

    let container_style = move || {
        if dragging.get().is_some() || panning.get().is_some() {
            "overflow: hidden; background: #0f172a; width: 100%; height: 70vh; cursor: grabbing;"
        } else {
            "overflow: hidden; background: #0f172a; width: 100%; height: 70vh; cursor: grab;"
        }
    };

    let inner_style = move || {
        let s = scale.get();
        let (ox, oy) = pan.get();
        format!(
            "position: relative; width: {canvas_w}px; height: {canvas_h}px; \
             transform-origin: 0 0; transform: translate({ox}px, {oy}px) scale({s});"
        )
    };

    view! {
        <div
            node_ref=container_ref
            style=container_style
            on:mousedown=on_container_mousedown
            on:mousemove=on_mousemove
            on:mouseup=on_mouseup
            on:mouseleave=on_mouseleave
            on:wheel=on_wheel
        >
            <div style=inner_style>
                {block_nodes}
                <svg
                    width=canvas_w
                    height=canvas_h
                    style="position: absolute; top: 0; left: 0; pointer-events: none; overflow: visible;"
                >
                    {stream_paths}
                    {message_paths}
                </svg>
            </div>
        </div>
    }
}
