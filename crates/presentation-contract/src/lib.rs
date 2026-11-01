//! One acceptance and authoring compiler, executed natively by Rust and as
//! WebAssembly by the renderer. No queries, filesystem, network or model calls.
mod schema;
use serde_json::{json, Map, Value as V};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
type R<T> = Result<T, String>;
fn manifest() -> &'static V {
    static VALUE: OnceLock<V> = OnceLock::new();
    VALUE.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../backend/resources/presentation-contract.json"
        ))
        .expect("generated presentation contract")
    })
}
fn err(path: &str, message: &str) -> String {
    format!("{path}: {message}")
}
fn object<'a>(value: &'a V, path: &str) -> R<&'a Map<String, V>> {
    value
        .as_object()
        .ok_or_else(|| err(path, "expected an object"))
}
fn array<'a>(value: &'a V, path: &str) -> R<&'a Vec<V>> {
    value
        .as_array()
        .ok_or_else(|| err(path, "expected an array"))
}
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value.bytes().enumerate().all(|(i, b)| {
            if i == 0 {
                b.is_ascii_alphabetic()
            } else {
                b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
            }
        })
}
fn state_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value.bytes().enumerate().all(|(i, b)| {
            if i == 0 {
                b.is_ascii_alphabetic()
            } else {
                b.is_ascii_alphanumeric() || b"_.-".contains(&b)
            }
        })
}
fn inspect(value: &V, path: &str, depth: usize, nodes: &mut usize) -> R<()> {
    *nodes += 1;
    if *nodes > 20_000 || depth > 128 {
        return Err(err(path, "JSON complexity budget exceeded"));
    }
    match value {
        V::Object(items) => {
            for (key, value) in items {
                if ["__proto__", "constructor", "prototype"].contains(&key.as_str()) {
                    return Err(err(path, "unsafe property"));
                }
                inspect(value, &format!("{path}.{key}"), depth + 1, nodes)?;
            }
        }
        V::Array(items) => {
            if items.len() > 1920 {
                return Err(err(path, "array budget exceeded"));
            }
            for (i, value) in items.iter().enumerate() {
                inspect(value, &format!("{path}[{i}]"), depth + 1, nodes)?;
            }
        }
        V::String(text) if text.len() > 64_000 => return Err(err(path, "text budget exceeded")),
        _ => {}
    }
    Ok(())
}

/// Compile authored nodes; already-saved flat json-render specs retain identity.
pub fn compile(input: &V) -> R<V> {
    if serde_json::to_vec(input)
        .map_err(|error| error.to_string())?
        .len()
        > 256_000
    {
        return Err(err("spec", "serialized size exceeds 256000 bytes"));
    }
    if input.get("version").and_then(V::as_str) != Some("fella-ui-1") {
        return Ok(input.clone());
    }
    inspect(input, "author", 0, &mut 0)?;
    schema::check(
        input,
        &manifest()["authorSchema"],
        &manifest()["authorSchema"],
        "author",
        0,
    )?;
    let states = input.get("state").cloned().unwrap_or(json!({}));
    let mut elements = Map::new();
    fn node(
        value: &V,
        path: &str,
        states: &V,
        elements: &mut Map<String, V>,
        depth: usize,
        parent_tab: Option<(&str, usize)>,
    ) -> R<String> {
        if depth > 24 || elements.len() >= 240 {
            return Err(err(path, "component/depth budget exceeded"));
        }
        let kind = value["type"]
            .as_str()
            .ok_or_else(|| err(path, "missing component type"))?;
        let id = value
            .get("id")
            .and_then(V::as_str)
            .unwrap_or(path)
            .to_owned();
        if !identifier(&id) || elements.contains_key(&id) {
            return Err(err(path, "invalid or duplicate element identity"));
        }
        let mut props = value.get("props").cloned().unwrap_or(json!({}));
        if let Some(key) = value.pointer("/bind/state").and_then(V::as_str) {
            let state = states
                .get(key)
                .ok_or_else(|| err(path, "binding references an undeclared state"))?;
            props["stateKey"] = json!(key);
            match kind {
                "Select" | "SegmentedControl" => {
                    if state["type"] != "enum" {
                        return Err(err(path, "choice controls require enum state"));
                    }
                    props["options"] = state["options"].clone();
                    props["defaultValue"] = state["default"].clone();
                }
                "Checkbox" => {
                    if state["type"] != "boolean" {
                        return Err(err(path, "checkbox requires boolean state"));
                    }
                    props["defaultChecked"] = state["default"].clone();
                }
                _ => return Err(err(path, "this component cannot produce state")),
            }
        }
        if ["Select", "SegmentedControl", "Checkbox"].contains(&kind) && value.get("bind").is_none()
        {
            return Err(err(path, "control needs bind.state"));
        }
        if let Some(filters) = value.get("filters") {
            props["filters"] = V::Array(
                array(filters, path)?
                    .iter()
                    .map(|binding| json!({"stateKey":binding["state"],"field":binding["field"]}))
                    .collect(),
            );
        }
        if let Some((name, index)) = parent_tab {
            props.as_object_mut().unwrap().remove("label");
            if kind != "TabPanel" {
                return Err(err(path, "Tabs children must be TabPanel"));
            }
            props["name"] = json!(name);
            props["index"] = json!(index);
        }
        if kind == "Tabs" {
            if props.get("name").is_none() {
                props["name"] = json!(id);
            }
            props["labels"] = V::Array(
                value
                    .get("children")
                    .and_then(V::as_array)
                    .ok_or_else(|| err(path, "Tabs need panels"))?
                    .iter()
                    .map(|child| child.pointer("/props/label").cloned().unwrap_or(V::Null))
                    .collect(),
            );
        }
        elements.insert(id.clone(), json!({"type":kind,"props":props,"children":[]}));
        let mut children = Vec::new();
        if let Some(items) = value.get("children").and_then(V::as_array) {
            for (index, child) in items.iter().enumerate() {
                let name = props.get("name").and_then(V::as_str).unwrap_or(&id);
                children.push(json!(node(
                    child,
                    &format!("{path}_{index}"),
                    states,
                    elements,
                    depth + 1,
                    if kind == "Tabs" {
                        Some((name, index))
                    } else {
                        None
                    }
                )?));
            }
        }
        elements[&id]["children"] = json!(children);
        if let Some(when) = value.get("when") {
            elements[&id]["visible"] = json!({"$state":format!("/control:{}",when["state"].as_str().unwrap_or("")),"eq":when["equals"]});
        }
        Ok(id)
    }
    let root = node(&input["root"], "doc", &states, &mut elements, 0, None)?;
    let mut metadata = json!({"version":"fella-ui-1","capabilities":input.get("capabilities").cloned().unwrap_or(json!(["display"])),"state":states});
    if let Some(intent) = input.get("intent") {
        metadata["intent"] = intent.clone();
    }
    elements[&root]["props"]["contract"] = metadata;
    Ok(json!({"root":root,"elements":elements}))
}

