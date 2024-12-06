import cytoscape from "https://esm.sh/cytoscape@3.23.0";
import dagre from "https://esm.sh/cytoscape-dagre@2.4.0";
import { onNodeClick, onEdgeClick } from '/wasm/speki-web.js';
cytoscape.use(dagre);

const instances = new Map();
const charWidth = 20;
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
                    "shape": "circle",
                    "background-color": "data(backgroundColor)",
                    "border-color": "#000",
                    "border-width": 1,
                    "label": "data(label)",
                    "color": "#000",
                    "text-wrap": "wrap",
                    "text-valign": "center",
                    "text-halign": "center",
                    "width": (ele) => calculateNodeWidth(ele.data("label"), lineLen),
                    "height": (ele) => calculateNodeHeight(ele.data("label"), lineLen),
                    "font-size": "6px",
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
        onNodeClick(nodeId);
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
    
        onEdgeClick(sourceNodeId, targetNodeId);
    });


    console.log(`setting instance`);

    instances.set(id, cy);
    return cy;
}

export function setContainer(cy_id) {
    const cy = getCytoInstance(cy_id); // Retrieve the Cytoscape instance
    const container = document.getElementById(cy_id); // Get the container by ID

    if (container) {
        console.log(`Setting container for Cytoscape instance with ID "${cy_id}"`);
        cy.mount(container); // Mount Cytoscape to the new container
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


function calculateNodeWidth(label, maxCharsPerLine) {
    return Math.min(maxCharsPerLine * charWidth, label.length * charWidth);
}

function calculateNodeHeight(label, maxCharsPerLine) {
    const lines = Math.ceil(label.length / maxCharsPerLine);
    const lineHeight = 20; 
    return lines * lineHeight + 10; 
}

export function getCytoInstance(id) {
    return instances.get(id);
}

export function runLayout(id, targetNodeId) {
    const cy = getCytoInstance(id);
    console.log("8 @@@@@@@@@@@@@@@@@@@@@@@@@@@@");
    if (cy) {
        // Run the Dagre layout
        cy.layout({
            name: "dagre",
            rankDir: "BT",          // Top-to-bottom flow
            fit: true,              // Fit the graph in the viewport
            padding: 50,            // Padding around the graph
            nodeSep: 50,            // Adjust spacing for better visibility
            rankSep: 25,            // Adjust rank separation
            edgeWeight: (edge) => {
                const connectedToTarget = edge.source().id() === targetNodeId || edge.target().id() === targetNodeId;
                return connectedToTarget ? 3 : 1; // Moderate weight for target-connected edges
            },
        }).run();

        // Adjust node proximity with directionality
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
    const incomingNeighbors = targetNode.incomers("node"); // Now dependents
    const outgoingNeighbors = targetNode.outgoers("node"); // Now dependencies

    const horizontalSpacing = 50; // Adjust spacing as needed
    const verticalDistance = 60;  // Adjust vertical distance as needed

    // Outgoing nodes (dependencies) are placed above
    outgoingNeighbors.forEach((node, index) => {
        node.position({
            x: targetPos.x + index * horizontalSpacing - (outgoingNeighbors.length * horizontalSpacing) / 2,
            y: targetPos.y - verticalDistance, 
        });
    });

    // Incoming nodes (dependents) are placed below
    incomingNeighbors.forEach((node, index) => {
        node.position({
            x: targetPos.x + index * horizontalSpacing - (incomingNeighbors.length * horizontalSpacing) / 2,
            y: targetPos.y + verticalDistance, 
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

export function addNode(cyto_id, id, label, backgroundColor) {
    const cy = getCytoInstance(cyto_id);
    if (cy) {
        const wrappedLabel = wrapText(label); 
        const node = cy.add({ data: { id, label: wrappedLabel, backgroundColor } });

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
    const label = node.data("label");
    const lines = label.split("\n").length;

    // Adjust node size based on the number of lines
    node.style({
        "height": 20 + lines * 10, // Base height + height per line
        "width": 20 + lines * 10,  // Base width to keep circle aspect ratio
    });
}