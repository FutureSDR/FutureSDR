use std::collections::HashMap;

use futuresdr_types::Pmt;
use leptos::html::Input;
use leptos::html::Select;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::FlowgraphHandle;

const MAX_SELECT_VALUES: usize = 10;

#[derive(Clone, Debug)]
struct SourceDescription {
    capabilities: SourceCapabilities,
    config: SourceConfig,
}

#[derive(Clone, Debug)]
struct SourceCapabilities {
    antenna: Option<Vec<String>>,
    bandwidth: Option<NumericRange>,
    freq: Option<NumericRange>,
    gain: Option<NumericRange>,
    sample_rate: Option<NumericRange>,
}

#[derive(Clone, Debug)]
struct SourceConfig {
    antenna: Option<String>,
    bandwidth: Option<f64>,
    freq: Option<f64>,
    gain: Option<f64>,
    sample_rate: Option<f64>,
}

#[derive(Clone, Debug)]
struct NumericRange {
    items: Vec<NumericRangeItem>,
}

#[derive(Clone, Debug)]
enum NumericRangeItem {
    Interval(f64, f64),
    Value(f64),
    Step(f64, f64, f64),
}

impl NumericRange {
    fn discrete_values(&self, limit: usize) -> Option<Vec<f64>> {
        let mut values = Vec::new();
        for item in &self.items {
            match *item {
                NumericRangeItem::Value(value) => push_unique(&mut values, value),
                NumericRangeItem::Interval(min, max) if nearly_equal(min, max) => {
                    push_unique(&mut values, min);
                }
                NumericRangeItem::Interval(_, _) => return None,
                NumericRangeItem::Step(min, max, step) => {
                    if !min.is_finite()
                        || !max.is_finite()
                        || !step.is_finite()
                        || max < min
                        || step <= 0.0
                    {
                        return None;
                    }
                    let count = ((max - min) / step).floor() + 1.0;
                    if !count.is_finite() {
                        return None;
                    }
                    for index in 0..count as usize {
                        let value = min + index as f64 * step;
                        push_unique(&mut values, value);
                        if values.len() > limit {
                            return None;
                        }
                    }
                }
            }
            if values.len() > limit {
                return None;
            }
        }

        values.sort_by(f64::total_cmp);
        values.dedup_by(|left, right| nearly_equal(*left, *right));
        (!values.is_empty() && values.len() <= limit).then_some(values)
    }

    fn contains(&self, value: f64) -> bool {
        self.items.iter().any(|item| match *item {
            NumericRangeItem::Interval(min, max) => min <= value && value <= max,
            NumericRangeItem::Value(allowed) => nearly_equal(value, allowed),
            NumericRangeItem::Step(min, max, step) => {
                if value < min || value > max || step <= 0.0 {
                    false
                } else {
                    nearly_equal((value - min) / step, ((value - min) / step).round())
                }
            }
        })
    }