pub fn structure(input: &V) -> R<V> {
    let spec = compile(input)?;
    inspect(&spec, "spec", 0, &mut 0)?;
    if serde_json::to_vec(&spec).map_err(|e| e.to_string())?.len() > 256_000 {
        return Err(err("spec", "serialized size exceeds 256000 bytes"));
    }
    let outer = object(&spec, "spec")?;
    if outer.len() != 2 || !outer.contains_key("root") || !outer.contains_key("elements") {
        return Err(err("spec", "only root and elements are allowed"));
    }
    let root = spec["root"]
        .as_str()
        .ok_or_else(|| err("root", "missing root identity"))?;
    let elements = object(&spec["elements"], "elements")?;
    if elements.is_empty()
        || elements.len() > 240
        || elements.get(root).and_then(|e| e["type"].as_str()) != Some("Document")
    {
        return Err(err(
            "root",
            "root must be Document; component budget is 240",
        ));
    }
    let mut parents = HashMap::new();
    for (key, element) in elements {
        let path = format!("elements.{key}");
        if !identifier(key) {
            return Err(err(&path, "invalid identity"));
        }
        let record = object(element, &path)?;
        if record
            .keys()
            .any(|key| !["type", "props", "children", "visible"].contains(&key.as_str()))
        {
            return Err(err(
                &path,
                "only type, props, children and typed visibility are supported",
            ));
        }
        let kind = element["type"]
            .as_str()
            .ok_or_else(|| err(&path, "missing component type"))?;
        if kind == "Document" && key != root {
            return Err(err(&path, "Document is only allowed at the root"));
        }
        let prop_schema = &manifest()["componentSchemas"][kind];
        if !prop_schema.is_object() {
            return Err(err(&path, "unsupported component"));
        }
        schema::check(
            &element["props"],
            prop_schema,
            prop_schema,
            &format!("{path}.props"),
            0,
        )?;
        let children = array(&element["children"], &path)?;
        if manifest()["slots"][kind]
            .as_array()
            .is_some_and(|slots| slots.is_empty())
            && !children.is_empty()
        {
            return Err(err(&path, "leaf components cannot contain children"));
        }
        for child in children {
            let child = child
                .as_str()
                .filter(|id| elements.contains_key(*id))
                .ok_or_else(|| err(&path, "missing child reference"))?;
            if parents.insert(child, key).is_some() {
                return Err(err(&path, "child has multiple parents"));
            }
        }
        if kind == "Chart" {
            chart_bounds(&element["props"]["spec"], None)?;
        }
    }
    if parents.contains_key(root) {
        return Err(err("root", "root cannot have a parent"));
    }
    let mut visited = HashSet::new();
    let mut pending = vec![(root, 0)];
    while let Some((key, depth)) = pending.pop() {
        if depth > 24 || !visited.insert(key) {
            return Err(err("elements", "cycle or depth budget exceeded"));
        }
        for child in array(&elements[key]["children"], key)? {
            pending.push((child.as_str().unwrap(), depth + 1));
        }
    }
    if visited.len() != elements.len() {
        return Err(err("elements", "every element must be reachable"));
    }
    interactions(&spec, None)?;
    Ok(spec)
}

