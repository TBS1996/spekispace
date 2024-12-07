const db = new Dexie("dexiedb2");

db.version(1).stores({
    files: "id,content,lastModified",
    reviews: "id,content,lastModified" 
});


export async function saveReviews(id, content) {
    await db.reviews.put({
        id, 
        content, 
        lastModified: Date.now()
    });
    console.log("reviews saved:", id);
}

export async function loadReviews(id) {
    const file = await db.reviews.get(id);
    return file?.content ?? null; 
}

export async function saveFile(id, content) {
    await db.files.put({
        id, 
        content, 
        lastModified: Date.now()
    });
    console.log("File saved:", id);
}

export async function loadFile(id) {
    const file = await db.files.get(id);
    return file?.content ?? null; 
}

export async function deleteFile(id) {
    await db.files.delete(id);
    console.log("File deleted:", id);
}

export async function loadAllFiles() {
    return db.files.toArray(); 
}

export async function lastModified(id) {
    const file = await db.files.get(id);
    return file?.lastModified ?? null; 
}

export async function loadIds() {
    const files = await db.files.toArray();
    return files.map(file => file.id); 
}