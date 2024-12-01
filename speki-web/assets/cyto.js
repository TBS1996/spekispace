import cytoscape from "https://esm.sh/cytoscape@3.23.0";
import dagre from "https://esm.sh/cytoscape-dagre@2.4.0";
import { onNodeClick } from '/wasm/speki-web.js';
cytoscape.use(dagre);

const instances = new Map();
const charWidth = 15;
const lineLen = 10;


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
        style: [
            {
                selector: "node",
                style: {
                    "shape": "rectangle",        
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
                    "line-color": "#f59842",            
                    "target-arrow-color": "#ccc",       
                    "target-arrow-shape": "triangle",   
                    "arrow-scale": 1.2,                 
                    "target-distance-from-node": 10,    
                    "curve-style": "bezier",            
                },
            },
        ],
        layout: {
            name: "dagre",
            rankDir: "TB",     
            directed: true,
            padding: 50,
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

export function runLayout(id) {
    const cy = getCytoInstance(id);
    if (cy) {
        cy.layout({
            name: "dagre", 
            fit: true,            
            padding: 50,
            animate: false,
        }).run();
    } else {
        console.warn(`Cytoscape instance with ID "${id}" not found.`);
    }
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