fn accepted_state(def: &V, value: &V) -> bool {
    match def["type"].as_str() {
        Some("boolean") => value.is_boolean(),
        Some("enum") => def["options"]
            .as_array()
            .is_some_and(|options| options.iter().any(|option| option["value"] == *value)),
        _ => false,
    }
}
fn interactions(spec: &V, results: Option<&[V]>) -> R<()> {
    let elements = spec["elements"].as_object().unwrap();
    let root = spec["root"].as_str().unwrap();
    let contract = elements[root]["props"].get("contract");
    let states = contract.and_then(|c| c["state"].as_object());
    let strict = contract.is_some();
    if let Some(contract) = contract {
        let caps = contract["capabilities"]
            .as_array()
            .ok_or_else(|| err("capabilities", "expected declared capabilities"))?;
        if !caps.contains(&json!("display"))
            || caps.iter().collect::<HashSet<_>>().len() != caps.len()
        {
            return Err(err(
                "capabilities",
                "display is required and capabilities must be unique",
            ));
        }
    }
    let mut producers = HashSet::new();
    let mut consumers = HashSet::new();
    let mut controls = HashSet::new();
    if let Some(states) = states {
        for (key, def) in states {
            if !state_id(key) || !accepted_state(def, &def["default"]) {
                return Err(err(
                    &format!("state.{key}"),
                    "invalid state identity or default",
                ));
            }
            if let Some(options) = def["options"].as_array() {
                let mut seen = HashSet::new();
                if options
                    .iter()
                    .any(|option| !seen.insert(option["value"].to_string()))
                {
                    return Err(err(key, "duplicate state option"));
                }
            }
        }
    }
    for (key, element) in elements {
        let kind = element["type"].as_str().unwrap();
        let props = &element["props"];
        let path = format!("elements.{key}");
        if ["Select", "SegmentedControl", "Checkbox"].contains(&kind) {
            let state = props["stateKey"]
                .as_str()
                .ok_or_else(|| err(&path, "missing stateKey"))?;
            if !state_id(state) {
                return Err(err(&path, "invalid stateKey"));
            }
            producers.insert(state.to_owned());
            if let Some(states) = states {
                let def = states
                    .get(state)
                    .ok_or_else(|| err(&path, "control refers to undeclared state"))?;
                if kind == "Checkbox" {
                    if def["type"] != "boolean" || props.get("defaultChecked") != def.get("default")
                    {
                        return Err(err(
                            &path,
                            "checkbox/default must match its state declaration",
                        ));
                    }
                } else if def["type"] != "enum"
                    || props.get("options") != def.get("options")
                    || props.get("defaultValue") != def.get("default")
                {
                    return Err(err(
                        &path,
                        "options/default must match the state declaration",
                    ));
                }
            }
        }
        if [
            "Select",
            "SegmentedControl",
            "Checkbox",
            "Tabs",
            "ResultFilter",
        ]
        .contains(&kind)
        {
            let name = props
                .get("name")
                .or_else(|| props.get("stateKey"))
                .or_else(|| props.get("label"))
                .and_then(V::as_str)
                .unwrap_or("");
            if name.is_empty() || !controls.insert(name) {
                return Err(err(&path, "control names must be unique"));
            }
        }
        if let Some(visible) = element.get("visible") {
            if !visible.is_boolean() {
                let obj = object(visible, &path)?;
                if obj.keys().any(|k| !["$state", "eq"].contains(&k.as_str())) {
                    return Err(err(
                        &path,
                        "visibility supports only declared-state equality",
                    ));
                }
                let state = visible["$state"]
                    .as_str()
                    .and_then(|p| p.strip_prefix("/control:"))
                    .ok_or_else(|| err(&path, "visibility needs a control state path"))?;
                consumers.insert(state.to_owned());
                if let Some(states) = states {
                    let def = states
                        .get(state)
                        .ok_or_else(|| err(&path, "visibility refers to undeclared state"))?;
                    if !accepted_state(def, &visible["eq"]) {
                        return Err(err(
                            &path,
                            "visibility equality has the wrong type or value",
                        ));
                    }
                }
            }
        }
        if kind == "Button"
            && props.pointer("/action/type").and_then(V::as_str) == Some("set_state")
        {
            let state = props["action"]["stateKey"].as_str().unwrap_or("");
            producers.insert(state.to_owned());
            if let Some(states) = states {
                if !states
                    .get(state)
                    .is_some_and(|def| accepted_state(def, &props["action"]["value"]))
                {
                    return Err(err(
                        &path,
                        "action sets an undeclared or impermissible state value",
                    ));
                }
            }
        }
        if strict && kind == "Button" && props.get("action").is_none() {
            return Err(err(
                &path,
                "a button requires an explicit display or follow-up action",
            ));
        }
        if strict
            && (kind == "FollowUp"
                || props.pointer("/action/type").and_then(V::as_str) == Some("follow_up"))
            && !contract.unwrap()["capabilities"]
                .as_array()
                .is_some_and(|caps| caps.contains(&json!("follow_up")))
        {
            return Err(err(&path, "follow_up capability was not declared"));
        }
        if let Some(filters) = props.get("filters").and_then(V::as_array) {
            for binding in filters {
                let state = binding["stateKey"].as_str().unwrap_or("");
                consumers.insert(state.to_owned());
                let def = states
                    .and_then(|states| states.get(state))
                    .ok_or_else(|| err(&path, "filter requires declared enum state"))?;
                if def["type"] != "enum" {
                    return Err(err(&path, "saved-row filters require enum state"));
                }
                if let Some(results) = results {
                    let result = find_result(results, props["resultId"].as_str().unwrap_or(""))?;
                    let column = column(result, binding["field"].as_str().unwrap_or(""))?;
                    for option in def["options"].as_array().unwrap() {
                        let text = option["value"].as_str().unwrap();
                        if text != "$all"
                            && !result["exact_rows"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .any(|row| row[column] == json!(text))
                        {
                            return Err(err(&path,"filter option is absent from the saved field; use $all for an unfiltered view"));
                        }
                    }
                }
            }
        }
        let parent = elements
            .iter()
            .find(|(_, e)| e["children"].as_array().unwrap().contains(&json!(key)))
            .map(|(_, p)| p);
        if kind == "TabPanel" && parent.is_none_or(|p| p["type"] != "Tabs") {
            return Err(err(&path, "TabPanel must belong to Tabs"));
        }
        if kind == "Tabs" {
            let children = element["children"].as_array().unwrap();
            if props["labels"].as_array().unwrap().len() != children.len() {
                return Err(err(&path, "tab labels and panel count differ"));
            }
            for (i, child) in children.iter().enumerate() {
                let panel = &elements[child.as_str().unwrap()];
                if panel["type"] != "TabPanel"
                    || panel["props"]["index"] != json!(i)
                    || panel["props"]["name"] != props["name"]
                {
                    return Err(err(&path, "panel name/index must match its tab set"));
                }
            }
        }
        if strict
            && kind == "ResultFilter"
            && !elements.values().any(|target| {
                ["Chart", "Table"].contains(&target["type"].as_str().unwrap_or(""))
                    && target["props"]["resultId"] == props["resultId"]
            })
        {
            return Err(err(
                &path,
                "result filter needs a compatible chart or table",
            ));
        }
    }
    if let Some(states) = states {
        for key in states.keys() {
            if !producers.contains(key) || !consumers.contains(key) {
                return Err(err(
                    &format!("state.{key}"),
                    "every declared state needs a control/action and a visible or filter consumer",
                ));
            }
        }
    }
    if strict {
        // A reference on a permanently hidden branch, or on the producing
        // control itself, is not an interaction. Prove a possible visible path
        // by intersecting each ancestor's bounded equality constraints.
        for state in states.unwrap().keys() {
            let mut real_consumer = false;
            for (key, element) in elements {
                let self_control = element["props"]["stateKey"] == state.as_str();
                let uses_visibility = element.get("visible").and_then(|v| v["$state"].as_str())
                    == Some(format!("/control:{state}").as_str());
                let uses_filter = element["props"]["filters"]
                    .as_array()
                    .is_some_and(|filters| filters.iter().any(|f| f["stateKey"] == state.as_str()));
                if self_control || !(uses_visibility || uses_filter) {
                    continue;
                }
                let mut possible = true;
                let mut constraints = HashMap::<String, V>::new();
                let mut current = Some(key.as_str());
                while let Some(id) = current {
                    if let Some(visible) = elements[id].get("visible") {
                        if visible == false {
                            possible = false;
                            break;
                        }
                        if let Some(path) = visible["$state"].as_str() {
                            if let Some(prior) =
                                constraints.insert(path.to_owned(), visible["eq"].clone())
                            {
                                if prior != visible["eq"] {
                                    possible = false;
                                    break;
                                }
                            }
                        }
                    }
                    current = elements
                        .iter()
                        .find(|(_, p)| p["children"].as_array().unwrap().contains(&json!(id)))
                        .map(|(id, _)| id.as_str());
                }
                real_consumer |= possible;
            }
            if !real_consumer {
                return Err(err(
                    &format!("state.{state}"),
                    "state requires an effective content consumer outside its control",
                ));
            }
        }
    }
    if !strict {
        for consumer in consumers {
            if !producers.contains(&consumer) {
                return Err(err("visible", "state consumer has no control"));
            }
        }
    }
    Ok(())
}
fn find_result<'a>(results: &'a [V], id: &str) -> R<&'a V> {
    results
        .iter()
        .find(|r| r["id"] == id)
        .ok_or_else(|| err("resultId", &format!("saved result {id:?} is unavailable")))
}
fn column(result: &V, field: &str) -> R<usize> {
    result["columns"]
        .as_array()
        .unwrap()
        .iter()
        .position(|c| c == field)
        .ok_or_else(|| {
            err(
                "field",
                &format!("{field:?} is not present in result {}", result["id"]),
            )
        })
}
fn chart_bounds(spec: &V, result: Option<&V>) -> R<()> {
    chart_analysis(spec, result).map(|_| ())
}

