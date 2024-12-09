const db = new Dexie("dexiedb4");

db.version(1).stores({
    cards: "id,content,lastModified",
    reviews: "id,content,lastModified",
    attrs: "id,content,lastModified" 
});

function getTable(tableName) {
    return db[tableName];
}

export async function loadContent(tableName, id) {
    const table = getTable(tableName);
    const record = await table.get(id);
    return record?.content ?? null; 
}

export async function lastModified(tableName, id) {
    const table = getTable(tableName);
    const record = await table.get(id);
    return record?.lastModified ?? null; 
}

export async function loadAllContent(tableName) {
    const table = getTable(tableName);
    const records = await table.toArray();
    return records.map(record => record.content); 
}

export async function loadAllIds(tableName) {
    const table = getTable(tableName);
    const records = await table.toArray();
    return records.map(record => record.id); 
}

export async function saveContent(tableName, id, content) {
    const table = getTable(tableName);
    await table.put({
        id,
        content,
        lastModified: Date.now() 
    });
}

export async function deleteContent(tableName, id) {
    const table = getTable(tableName);
    await table.delete(id); 
}