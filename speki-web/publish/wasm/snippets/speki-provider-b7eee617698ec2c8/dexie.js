const db = new Dexie("dexiedb4");

db.version(1).stores({
    cards: "id,content,lastModified",
    reviews: "id,content,lastModified",
    attrs: "id,content,lastModified" 
});

function getTable(tableName) {
    return db[tableName];
}

function unixSecs() {
    return Math.floor(Date.now() / 1000);
}

const TooBig = 173426346900;

function ensureUnixSeconds(timestamp) {
    if (timestamp == null) {
        return null;
    }
    if (timestamp > TooBig) {
        return Math.floor(timestamp / 1000);
    } else {
        return Math.floor(timestamp);
    }
}

export async function loadRecord(tableName, id) {
    const table = getTable(tableName);
    const record = await table.get(id);

    if (!record) {
        return null; 
    }

    return {
        content: record.content,
        last_modified: ensureUnixSeconds(record.lastModified) ?? null 
    };
}

export async function loadAllRecords(tableName) {
    const table = getTable(tableName);
    const records = await table.toArray();

    return records.reduce((map, record) => {
        map[record.id] = {
            content: record.content,
            last_modified: ensureUnixSeconds(record.lastModified)
        };
        return map;
    }, {});

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
        lastModified: unixSecs()
    });
}

export async function deleteContent(tableName, id) {
    const table = getTable(tableName);
    await table.delete(id); 
}