    fn description(&self, scale: f64, unit: &str) -> String {
        self.items
            .iter()
            .map(|item| match *item {
                NumericRangeItem::Interval(min, max) => format!(
                    "{}–{} {unit}",
                    format_number(min / scale),
                    format_number(max / scale)
                ),
                NumericRangeItem::Value(value) => {
                    format!("{} {unit}", format_number(value / scale))
                }
                NumericRangeItem::Step(min, max, step) => format!(
                    "{}–{} {unit} in {} {unit} steps",
                    format_number(min / scale),
                    format_number(max / scale),
                    format_number(step / scale)
                ),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Capability-driven controls for a Seify source block.
///
/// The component queries the source's `capabilities` and `config` message
/// handlers through either an in-browser or remote [`FlowgraphHandle`].
/// Settings with at most ten discrete values use a select box; larger or
/// continuous ranges use a numeric input.
#[component]
pub fn SeifySource(
    fg_handle: FlowgraphHandle,
    block_id: usize,
    #[prop(default = 0)] channel: usize,
) -> impl IntoView {
    let description = LocalResource::new({
        let fg_handle = fg_handle.clone();
        move || {
            let fg_handle = fg_handle.clone();
            async move { query_source(&fg_handle, block_id, channel).await }
        }
    });

    view! {
        <div class="basis-full">
            {move || match description.get() {
                None => view! {
                    <div class="text-sm text-slate-400">"Loading radio controls…"</div>
                }
                    .into_any(),
                Some(Err(error)) => view! {
                    <div class="text-sm text-red-400">
                        {format!("Unable to load radio controls: {error}")}
                    </div>
                }
                    .into_any(),
                Some(Ok(description)) => {
                    let capabilities = description.capabilities;
                    let config = description.config;
                    view! {
                        <div class="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
                            {capabilities
                                .antenna
                                .map(|values| {
                                    view! {
                                        <StringSetting
                                            fg_handle=fg_handle.clone()
                                            block_id=block_id
                                            channel=channel
                                            name="antenna"
                                            label="Antenna"
                                            values=values
                                            current=config.antenna.clone()
                                        />
                                    }
                                        .into_any()
                                })}
                            {capabilities
                                .freq
                                .map(|range| {
                                    view! {
                                        <NumericSetting
                                            fg_handle=fg_handle.clone()
                                            block_id=block_id
                                            channel=channel
                                            name="freq"
                                            label="Frequency"
                                            unit="MHz"
                                            scale=1e6
                                            range=range
                                            current=config.freq
                                        />
                                    }
                                        .into_any()
                                })}
                            {capabilities
                                .gain
                                .map(|range| {
                                    view! {
                                        <NumericSetting
                                            fg_handle=fg_handle.clone()
                                            block_id=block_id
                                            channel=channel
                                            name="gain"
                                            label="Gain"
                                            unit="dB"
                                            scale=1.0
                                            range=range
                                            current=config.gain
                                        />
                                    }
                                        .into_any()
                                })}
                            {capabilities
                                .sample_rate
                                .map(|range| {
                                    view! {
                                        <NumericSetting
                                            fg_handle=fg_handle.clone()
                                            block_id=block_id
                                            channel=channel
                                            name="sample_rate"
                                            label="Sample Rate"
                                            unit="MHz"
                                            scale=1e6
                                            range=range
                                            current=config.sample_rate
                                        />
                                    }
                                        .into_any()
                                })}
                            {capabilities
                                .bandwidth
                                .map(|range| {
                                    view! {
                                        <NumericSetting
                                            fg_handle=fg_handle.clone()
                                            block_id=block_id
                                            channel=channel
                                            name="bandwidth"
                                            label="Bandwidth"
                                            unit="MHz"
                                            scale=1e6
                                            range=range
                                            current=config.bandwidth
                                        />
                                    }
                                        .into_any()
                                })}
                        </div>
                    }
                        .into_any()
                }
            }}
        </div>
    }
}

#[component]
fn NumericSetting(
    fg_handle: FlowgraphHandle,
    block_id: usize,
    channel: usize,
    name: &'static str,
    label: &'static str,
    unit: &'static str,
    scale: f64,
    range: NumericRange,
    current: Option<f64>,
) -> impl IntoView {
    let (current, set_current) = signal(current);
    let (busy, set_busy) = signal(false);
    let (error, set_error) = signal(None::<String>);
    let range_for_submit = range.clone();
    let submit = Callback::new(move |value: f64| {
        if !value.is_finite() || !range_for_submit.contains(value) {
            set_error(Some(format!(
                "Value must be within {}",
                range_for_submit.description(scale, unit)
            )));
            return;
        }

        set_busy(true);
        set_error(None);
        let fg_handle = fg_handle.clone();
        spawn_local(async move {
            match update_setting(&fg_handle, block_id, channel, name, Pmt::F64(value)).await {
                Ok(value) => match pmt_number(&value) {
                    Some(value) => set_current(Some(value)),
                    None => set_error(Some("Source returned a non-numeric value".to_string())),
                },
                Err(error) => set_error(Some(error)),
            }
            set_busy(false);
        });
    });

    let control = match range.discrete_values(MAX_SELECT_VALUES) {
        Some(values) => {
            let select_ref = NodeRef::<Select>::new();
            view! {
                <select
                    node_ref=select_ref
                    class="w-full rounded border border-slate-600 bg-slate-800 px-3 py-2 text-white disabled:opacity-50"
                    disabled=move || busy.get()
                    on:change=move |_| {
                        if let Some(select) = select_ref.get()
                            && let Ok(value) = select.value().parse::<f64>()
                        {
                            submit.run(value);
                        }
                    }
                >
                    <option value="" disabled=true selected=move || current.get().is_none()>
                        "Select a value"
                    </option>
                    {values
                        .into_iter()
                        .map(|value| {
                            view! {
                                <option
                                    value=value.to_string()
                                    selected=move || current.get().is_some_and(|x| nearly_equal(x, value))
                                >
                                    {format!("{} {unit}", format_number(value / scale))}
                                </option>
                            }
                        })
                        .collect::<Vec<_>>()}
                </select>
            }
                .into_any()
        }
        None => {
            let input_ref = NodeRef::<Input>::new();
            let range_description = range.description(scale, unit);
            view! {
                <form
                    class="flex gap-2"
                    on:submit=move |event| {
                        event.prevent_default();
                        let Some(input) = input_ref.get() else {
                            return;
                        };
                        match input.value().trim().parse::<f64>() {
                            Ok(value) => submit.run(value * scale),
                            Err(_) => set_error(Some("Enter a numeric value".to_string())),
                        }
                    }
                >
                    <input
                        node_ref=input_ref
                        type="text"
                        inputmode="decimal"
                        class="min-w-0 flex-1 rounded border border-slate-600 bg-slate-800 px-3 py-2 text-white disabled:opacity-50"
                        prop:value=move || current.get().map(|value| format_number(value / scale)).unwrap_or_default()
                        disabled=move || busy.get()
                        aria-label=format!("{label} in {unit}")
                    />
                    <button
                        type="submit"
                        class="rounded bg-slate-600 px-3 py-2 text-white hover:bg-slate-700 disabled:opacity-50"
                        disabled=move || busy.get()
                    >
                        "Apply"
                    </button>
                </form>
                <div class="mt-1 text-xs text-slate-400">{range_description}</div>
            }
                .into_any()
        }
    };

    view! {
        <div class="rounded border border-slate-700 bg-slate-900 p-3">
            <label class="mb-2 block text-sm text-slate-300">{format!("{label} ({unit})")}</label>
            {control}
            <div class="mt-1 min-h-4 text-xs text-red-400">
                {move || error.get().unwrap_or_default()}
            </div>
        </div>
    }
}

#[component]
fn StringSetting(
    fg_handle: FlowgraphHandle,
    block_id: usize,
    channel: usize,
    name: &'static str,
    label: &'static str,
    values: Vec<String>,
    current: Option<String>,
) -> impl IntoView {
    let allowed_values = values.clone();
    let (current, set_current) = signal(current);
    let (busy, set_busy) = signal(false);
    let (error, set_error) = signal(None::<String>);
    let submit = Callback::new(move |value: String| {
        if !allowed_values.contains(&value) {
            set_error(Some("Unsupported value".to_string()));
            return;
        }

        set_busy(true);
        set_error(None);
        let fg_handle = fg_handle.clone();
        spawn_local(async move {
            match update_setting(&fg_handle, block_id, channel, name, Pmt::String(value)).await {
                Ok(Pmt::String(value)) => set_current(Some(value)),
                Ok(_) => set_error(Some("Source returned a non-text value".to_string())),
                Err(error) => set_error(Some(error)),
            }
            set_busy(false);
        });
    });

    let control = if values.len() <= MAX_SELECT_VALUES {
        let select_ref = NodeRef::<Select>::new();
        view! {
            <select
                node_ref=select_ref
                class="w-full rounded border border-slate-600 bg-slate-800 px-3 py-2 text-white disabled:opacity-50"
                disabled=move || busy.get()
                on:change=move |_| {
                    if let Some(select) = select_ref.get() {
                        submit.run(select.value());
                    }
                }
            >
                <option value="" disabled=true selected=move || current.get().is_none()>
                    "Select a value"
                </option>
                {values
                    .into_iter()
                    .map(|value| {
                        let option_value = value.clone();
                        let selected_value = value.clone();
                        view! {
                            <option
                                value=option_value
                                selected=move || current.get().as_ref() == Some(&selected_value)
                            >
                                {value}
                            </option>
                        }
                    })
                    .collect::<Vec<_>>()}
            </select>
        }
            .into_any()
    } else {
        let input_ref = NodeRef::<Input>::new();
        view! {
            <form
                class="flex gap-2"
                on:submit=move |event| {
                    event.prevent_default();
                    if let Some(input) = input_ref.get() {
                        submit.run(input.value());
                    }
                }
            >
                <input
                    node_ref=input_ref
                    type="text"
                    class="min-w-0 flex-1 rounded border border-slate-600 bg-slate-800 px-3 py-2 text-white disabled:opacity-50"
                    prop:value=move || current.get().unwrap_or_default()
                    disabled=move || busy.get()
                />
                <button
                    type="submit"
                    class="rounded bg-slate-600 px-3 py-2 text-white hover:bg-slate-700 disabled:opacity-50"
                    disabled=move || busy.get()
                >
                    "Apply"
                </button>
            </form>
        }
            .into_any()
    };

    view! {
        <div class="rounded border border-slate-700 bg-slate-900 p-3">
            <label class="mb-2 block text-sm text-slate-300">{label}</label>
            {control}
            <div class="mt-1 min-h-4 text-xs text-red-400">
                {move || error.get().unwrap_or_default()}
            </div>
        </div>
    }
}

async fn query_source(
    fg_handle: &FlowgraphHandle,
    block_id: usize,
    channel: usize,
) -> Result<SourceDescription, String> {
    let channel = Pmt::U64(channel as u64);
    let capabilities = fg_handle
        .call(block_id, "capabilities", channel.clone())
        .await
        .map_err(|error| error.to_string())?;
    let config = fg_handle
        .call(block_id, "config", channel)
        .await
        .map_err(|error| error.to_string())?;

    Ok(SourceDescription {
        capabilities: SourceCapabilities::try_from(capabilities)?,
        config: SourceConfig::try_from(config)?,
    })
}

async fn update_setting(
    fg_handle: &FlowgraphHandle,
    block_id: usize,
    channel: usize,
    name: &str,
    value: Pmt,
) -> Result<Pmt, String> {
    let command = Pmt::MapStrPmt(HashMap::from([
        ("chan".to_string(), Pmt::U64(channel as u64)),
        (name.to_string(), value),
    ]));
    match fg_handle
        .call(block_id, "cmd", command)
        .await
        .map_err(|error| error.to_string())?
    {
        Pmt::Ok => {}
        Pmt::InvalidValue => return Err("Device rejected the value".to_string()),
        response => return Err(format!("Unexpected source response: {response}")),
    }

    let config = fg_handle
        .call(block_id, "config", Pmt::U64(channel as u64))
        .await
        .map_err(|error| error.to_string())?;
    let Pmt::MapStrPmt(mut config) = config else {
        return Err("Source returned an invalid configuration".to_string());
    };
    config
        .remove(name)
        .ok_or_else(|| format!("Source did not return its {name} setting"))
}

impl TryFrom<Pmt> for SourceCapabilities {
    type Error = String;

    fn try_from(value: Pmt) -> Result<Self, Self::Error> {
        let Pmt::MapStrPmt(mut map) = value else {
            return Err("source returned invalid capabilities".to_string());
        };
        Ok(Self {
            antenna: map
                .remove("antenna")
                .map(string_values)
                .transpose()?
                .filter(|values| !values.is_empty()),
            bandwidth: map
                .remove("bandwidth")
                .map(NumericRange::try_from)
                .transpose()?,
            freq: map.remove("freq").map(NumericRange::try_from).transpose()?,
            gain: map.remove("gain").map(NumericRange::try_from).transpose()?,
            sample_rate: map
                .remove("sample_rate")
                .map(NumericRange::try_from)
                .transpose()?,
        })
    }
}

impl TryFrom<Pmt> for SourceConfig {
    type Error = String;

    fn try_from(value: Pmt) -> Result<Self, Self::Error> {
        let Pmt::MapStrPmt(mut map) = value else {
            return Err("source returned an invalid configuration".to_string());
        };
        Ok(Self {
            antenna: optional_string(map.remove("antenna"))?,
            bandwidth: optional_number(map.remove("bandwidth"))?,
            freq: optional_number(map.remove("freq"))?,
            gain: optional_number(map.remove("gain"))?,
            sample_rate: optional_number(map.remove("sample_rate"))?,
        })
    }
}

impl TryFrom<Pmt> for NumericRange {
    type Error = String;

    fn try_from(value: Pmt) -> Result<Self, Self::Error> {
        let Pmt::VecPmt(items) = value else {
            return Err("source returned an invalid numeric range".to_string());
        };
        let items = items
            .into_iter()
            .map(|item| match item {
                Pmt::F64(value) => Ok(NumericRangeItem::Value(value)),
                Pmt::MapStrPmt(mut item) => {
                    let min = item
                        .remove("min")
                        .as_ref()
                        .and_then(pmt_number)
                        .ok_or_else(|| "numeric range has no minimum".to_string())?;
                    let max = item
                        .remove("max")
                        .as_ref()
                        .and_then(pmt_number)
                        .ok_or_else(|| "numeric range has no maximum".to_string())?;
                    match item.remove("step") {
                        Some(step) => pmt_number(&step)
                            .map(|step| NumericRangeItem::Step(min, max, step))
                            .ok_or_else(|| "numeric range has an invalid step".to_string()),
                        None => Ok(NumericRangeItem::Interval(min, max)),
                    }
                }
                _ => Err("source returned an invalid numeric range item".to_string()),
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { items })
    }
}

fn string_values(value: Pmt) -> Result<Vec<String>, String> {
    let Pmt::VecPmt(values) = value else {
        return Err("source returned invalid text values".to_string());
    };
    values
        .into_iter()
        .map(|value| match value {
            Pmt::String(value) => Ok(value),
            _ => Err("source returned an invalid text value".to_string()),
        })
        .collect()
}

fn optional_string(value: Option<Pmt>) -> Result<Option<String>, String> {
    value
        .map(|value| match value {
            Pmt::String(value) => Ok(value),
            _ => Err("source returned an invalid text setting".to_string()),
        })
        .transpose()
}

fn optional_number(value: Option<Pmt>) -> Result<Option<f64>, String> {
    value
        .map(|value| {
            pmt_number(&value)
                .ok_or_else(|| "source returned an invalid numeric setting".to_string())
        })
        .transpose()
}

fn pmt_number(value: &Pmt) -> Option<f64> {
    match value {
        Pmt::F32(value) => Some(*value as f64),
        Pmt::F64(value) => Some(*value),
        Pmt::U32(value) => Some(*value as f64),
        Pmt::U64(value) => Some(*value as f64),
        Pmt::Usize(value) => Some(*value as f64),
        _ => None,
    }
}

fn nearly_equal(left: f64, right: f64) -> bool {
    (left - right).abs() <= f64::EPSILON * left.abs().max(right.abs()).max(1.0) * 8.0
}

fn push_unique(values: &mut Vec<f64>, value: f64) {
    if !values.iter().any(|allowed| nearly_equal(*allowed, value)) {
        values.push(value);
    }
}

fn format_number(value: f64) -> String {
    if nearly_equal(value, value.round()) {
        format!("{value:.0}")
    } else {
        format!("{value:.6}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ten_discrete_values_use_select() {
        let range = NumericRange {
            items: vec![NumericRangeItem::Step(0.0, 9.0, 1.0)],
        };

        assert_eq!(range.discrete_values(MAX_SELECT_VALUES).unwrap().len(), 10);
    }

    #[test]
    fn eleven_discrete_values_use_text_input() {
        let range = NumericRange {
            items: vec![NumericRangeItem::Step(0.0, 10.0, 1.0)],
        };

        assert!(range.discrete_values(MAX_SELECT_VALUES).is_none());
    }

    #[test]
    fn continuous_range_uses_text_input() {
        let range = NumericRange {
            items: vec![NumericRangeItem::Interval(1.0, 10.0)],
        };

        assert!(range.discrete_values(MAX_SELECT_VALUES).is_none());
    }
}
