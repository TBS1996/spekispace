
const dexieInstances = {}; 

const metadataDb = new Dexie("metadata_db");
metadataDb.version(1).stores({
    db_id: "key, id",
    sync_data: "key, lastSync",
});

function createDexieInstance(typeName) {
    const db = new Dexie(`dexie_${typeName}`);
    db.version(1).stores({
        records: "id, content",
    });
    return db;
}

function getDexieInstance(typeName) {
    if (!dexieInstances[typeName]) {
        dexieInstances[typeName] = createDexieInstance(typeName);
    }
    return dexieInstances[typeName];
}

export async function clearSpace(typeName) {
    const db = getDexieInstance(typeName);
    await db.records.clear();  
    const remainingRecords = await db.records.toArray();
    if (!(remainingRecords.length === 0)) {
        console.error(`❌ Error: dexie_${typeName} is NOT empty after clearing!`, remainingRecords);
    } else {
        console.log(`Cleared all data from dexie_${typeName}`);
    }
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

export async function saveContent(typeName, id, content) {
    console.log(`Saving content to type: ${typeName}`);
    const db = getDexieInstance(typeName);
    await db.records.put({ id, content });
}

export async function loadRecord(typeName, id) {
    const db = getDexieInstance(typeName);
    const record = await db.records.get(id);

    if (!record) return null;
    return record.content;
}

export async function loadAllRecords(typeName) {
    const db = getDexieInstance(typeName);
    const records = await db.records.toArray();
    console.log(`heyy found ${records.length} ids of type ${typeName}`);

    return records.reduce((map, record) => {
        map[record.id] = record.content;
        return map;
    }, {});
}

export async function loadAllIds(typeName) {
    const db = getDexieInstance(typeName);
    const records = await db.records.toArray();
    console.log(`Found ${records.length} ids of type ${typeName}`);
    return records.map((record) => record.id);
}

export async function deleteContent(typeName, id) {
    const db = getDexieInstance(typeName);
    await db.records.delete(id);
}
