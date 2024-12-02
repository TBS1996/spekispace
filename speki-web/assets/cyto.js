import cytoscape from "https://esm.sh/cytoscape@3.23.0";
import dagre from "https://esm.sh/cytoscape-dagre@2.4.0";
import { onNodeClick } from '/wasm/speki-web.js';
cytoscape.use(dagre);

const instances = new Map();
const charWidth = 20;
const lineLen = 20;


export function createCytoInstance(id) {
    if (instances.has(id)) {
        console.log("cyto instance already exist");
        const existingInstance = instances.get(id);
        console.log(`Cytoscape instance with ID "${id}" already exists. Clearing it.`);
        existingInstance.destroy(); 
    }

    console.log("creating cyto instance");
    const cy = cytoscape({
        container: document.getElementById(id),
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
                        "line-color": "#000",               // Line color
                        "width": 1,                         // Line thickness (reduce for thinner lines)
                        "target-arrow-color": "#ccc",       // Arrowhead color
                        "target-arrow-shape": "triangle",   // Arrowhead shape
                        "arrow-scale": 0.5,                 // Smaller arrow size (reduce for thinner arrows)
                        "target-distance-from-node": 3,    // Distance of the arrowhead from the node
                        "curve-style": "bezier",            // Style of the line
                    },
                },
        ],
        layout: {
            name: "dagre",
            rankDir: "BT",     
            nodeSep: 5,  // Minimum spacing between nodes on the same rank (default: 50)
            rankSep: 10,  // Minimum spacing between adjacent ranks (default: 50)
            directed: true,
            padding: 10,
        },
    });

    cy.on('tap', 'node', (event) => {
        const node = event.target; 
        const nodeId = node.id(); 
        console.log(`Node clicked: ${nodeId}`);
        onNodeClick(nodeId); 
    });

    console.log("setting instance");
    instances.set(id, cy);
    return cy;
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
    let first = maxCharsPerLine * charWidth;
    let sec = label.length * charWidth;

    console.log(label);
    console.log(`label len: ${label.length}`);
    console.log(`maxchars: ${maxCharsPerLine}`);
    console.log(`width: ${charWidth}`);
    console.log(`first: ${first}`);
    console.log(`sec: ${first}`);

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
    if (cy) {
        // Run the Dagre layout
        cy.layout({
            name: "dagre",
            rankDir: "TB",          // Top-to-bottom flow
            fit: true,              // Fit the graph in the viewport
            padding: 50,            // Padding around the graph
            nodeSep: 50,            // Adjust spacing for better visibility
            rankSep: 75,            // Adjust rank separation
            edgeWeight: (edge) => {
                const connectedToTarget = edge.source().id() === targetNodeId || edge.target().id() === targetNodeId;
                return connectedToTarget ? 3 : 1; // Moderate weight for target-connected edges
            },
        }).run();

        // Adjust node proximity with directionality
        adjustProximityToTargetWithDirection(cy, targetNodeId);
    } else {
        console.warn(`Cytoscape instance with ID "${id}" not found.`);
    }
}

function adjustProximityToTargetWithDirection(cy, targetNodeId) {
    const targetNode = cy.getElementById(targetNodeId);
    const targetPos = targetNode.position();
    const incomingNeighbors = targetNode.incomers("node");
    const outgoingNeighbors = targetNode.outgoers("node");

    const horizontalSpacing = 50; // Increased left-right spacing
    const verticalDistance = 60; // Reduced up-down distance


    incomingNeighbors.forEach((node, index) => {
        node.position({
            x: targetPos.x + index * horizontalSpacing - (incomingNeighbors.length * horizontalSpacing) / 2,
            y: targetPos.y - verticalDistance, 
        });
    });

    outgoingNeighbors.forEach((node, index) => {
        node.position({
            x: targetPos.x + index * horizontalSpacing - (outgoingNeighbors.length * horizontalSpacing) / 2,
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