// The generated grammar owns allowed properties, marks, channels and static
// settings. These checks add result-dependent semantics and cumulative budgets.
fn chart_limit(name: &str, fallback: usize) -> usize {
    manifest()["limits"][name]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(fallback)
}
type ChartScope = HashMap<String, Vec<String>>;

#[derive(Default)]
struct ChartBudget {
    views: usize,
    pixels: f64,
    work: usize,
    raw_fields: HashSet<String>,
}

fn chart_static_bounds(
    value: &V,
    path: &str,
    depth: usize,
    temporal: bool,
    nodes: &mut usize,
) -> R<()> {
    *nodes += 1;
    if *nodes > chart_limit("chartNodes", 6000) || depth > chart_limit("depth", 24) {
        return Err(err(path, "chart complexity budget exceeded"));
    }
    match value {
        V::Object(settings) => {
            let temporal = value
                .get("type")
                .and_then(V::as_str)
                .map_or(temporal, |t| t == "temporal");
            if let Some(format) = settings.get("format") {
                if let Some(pattern) = format.as_str() {
                    let time = value["formatType"] == "time"
                        || (temporal && value["formatType"] != "number");
                    let mut digits = 0;
                    for byte in pattern.bytes() {
                        digits = if byte.is_ascii_digit() { digits + 1 } else { 0 };
                        if !time && digits > chart_limit("numericFormatDigits", 4) {
                            return Err(err(
                                &format!("{path}.format"),
                                "numeric format exceeds the label allocation budget",
                            ));
                        }
                    }
                } else if format.as_object().is_some_and(|f| f.is_empty())
                    || value["formatType"] == "number"
                {
                    return Err(err(
                        &format!("{path}.format"),
                        "dynamic time format must be nonempty and cannot use numeric formatType",
                    ));
                }
            }
            for (key, child) in settings {
                // Inert pinned metadata is already checked by the grammar.
                if key != "$schema" {
                    chart_static_bounds(
                        child,
                        &format!("{path}.{key}"),
                        depth + 1,
                        temporal,
                        nodes,
                    )?;
                }
            }
        }
        V::Array(items) => {
            if items.len() > 256 {
                return Err(err(path, "chart array budget exceeded"));
            }
            for (index, child) in items.iter().enumerate() {
                chart_static_bounds(
                    child,
                    &format!("{path}[{index}]"),
                    depth + 1,
                    temporal,
                    nodes,
                )?;
            }
        }
        V::Number(n) => {
            if n.as_f64().is_none_or(|n| !n.is_finite() || n.abs() > 1e15) {
                return Err(err(path, "chart numeric budget exceeded"));
            }
        }
        V::String(text) => {
            static RESOURCES: OnceLock<regex::Regex> = OnceLock::new();
            let resources=RESOURCES.get_or_init(||regex::Regex::new(r"(?i)(?:url\s*\(|(?:https?|file|javascript|data|vbscript):|</?(?:script|svg|iframe)\b)").unwrap());
            if text.chars().count() > 4000 || resources.is_match(text) {
                return Err(err(
                    path,
                    "chart text contains an external resource or exceeds its budget",
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

fn chart_field_names(value: &V, scope: &ChartScope, path: &str) -> R<Vec<String>> {
    if let Some(name) = value.as_str() {
        return Ok(vec![name.to_owned()]);
    }
    let group = value
        .get("repeat")
        .and_then(V::as_str)
        .ok_or_else(|| err(path, "expected a saved field or repeat reference"))?;
    scope.get(group).cloned().ok_or_else(|| {
        err(
            path,
            &format!("repeat {group:?} is outside its declared scope"),
        )
    })
}

fn chart_saved_fields(value: &V, result: Option<&V>, scope: &ChartScope, path: &str) -> R<()> {
    if let Some(settings) = value.as_object() {
        if let Some(field) = settings.get("field") {
            for name in chart_field_names(field, scope, &format!("{path}.field"))? {
                if let Some(result) = result {
                    column(result, &name)
                        .map_err(|message| err(&format!("{path}.field"), &message))?;
                }
            }
        }
        for (key, child) in settings {
            chart_saved_fields(child, result, scope, &format!("{path}.{key}"))?;
        }
    } else if let Some(items) = value.as_array() {
        for (index, child) in items.iter().enumerate() {
            chart_saved_fields(child, result, scope, &format!("{path}[{index}]"))?;
        }
    }
    Ok(())
}

fn chart_raw_fields(value: &V, scope: &ChartScope, raw: &mut HashSet<String>, path: &str) -> R<()> {
    if let Some(items) = value.as_array() {
        for (index, item) in items.iter().enumerate() {
            chart_raw_fields(item, scope, raw, &format!("{path}[{index}]"))?;
        }
    } else if let Some(settings) = value.as_object() {
        if let Some(field) = settings.get("field") {
            if !["aggregate", "bin", "timeUnit"]
                .iter()
                .any(|key| settings.contains_key(*key))
            {
                raw.extend(chart_field_names(field, scope, &format!("{path}.field"))?);
            }
        }
        // Do not collect sort/scale references: they do not display raw values.
    }
    Ok(())
}

fn chart_facet_factor(fields: &[&V], result: Option<&V>, path: &str) -> R<usize> {
    let Some(result) = result else { return Ok(1) };
    let rows = array(&result["rows"], path)?;
    let mut panels = 1usize;
    for field in fields {
        let name = field["field"]
            .as_str()
            .ok_or_else(|| err(path, "facet requires a saved field"))?;
        let index = column(result, name).map_err(|message| err(path, &message))?;
        // Raw cardinality is a conservative upper bound for binned/time facets.
        // Include null and preserve scalar types rather than compare label text.
        let choices = rows
            .iter()
            .map(|row| row[index].to_string())
            .collect::<HashSet<_>>()
            .len()
            .max(1);
        panels = panels
            .checked_mul(choices)
            .ok_or_else(|| err(path, "facet cardinality overflow"))?;
        if panels > chart_limit("facets", 60) {
            return Err(err(path, "facet cardinality exceeds the panel budget"));
        }
    }
    Ok(panels)
}

fn chart_repeat_scope(
    repeat: &V,
    result: Option<&V>,
    scope: &ChartScope,
    path: &str,
) -> R<(ChartScope, usize)> {
    let mut child_scope = scope.clone();
    let mut panels = 1usize;
    let groups: Vec<(&str, &V)> = if repeat.is_array() {
        vec![("repeat", repeat)]
    } else {
        object(repeat, path)?
            .iter()
            .map(|(key, value)| (key.as_str(), value))
            .collect()
    };
    for (group, values) in groups {
        let values = array(values, path)?;
        let mut names = Vec::new();
        for (index, value) in values.iter().enumerate() {
            let name = value
                .as_str()
                .ok_or_else(|| err(path, "repeat requires saved field names"))?;
            if let Some(result) = result {
                column(result, name)
                    .map_err(|message| err(&format!("{path}.{group}[{index}]"), &message))?;
            }
            names.push(name.to_owned());
        }
        panels = panels
            .checked_mul(names.len())
            .ok_or_else(|| err(path, "repeat work overflow"))?;
        if panels > chart_limit("facets", 60) {
            return Err(err(path, "repeated views exceed the panel budget"));
        }
        child_scope.insert(group.to_owned(), names);
    }
    Ok((child_scope, panels))
}

fn chart_view(
    view: &V,
    result: Option<&V>,
    scope: &ChartScope,
    inherited: &Map<String, V>,
    size: (f64, f64),
    instances: usize,
    budget: &mut ChartBudget,
    path: &str,
) -> R<()> {
    budget.views += 1;
    if budget.views > chart_limit("chartLayers", 24) {
        return Err(err(path, "composed view budget exceeded"));
    }
    let size = (
        view.get("width").and_then(V::as_f64).unwrap_or(size.0),
        view.get("height").and_then(V::as_f64).unwrap_or(size.1),
    );
    let mut encoding = inherited.clone();
    if let Some(local) = view["encoding"].as_object() {
        chart_saved_fields(
            &view["encoding"],
            result,
            scope,
            &format!("{path}.encoding"),
        )?;
        for (key, value) in local {
            encoding.insert(key.clone(), value.clone());
        }
    }
    let mut panels = 1usize;
    if let Some(facet) = view.get("facet") {
        chart_saved_fields(facet, result, scope, &format!("{path}.facet"))?;
        let fields: Vec<&V> = if facet.get("field").is_some() {
            vec![facet]
        } else {
            object(facet, path)?.values().collect()
        };
        for field in &fields {
            chart_raw_fields(
                field,
                scope,
                &mut budget.raw_fields,
                &format!("{path}.facet"),
            )?;
        }
        panels = chart_facet_factor(&fields, result, &format!("{path}.facet"))?;
    }
    // Faceting through encoding is equivalent to the explicit facet operator.
    let mut facet_fields: Vec<&V> = ["row", "column"]
        .iter()
        .filter_map(|key| encoding.get(*key))
        .filter(|value| value.is_object())
        .collect();
    if facet_fields.is_empty() {
        if let Some(field) = encoding.get("facet").filter(|v| v.is_object()) {
            facet_fields.push(field);
        }
    }
    panels = panels
        .checked_mul(chart_facet_factor(
            &facet_fields,
            result,
            &format!("{path}.encoding"),
        )?)
        .ok_or_else(|| err(path, "facet work overflow"))?;
    let (child_scope, repeats) = if let Some(repeat) = view.get("repeat") {
        chart_repeat_scope(repeat, result, scope, &format!("{path}.repeat"))?
    } else {
        (scope.clone(), 1)
    };
    let instances = instances
        .checked_mul(panels)
        .and_then(|n| n.checked_mul(repeats))
        .ok_or_else(|| err(path, "chart work overflow"))?;
    budget.pixels += size.0 * size.1 * instances as f64;
    let rows = result
        .and_then(|r| r["rows"].as_array())
        .map_or(0, Vec::len);
    budget.work = rows
        .checked_mul(instances)
        .and_then(|n| budget.work.checked_add(n))
        .ok_or_else(|| err(path, "chart work overflow"))?;
    if !budget.pixels.is_finite() || budget.pixels > chart_limit("chartPixels", 8_000_000) as f64 {
        return Err(err(path, "total chart pixel budget exceeded"));
    }
    if budget.work > chart_limit("chartWork", 500_000) {
        return Err(err(path, "total chart row/view work budget exceeded"));
    }
    if view.get("mark").is_some() {
        for (channel, definition) in &encoding {
            chart_raw_fields(
                definition,
                scope,
                &mut budget.raw_fields,
                &format!("{path}.encoding.{channel}"),
            )?;
        }
    }
    for key in ["layer", "concat", "hconcat", "vconcat"] {
        if let Some(children) = view.get(key).and_then(V::as_array) {
            let empty = Map::new();
            let inherited = if key == "layer" { &encoding } else { &empty };
            for (index, child) in children.iter().enumerate() {
                chart_view(
                    child,
                    result,
                    scope,
                    inherited,
                    size,
                    instances,
                    budget,
                    &format!("{path}.{key}[{index}]"),
                )?;
            }
        }
    }
    if let Some(child) = view.get("spec") {
        chart_view(
            child,
            result,
            &child_scope,
            &Map::new(),
            size,
            instances,
            budget,
            &format!("{path}.spec"),
        )?;
    }
    Ok(())
}

fn chart_analysis(spec: &V, result: Option<&V>) -> R<ChartBudget> {
    chart_static_bounds(spec, "Chart.spec", 0, false, &mut 0)?;
    let mut budget = ChartBudget::default();
    chart_view(
        spec,
        result,
        &ChartScope::new(),
        &Map::new(),
        (300.0, 240.0),
        1,
        &mut budget,
        "Chart.spec",
    )?;
    Ok(budget)
}

/// Acceptance should call this with Chart props after generated-schema checking.
/// It includes chart_bounds plus the result-bound linkBy restriction.
pub fn chart_fields(props: &V, result: &V) -> R<()> {
    let budget = chart_analysis(&props["spec"], Some(result))?;
    if let Some(fields) = props.get("linkBy").and_then(V::as_array) {
        for (index, field) in fields.iter().enumerate() {
            let name = field.as_str().unwrap_or("");
            column(result, name)
                .map_err(|message| err(&format!("Chart.linkBy[{index}]"), &message))?;
            if !budget.raw_fields.contains(name) {
                return Err(err(
                    &format!("Chart.linkBy[{index}]"),
                    "linked chart fields must be encoded as raw, unaggregated saved values",
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod chart_semantic_tests {
    use super::*;

    fn result(rows: usize) -> V {
        json!({"id":"chart-test","columns":["category","other","value"],
            "rows":(0..rows).map(|i|json!([format!("group-{i}"),format!("other-{i}"),i])).collect::<Vec<_>>()})
    }

    // Fixed criteria: all referenced/repeated fields must exist; repeat bindings
    // are lexical and cannot leak across siblings or survive outside their spec.
    #[test]
    fn chart_semantics_repeat_fields_and_scope() {
        let saved = result(2);
        let repeated = json!({"repeat":["value"],"spec":{"mark":"point","encoding":{"x":{"field":{"repeat":"repeat"},"type":"quantitative"}}}});
        assert!(chart_bounds(&repeated, Some(&saved)).is_ok());
        let missing = json!({"repeat":["absent"],"spec":{"mark":"point"}});
        assert!(chart_bounds(&missing, Some(&saved))
            .unwrap_err()
            .contains("absent"));
        let unscoped = json!({"mark":"point","encoding":{"x":{"field":{"repeat":"row"}}}});
        assert!(chart_bounds(&unscoped, None).unwrap_err().contains("scope"));
        let sibling = json!({"hconcat":[repeated,unscoped]});
        assert!(chart_bounds(&sibling, Some(&saved))
            .unwrap_err()
            .contains("scope"));
        let shadowed = json!({"repeat":{"row":["value"]},"spec":{"repeat":{"row":["other"]},"spec":{"mark":"point","encoding":{"x":{"field":{"repeat":"row"}}}}}});
        let props = json!({"spec":shadowed,"linkBy":["other"]});
        assert!(chart_fields(&props, &saved).is_ok());
        let outer = json!({"spec":props["spec"],"linkBy":["value"]});
        assert!(chart_fields(&outer, &saved).is_err());
    }

    // Fixed criteria: explicit and encoding facets are capped at 60 Cartesian
    // panels; sibling repeat/facet multipliers do not multiply one another.
    #[test]
    fn chart_semantics_facet_cardinality_and_branch_work() {
        let facet = json!({"facet":{"field":"category"},"spec":{"mark":"point"}});
        assert!(chart_bounds(&facet, Some(&result(61)))
            .unwrap_err()
            .contains("facet"));
        let encoded = json!({"mark":"point","encoding":{"row":{"field":"category"}}});
        assert!(chart_bounds(&encoded, Some(&result(61)))
            .unwrap_err()
            .contains("facet"));
        let product = json!({"facet":{"row":{"field":"category"},"column":{"field":"other"}},"spec":{"mark":"point"}});
        assert!(chart_bounds(&product, Some(&result(8)))
            .unwrap_err()
            .contains("facet"));
        let siblings = json!({"hconcat":[facet.clone(),facet]});
        assert!(chart_bounds(&siblings, Some(&result(10))).is_ok());
    }

    // Fixed criteria: every view counts toward 24; cumulative pixels are capped
    // at 8M and saved-row/view work at 500K, including nested repeated views.
    #[test]
    fn chart_semantics_cumulative_rendering_budgets() {
        let nested = (0..24).fold(
            json!({"mark":"point"}),
            |child, _| json!({"hconcat":[child]}),
        );
        assert!(chart_bounds(&nested, None).is_err());
        let pixels = json!({"layer":[{"mark":"point"},{"mark":"point"},{"mark":"point"}],"width":1600,"height":1600});
        assert!(chart_bounds(&pixels, None).unwrap_err().contains("pixel"));
        let repeats = vec!["value"; 60];
        let work = json!({"repeat":repeats,"spec":{"mark":"point","width":1,"height":1},"width":1,"height":1});
        assert!(chart_bounds(&work, Some(&result(10_000)))
            .unwrap_err()
            .contains("work"));
        let repeat_product = json!({"repeat":{"row":vec!["value";8],"column":vec!["value";8]},"spec":{"mark":"point"}});
        assert!(chart_bounds(&repeat_product, None)
            .unwrap_err()
            .contains("panel"));
        let nested_work = json!({"repeat":vec!["value";10],"spec":{"repeat":vec!["value";10],"spec":{"mark":"point","width":1,"height":1}},"width":1,"height":1});
        assert!(chart_bounds(&nested_work, Some(&result(5_000)))
            .unwrap_err()
            .contains("work"));
    }

    // Fixed criteria: numeric format width/precision cannot contain five-digit
    // allocations; built-in time formatting and null axis/legend remain valid.
    #[test]
    fn chart_semantics_format_allocations_and_resources() {
        let numeric = json!({"mark":"point","encoding":{"x":{"field":"value","axis":{"format":"1000000000.2f"}}}});
        assert!(chart_bounds(&numeric, None)
            .unwrap_err()
            .contains("allocation"));
        let temporal = json!({"mark":"point","encoding":{"x":{"field":"value","type":"temporal","axis":{"format":"%Y 123456"}},"color":{"field":"category","legend":null},"y":{"field":"value","axis":null}}});
        assert!(chart_bounds(&temporal, None).is_ok());
        let dynamic = json!({"mark":"point","encoding":{"x":{"field":"value","format":{"year":"%Y"},"formatType":"number"}}});
        assert!(chart_bounds(&dynamic, None).is_err());
        let empty = json!({"mark":"point","encoding":{"x":{"field":"value","format":{}}}});
        assert!(chart_bounds(&empty, None).is_err());
        let resource = json!({"mark":{"type":"point","font":"URL \t(https://example.com/font)"}});
        assert!(chart_bounds(&resource, None)
            .unwrap_err()
            .contains("resource"));
        let schema =
            json!({"$schema":"https://vega.github.io/schema/vega-lite/v6.json","mark":"point"});
        assert!(chart_bounds(&schema, None).is_ok());
    }

    // Fixed criteria: linkBy must describe a saved field in an effective raw
    // encoding; sort-only, transformed and fully overridden fields do not count.
    #[test]
    fn chart_semantics_linked_fields_and_inheritance() {
        let saved = result(2);
        let inherited =
            json!({"encoding":{"color":{"field":"category"}},"layer":[{"mark":"point"}]});
        assert!(chart_fields(&json!({"spec":inherited,"linkBy":["category"]}), &saved).is_ok());
        let overridden = json!({"encoding":{"color":{"field":"category"}},"layer":[{"mark":"point","encoding":{"color":{"field":"other"}}}]});
        assert!(chart_fields(&json!({"spec":overridden,"linkBy":["category"]}), &saved).is_err());
        let sorted = json!({"mark":"bar","encoding":{"x":{"field":"other","sort":{"field":"category","op":"min"}}}});
        assert!(chart_fields(&json!({"spec":sorted,"linkBy":["category"]}), &saved).is_err());
        for transform in ["aggregate", "bin", "timeUnit"] {
            let mut definition = json!({"field":"value"});
            definition[transform] = match transform {
                "aggregate" => json!("mean"),
                "bin" => json!(true),
                _ => json!("year"),
            };
            let props =
                json!({"spec":{"mark":"point","encoding":{"x":definition}},"linkBy":["value"]});
            assert!(chart_fields(&props, &saved).is_err(), "{transform}");
        }
        let tooltip = json!({"spec":{"mark":"point","encoding":{"tooltip":[{"field":"category"}]}},"linkBy":["category"]});
        assert!(chart_fields(&tooltip, &saved).is_ok());
    }
}
fn results_valid(results: &[V]) -> R<()> {
    if results.len() > 32 {
        return Err(err("results", "result count exceeds 32"));
    }
    let mut ids = HashSet::new();
    let mut cells = 0usize;
    let mut chars = 0usize;
    for result in results {
        let id = result["id"]
            .as_str()
            .ok_or_else(|| err("results", "missing identity"))?;
        if id.is_empty() || !ids.insert(id) {
            return Err(err("results", "duplicate result identity"));
        }
        let columns = array(&result["columns"], id)?;
        let rows = array(&result["rows"], id)?;
        let exact = array(&result["exact_rows"], id)?;
        if columns.is_empty()
            || columns.len() > 100
            || columns
                .iter()
                .any(|c| c.as_str().is_none_or(|s| s.is_empty() || s.len() > 240))
            || columns.iter().collect::<HashSet<_>>().len() != columns.len()
        {
            return Err(err(id, "invalid result columns"));
        }
        let count = result["row_count"]
            .as_u64()
            .ok_or_else(|| err(id, "invalid row count"))?;
        if rows.len() > 10_000
            || exact.len() != rows.len()
            || count < rows.len() as u64
            || !result["complete"].is_boolean()
            || result["complete"] == true && count != rows.len() as u64
        {
            return Err(err(id, "invalid result extent or missing exact rows"));
        }
        for (i, row) in rows.iter().enumerate() {
            let values = array(row, id)?;
            let canonical = array(&exact[i], id)?;
            if values.len() != columns.len() || canonical.len() != columns.len() {
                return Err(err(id, "invalid saved row width"));
            }
            for (j, value) in values.iter().enumerate() {
                if value.is_object() || value.is_array() {
                    return Err(err(id, "saved cells must be scalar"));
                }
                let text = canonical[j]
                    .as_str()
                    .ok_or_else(|| err(id, "exact cells must be strings"))?;
                if text.len() > 16_000 {
                    return Err(err(id, "exact value text budget exceeded"));
                }
                cells += 1;
                chars += text.len() + value.as_str().map_or(0, str::len);
            }
        }
    }
    if cells > 200_000 || chars > 8_000_000 {
        return Err(err("results", "saved result memory budget exceeded"));
    }
    Ok(())
}
pub fn accept(input: &V, results: &[V], charts_enabled: bool) -> R<V> {
    results_valid(results)?;
    let spec = structure(input)?;
    let mut ids = HashSet::new();
    for (key, element) in spec["elements"].as_object().unwrap() {
        let kind = element["type"].as_str().unwrap();
        let props = &element["props"];
        let path = format!("elements.{key}.props");
        if kind == "Chart" && !charts_enabled {
            return Err(err(&path, "charts are disabled by host capabilities"));
        }
        if ["Metric", "Progress", "Chart", "Table", "ResultFilter"].contains(&kind) {
            let result = find_result(results, props["resultId"].as_str().unwrap_or(""))?;
            ids.insert(result["id"].as_str().unwrap());
            if ["Metric", "Progress", "ResultFilter"].contains(&kind) {
                let col = column(result, props["field"].as_str().unwrap_or(""))?;
                if kind != "ResultFilter" {
                    let row = props["row"].as_u64().unwrap_or(u64::MAX) as usize;
                    let rows = result["rows"].as_array().unwrap();
                    if row >= rows.len() {
                        return Err(err(&path, "bound result row is unavailable"));
                    }
                    if kind == "Progress" && !rows[row][col].is_number() {
                        return Err(err(&path, "Progress needs a numeric saved value"));
                    }
                } else {
                    let count = result["rows"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|row| row[col].to_string())
                        .collect::<HashSet<_>>()
                        .len();
                    if count > 200 {
                        return Err(err(&path, "filter exceeds 200 saved choices"));
                    }
                }
            }
            if let Some(fields) = props["fields"].as_array() {
                for field in fields {
                    column(result, field.as_str().unwrap_or(""))?;
                }
            }
            if let Some(formats) = props["columnFormats"].as_object() {
                for field in formats.keys() {
                    column(result, field)?;
                }
            }
            if props.get("linkBy").is_some() && props.get("interactionGroup").is_none() {
                return Err(err(&path, "linkBy requires interactionGroup"));
            }
            if let Some(fields) = props["linkBy"].as_array() {
                for field in fields {
                    column(result, field.as_str().unwrap_or(""))?;
                    if kind == "Table"
                        && props["fields"]
                            .as_array()
                            .is_some_and(|shown| !shown.contains(field))
                    {
                        return Err(err(&path, "linked table fields must be visible"));
                    }
                }
            }
            if kind == "Chart" {
                chart_fields(props, result)?;
            }
        }
    }
    interactions(&spec, Some(results))?;
    Ok(spec)
}
pub fn evaluate(request: &V) -> V {
    let result = if request["mode"] == "structure" {
        structure(&request["spec"])
    } else {
        request["results"]
            .as_array()
            .ok_or_else(|| err("results", "expected saved results"))
            .and_then(|results| {
                accept(
                    &request["spec"],
                    results,
                    request["charts_enabled"].as_bool().unwrap_or(true),
                )
            })
    };
    match result {
        Ok(spec) => {
            let mut ids: Vec<_> = spec["elements"]
                .as_object()
                .unwrap()
                .values()
                .filter_map(|e| e["props"]["resultId"].as_str())
                .collect();
            ids.sort();
            ids.dedup();
            let ids: Vec<_> = ids.into_iter().map(str::to_owned).collect();
            json!({"valid":true,"spec":spec,"issues":[],"resultIds":ids,"contractHash":manifest()["hash"]})
        }
        Err(message) => {
            json!({"valid":false,"spec":null,"issues":[{"path":message.split(':').next().unwrap_or("root"),"message":message}],"resultIds":[],"contractHash":manifest()["hash"]})
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    #[no_mangle]
    pub extern "C" fn allocate(len: usize) -> *mut u8 {
        Box::into_raw(vec![0u8; len].into_boxed_slice()) as *mut u8
    }
    #[no_mangle]
    pub unsafe extern "C" fn release(ptr: *mut u8, len: usize) {
        drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)));
    }
    #[no_mangle]
    pub unsafe extern "C" fn validate(ptr: *const u8, len: usize) -> u64 {
        let result=std::panic::catch_unwind(||serde_json::from_slice::<V>(std::slice::from_raw_parts(ptr,len)).map(|request|evaluate(&request)).unwrap_or_else(|_|json!({"valid":false,"spec":null,"issues":[{"path":"input","message":"invalid validation payload"}],"resultIds":[]}))).unwrap_or_else(|_|json!({"valid":false,"spec":null,"issues":[{"path":"input","message":"validation failed"}],"resultIds":[]}));
        let bytes = serde_json::to_vec(&result).unwrap().into_boxed_slice();
        let len = bytes.len();
        let out = Box::into_raw(bytes) as *mut u8;
        ((len as u64) << 32) | (out as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frozen_authoring_and_rejection_cases() {
        let fixtures: V = serde_json::from_str(include_str!(
            "../../../tests/fixtures/intelligent-ui-v1.json"
        ))
        .unwrap();
        let mut failures = Vec::new();
        for case in fixtures["cases"].as_array().unwrap() {
            let result = evaluate(&json!({"spec":case["input"],"results":fixtures["results"]}));
            if result["valid"] != case["expected"]["valid"] {
                failures.push(format!("{}: {}", case["id"], result));
                continue;
            }
            if result["valid"] == true {
                let again = evaluate(&json!({"spec":result["spec"],"results":fixtures["results"]}));
                assert_eq!(again, result, "canonical reopen: {}", case["id"]);
            } else {
                assert!(result["spec"].is_null());
                assert!(!result["issues"].as_array().unwrap().is_empty());
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
