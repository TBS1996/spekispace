use speki_web::Node;
use tracing::info;

#[cfg(feature = "web")]
use wasm_bindgen::prelude::*;

/// **Execute JavaScript inside WebView**
#[cfg(feature = "desktop")]
fn eval_js(js_code: &str) -> Option<String> {
    let window = use_window();
    window.with_webview(|webview| webview.evaluate_script(js_code).ok().flatten())
}

#[cfg(feature = "web")]
#[wasm_bindgen(module = "/assets/cyto.js")]
extern "C" {
    fn createCytoInstance(id: &str);
    fn addNode(cyto_id: &str, id: &str, label: &str, color: &str, shape: &str, border: bool);
    fn addEdge(cyto_id: &str, source: &str, target: &str);
    fn runLayout(cyto_id: &str, node: &str);
    fn zoomToNode(cyto_id: &str, node_id: &str);
    fn setContainer(cyto_id: &str);
    fn updateLabel(cyto_id: &str, node_id: &str, label: &str);
    fn updateShape(cyto_id: &str, node_id: &str, shape: &str);
}

#[cfg(feature = "desktop")]
use dioxus::desktop::use_window;

/// Update the shape of a node
pub fn _update_shape(cyto_id: &str, node: Node, shape: &str) {
    info!("new shape!: {shape}");
    let node_id = node.id().to_string();

    #[cfg(feature = "web")]
    updateShape(cyto_id, &node_id, shape);

    #[cfg(feature = "desktop")]
    {
        let window = use_window();
        eval_js(&format!(
            "updateShape('{}', '{}', '{}')",
            cyto_id, node_id, shape
        ));
    }
}

/// Update the label of a node
pub fn update_label(cyto_id: &str, node: Node, label: &str) {
    info!("new label!: {label}");
    let node_id = node.id().to_string();

    #[cfg(feature = "web")]
    updateLabel(cyto_id, &node_id, label);

    #[cfg(feature = "desktop")]
    {
        let window = use_window();
        eval_js(&format!(
            "updateLabel('{}', '{}', '{}')",
            cyto_id, node_id, label
        ));
    }
}

/// Zoom to a specific node
pub fn zoom_to_node(cyto_id: &str, node: &str) {
    #[cfg(feature = "web")]
    zoomToNode(cyto_id, node);

    #[cfg(feature = "desktop")]
    {
        let window = use_window();
        eval_js(&format!("zoomToNode('{}', '{}')", cyto_id, node));
    }
}

/// Run the layout algorithm
pub fn run_layout(id: &str, node: &str) {
    #[cfg(feature = "web")]
    runLayout(id, node);

    #[cfg(feature = "desktop")]
    {
        let window = use_window();
        eval_js(&format!("runLayout('{}', '{}')", id, node));
    }
}

/// Create a Cytoscape instance
pub fn create_cyto_instance(id: &str) {
    info!("cyto instance id: {id}");

    #[cfg(feature = "web")]
    createCytoInstance(id);

    #[cfg(feature = "desktop")]
    {
        let window = use_window();
        eval_js(&format!("createCytoInstance('{}')", id));
    }
}

/// Add a new node
pub fn add_node(cyto_id: &str, id: &str, label: &str, color: &str, shape: &str, border: bool) {
    #[cfg(feature = "web")]
    addNode(cyto_id, id, label, color, shape, border);

    #[cfg(feature = "desktop")]
    {
        let window = use_window();
        eval_js(&format!(
            "addNode('{}', '{}', '{}', '{}', '{}', {})",
            cyto_id, id, label, color, shape, border
        ));
    }
}

/// Add an edge between two nodes
pub fn add_edge(cyto_id: &str, source: &str, target: &str) {
    #[cfg(feature = "web")]
    addEdge(cyto_id, source, target);

    #[cfg(feature = "desktop")]
    {
        let window = use_window();
        eval_js(&format!(
            "addEdge('{}', '{}', '{}')",
            cyto_id, source, target
        ));
    }
}
