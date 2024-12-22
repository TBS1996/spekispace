
export function maxCircumference(node) {
    let label = node.data("label");
    let lines = label.split('\n'); 
    let textHeight = calculateTextHeight(node);
    let totalLines = lines.length; 
    let tot_height = textHeight * totalLines;
    let maxCircum = 0;

    lines.forEach((line, currentLine) => {
        let hypo = Hypo(line, textHeight, totalLines, currentLine, node);
        maxCircum = Math.max(maxCircum, hypo);
    });

    return Math.max(maxCircum, tot_height);
}

function Hypo(line, textHeight, totalLines, currentLine, node) {
    let width = calculateTextWidth(line, node);
    let height = topLineHeight(textHeight, totalLines, currentLine);
    let hypo = Math.sqrt(width ** 2 + height ** 2);
    return hypo;
}

function topLineHeight(textHeight, totalLines, currentLine) {
    let center = (totalLines - 1) / 2;
    let distance = Math.abs(currentLine - center);
    let dist = (distance * textHeight) + (textHeight / 2);
    return dist;
}

function calculateTextHeight(node) {
    const font = `${node.style('font-weight')} ${node.style('font-size')} ${node.style('font-family')}`;
    const canvas = document.createElement('canvas');
    const context = canvas.getContext('2d');
    context.font = font;
    const metrics = context.measureText("M");
    return metrics.actualBoundingBoxAscent + metrics.actualBoundingBoxDescent + 7;
}

function calculateTextWidth(text, node) {
    const font = `${node.style('font-weight')} ${node.style('font-size')} ${node.style('font-family')}`;
    const canvas = document.createElement('canvas');
    const context = canvas.getContext('2d');
    context.font = font;
    return context.measureText(text).width;
}