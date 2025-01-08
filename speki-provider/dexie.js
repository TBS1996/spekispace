
const dexieInstances = {}; 

const metadataDb = new Dexie("metadata_db");
metadataDb.version(1).stores({
    db_id: "key, id",
    sync_data: "key, lastSync",
});

function createDexieInstance(typeName) {
    const db = new Dexie(`dexie_${typeName}`);
    db.version(1).stores({
        records: "id, content, lastModified",
    });
    return db;
}

function getDexieInstance(typeName) {
    if (!dexieInstances[typeName]) {
        dexieInstances[typeName] = createDexieInstance(typeName);
    }
    return dexieInstances[typeName];
}

function ensureUnixSeconds(timestamp) {
    const TooBig = 173426346900;
    if (timestamp == null) return null;
    return timestamp > TooBig ? Math.floor(timestamp / 1000) : Math.floor(timestamp);
}

export async function saveDbId(id) {
    console.log("Saving DB ID:", id);
    await metadataDb.db_id.put({ key: "db_id", id });
}

export async function loadDbId() {
    const dbId = await metadataDb.db_id.get("db_id");
    if (!dbId) {
        console.log("No DB ID found, returning empty string.");
        return "";
    }
    console.log("Loaded DB ID:", dbId.id);
    return dbId.id;
}

export async function saveSyncTime(key, lastSync) {
    console.log(`Saving sync time for key '${key}':`, lastSync);
    await metadataDb.sync_data.put({ key, lastSync });
}

export async function loadSyncTime(key) {
    const syncData = await metadataDb.sync_data.get(key);
    if (!syncData) {
        console.log(`No sync time found for key '${key}', returning 0.`);
        return 0;
    }
    console.log(`Loaded sync time for key '${key}':`, syncData.lastSync);
    return syncData.lastSync;
}

export async function saveContent(typeName, id, content, lastModified) {
    console.log(`Saving content to type: ${typeName}`);
    const db = getDexieInstance(typeName);
    await db.records.put({ id, content, lastModified });
}

export async function loadRecord(typeName, id) {
    const db = getDexieInstance(typeName);
    const record = await db.records.get(id);

    if (!record) return null;

    return {
        id,
        content: record.content,
        last_modified: ensureUnixSeconds(record.lastModified) ?? null,
        inserted: null,
    };
}

export async function loadAllRecords(typeName) {
    const db = getDexieInstance(typeName);
    const records = await db.records.toArray();

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

export async function loadAllIds(typeName) {
    const db = getDexieInstance(typeName);
    const records = await db.records.toArray();
    return records.map((record) => record.id);
}

export async function deleteContent(typeName, id) {
    const db = getDexieInstance(typeName);
    await db.records.delete(id);
}
