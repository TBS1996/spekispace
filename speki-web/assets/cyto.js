import cytoscape from "https://esm.sh/cytoscape@3.23.0";
import dagre from "https://esm.sh/cytoscape-dagre@2.4.0";
import { onNodeClick, onEdgeClick } from '/wasm/speki-web.js';
import { maxCircumference } from "/assets/textcalc.js"; 
cytoscape.use(dagre);

const instances = new Map();
const lineLen = 20;


export function createCytoInstance(id) {
    if (instances.has(id)) {
        console.log(`Cytoscape instance with ID "${id}" already exists. Destroying it...`);
        const cy = instances.get(id);
        cy.destroy(); 
        instances.delete(id); 
    }

    let container = document.getElementById(id);

    console.log(`Creating new Cytoscape instance for container id ${id}, w container: ${container}`);

    const cy = cytoscape({
        container, 
        elements: [], 
        minZoom: 0.5,
        maxZoom: 4,
        style: [
            {
                selector: "node",
                style: {
                    "background-color": "data(backgroundColor)",
                    "border-color": "#000",
                    "border-width": 1,
                    "shape": "data(shape)",
                    "label": "data(label)",
                    "color": "#000",
                    "text-wrap": "wrap",
                    "text-valign": "center",
                    "text-halign": "center",
                    "width": (ele) => maxCircumference(ele),
                    "height": (ele) => maxCircumference(ele),
                    'font-family': 'Arial',
                    'font-size': '8px',
                    'font-weight': 'normal',
                },
            },
            {
                selector: "edge",
                style: {
                    "line-color": "#000",
                    "width": 1,
                    "target-arrow-color": "#ccc",
                    "target-arrow-shape": "triangle",
                    "arrow-scale": 0.5,
                    "target-distance-from-node": 3,
                    "curve-style": "bezier",
                },
            },
        ],
    });

    console.log(`adding on tap`);

    cy.on("tap", "node", (event) => {
        const node = event.target;
        const nodeId = node.id();
        console.log(`Node clicked: ${nodeId}`);
        onNodeClick(id, nodeId);
    });



    cy.on("tap", "edge", (event) => {
        const edge = event.target;
        const edgeId = edge.id();
        const sourceNodeId = edge.source().id();
        const targetNodeId = edge.target().id();
    
        console.log(`Edge clicked: ${edgeId}`);
        console.log(`Source Node: ${sourceNodeId}`);
        console.log(`Target Node: ${targetNodeId}`);
        console.log("bruhhhuhu");
    
        onEdgeClick(id, sourceNodeId, targetNodeId);
    });


    console.log(`setting instance`);

    instances.set(id, cy);
    return cy;
}


export function updateShape(cy_id, node_id, shape) {
    const cy = getCytoInstance(cy_id); 
    const node = cy.getElementById(node_id);
    node.data("shape", shape); 
}

export function updateLabel(cy_id, node_id, label) {
    const cy = getCytoInstance(cy_id); 
    const node = cy.getElementById(node_id);
    let thelabel = wrapText(label);
    node.data("label", thelabel); 
    resizeNodeToFitLabel(node);
}

export function setContainer(cy_id) {
    const cy = getCytoInstance(cy_id); 
    const container = document.getElementById(cy_id); 

    if (container) {
        console.log(`Setting container for Cytoscape instance with ID "${cy_id}"`);
        cy.mount(container); 
    } else {
        console.error(`Container with ID "${cy_id}" not found.`);
    }
}

export function zoomToNode(cy_id, node_id) {
    const cy = getCytoInstance(cy_id);
    const node = cy.getElementById(node_id);

    cy.center(node);
    cy.zoom({
        level: 3, 
        position: { x: node.position('x'), y: node.position('y') },
    });
}

export function getCytoInstance(id) {
    return instances.get(id);
}

export function runLayout(id, targetNodeId) {
    const cy = getCytoInstance(id);
    if (cy) {
        cy.layout({
            name: "dagre",
            rankDir: "BT",          
            fit: true,              
            padding: 50,            
            nodeSep: 50,            
            rankSep: 25,            
            edgeWeight: (edge) => {
                const connectedToTarget = edge.source().id() === targetNodeId || edge.target().id() === targetNodeId;
                return connectedToTarget ? 3 : 1; 
            },
        }).run();

        adjustProximityToTargetWithDirection(cy, targetNodeId);
        cy.reset();
        cy.fit();
    } else {
        console.warn(`Cytoscape instance with ID "${id}" not found.`);
    }
}


function adjustProximityToTargetWithDirection(cy, targetNodeId) {
    const targetNode = cy.getElementById(targetNodeId);
    const targetPos = targetNode.position();
    const incomingNeighbors = targetNode.incomers("node"); 
    const outgoingNeighbors = targetNode.outgoers("node"); 

    const node = cy.getElementById(targetNodeId);
    let origin_size = maxCircumference(node) / 2;

    // Outgoing nodes (dependencies) are placed above
    outgoingNeighbors.forEach((node, index) => {
        let node_size = maxCircumference(node);
        let node_pad = node_size + origin_size;
        node.position({
            x: targetPos.x + index * node_pad - (outgoingNeighbors.length * node_pad) / 2,
            y: targetPos.y - node_pad, 
        });
    });

    // Incoming nodes (dependents) are placed below
    incomingNeighbors.forEach((node, index) => {
        let node_size = maxCircumference(node);
        let node_pad = node_size + origin_size;
        node.position({
            x: targetPos.x + index * node_pad - (incomingNeighbors.length * node_pad) / 2,
            y: targetPos.y + node_pad,
        });
    });

    cy.fit(); 
}




export function addEdge(id, source, target) {
    const cy = getCytoInstance(id);
    if (cy) {
        cy.add({ data: { source, target } });
    }
}

export function addNode(cyto_id, id, label, backgroundColor, shape) {
    const cy = getCytoInstance(cyto_id);
    if (cy) {
        const wrappedLabel = wrapText(label); 
        const node = cy.add({ 
            data: { 
                id, 
                label: wrappedLabel, 
                backgroundColor, 
                shape 
            } 
        });

        resizeNodeToFitLabel(node);
    }
}

function wrapText(text) {
    const words = text.split(" ");
    let lines = [];
    let currentLine = "";

    words.forEach((word) => {
        if ((currentLine + word).length > lineLen) {
            lines.push(currentLine.trim());
            currentLine = word + " ";
        } else {
            currentLine += word + " ";
        }
    });

    if (currentLine.trim()) {
        lines.push(currentLine.trim());
    }

    return lines.join("\n");
}

function resizeNodeToFitLabel(node) {
    let circum = maxCircumference(node);

    node.style({
        "width": (ele) => circum,
        "height": (ele) => circum,
    });
}
