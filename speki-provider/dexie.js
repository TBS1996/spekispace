const db = new Dexie("dexiedb4");

db.version(5).stores({
    cards: "id,content,lastModified",
    reviews: "id,content,lastModified",
    attrs: "id,content,lastModified" ,
    db_id: "key, id",
    sync_data: "key, lastSync",

});

function getTable(tableName) {
    return db[tableName];
}


export async function saveDbId(id) {
    console.log("Saving DB ID:", id);
    await db.db_id.put({ key: "db_id", id });
}

export async function loadDbId() {
    const dbId = await db.db_id.get("db_id");
    if (!dbId) {
        console.log("No DB ID found, returning empty string.");
        return "";
    }
    console.log("Loaded DB ID:", dbId.id);
    return dbId.id; 
}

export async function saveSyncTime(key, lastSync) {
    console.log(`Saving sync time for key '${key}':`, lastSync);
    await db.sync_data.put({ key, lastSync });
}

export async function loadSyncTime(key) {
    const syncData = await db.sync_data.get(key);
    if (!syncData) {
        console.log(`No sync time found for key '${key}', returning 0.`);
        return 0; 
    }
    console.log(`Loaded sync time for key '${key}':`, syncData.lastSync);
    return syncData.lastSync;
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
        id,
        content: record.content,
        last_modified: ensureUnixSeconds(record.lastModified) ?? null,
        inserted: null
    };
}

export async function loadAllRecords(tableName) {
    const table = getTable(tableName);
    const records = await table.toArray();

    return records.reduce((map, record) => {
        map[record.id] = {
            id: record.id,
            content: record.content,
            last_modified: ensureUnixSeconds(record.lastModified),
            inserted: null,
        };
        return map;
    }, {});

}

export async function loadAllIds(tableName) {
    const table = getTable(tableName);
    const records = await table.toArray();
    return records.map(record => record.id); 
}

export async function saveContent(tableName, id, content, lastModified) {
    console.log(`dexie saving content to: ${tableName}`);
    const table = getTable(tableName);
    await table.put({
        id,
        content,
        lastModified
    });
}

export async function deleteContent(tableName, id) {
    const table = getTable(tableName);
    await table.delete(id); 
}