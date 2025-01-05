import { 
  getFirestore, collection, doc, setDoc, getDocs, getDoc, deleteDoc, writeBatch, updateDoc
} from 'https://www.gstatic.com/firebasejs/11.0.2/firebase-firestore-lite.js';
import { initializeApp } from 'https://www.gstatic.com/firebasejs/11.0.2/firebase-app.js';
import { 
  getAuth, signInWithPopup, GoogleAuthProvider, signOut 
} from 'https://www.gstatic.com/firebasejs/11.0.2/firebase-auth.js';


const firebaseConfig = {
  apiKey: "AIzaSyBm4O49Wo2SKA3tamdS0M_fhOfOflSYngU",
  authDomain: "speki-72569.firebaseapp.com",
  projectId: "speki-72569",
  storageBucket: "speki-72569.firebasestorage.app",
  messagingSenderId: "557666655647",
  appId: "1:557666655647:web:71b1cc379d1068f3f871aa",
  measurementId: "G-BSJFY9KBFS"
};


// Initialize Firebase
const app = initializeApp(firebaseConfig);
const db = getFirestore(app);
const auth = getAuth(app);
const provider = new GoogleAuthProvider();

function getTable(userId, tableName) {
  return collection(db, `users/${userId}/${tableName}`);
}

function getContentRef(userId, tableName, contentId) {
  return doc(getTable(userId, tableName), contentId);
}

async function getContent(userId, tableName, contentId) {
  const docRef = getContentRef(userId, tableName, contentId);
  const docSnap = await getDoc(docRef);
  return docSnap;
}

export async function loadRecord(userId, tableName, contentId) {
  const docSnap = await getContent(userId, tableName, contentId);

  if (!docSnap.exists()) {
    return null; 
  }

  const data = docSnap.data();
  return {
    id: docSnap.id,
    content: data.content,
    last_modified: data.lastModified ? Math.floor(data.lastModified.toMillis() / 1000) : null 
  };
}


function getDbIdRef(userId) {
  return doc(db, `users/${userId}/meta/db_id`);
}

export async function loadDbId(userId) {
  const dbIdRef = getDbIdRef(userId);
  const dbIdSnap = await getDoc(dbIdRef);

  if (!dbIdSnap.exists()) {
      console.log(`No DB ID found for user: ${userId}`);
      return null; 
  }

  console.log(`Loaded DB ID for user: ${userId}`, dbIdSnap.data().id);
  return dbIdSnap.data().id; 
}

export async function saveDbId(userId, id) {
  const dbIdRef = getDbIdRef(userId);
  await setDoc(dbIdRef, { id }); // Set the `id` field
  console.log(`Saved DB ID for user: ${userId}`, id);
}


function getSyncTimeRef(userId, key) {
  return doc(db, `users/${userId}/meta/sync_data/sync/${key}`);
}

export async function saveSyncTime(userId, key, lastSync) {
  const syncTimeRef = getSyncTimeRef(userId, key);
  await setDoc(syncTimeRef, { lastSync });
  console.log(`Saved sync time for key '${key}' for user: ${userId}`, lastSync);
}

export async function loadSyncTime(userId, key) {
  const syncTimeRef = getSyncTimeRef(userId, key);
  const syncTimeSnap = await getDoc(syncTimeRef);

  if (!syncTimeSnap.exists()) {
      console.log(`No sync time found for key '${key}' for user: ${userId}`);
      return 0; 
  }

  let last_sync = syncTimeSnap.data().lastSync;

  console.log(`Loaded sync time for key '${key}' for user: ${userId}`, last_sync);

  return last_sync;
}

function extractSeconds(inserted) {
  if (typeof inserted === "number") {
    return Math.floor(inserted); 
  } else if (typeof inserted === "object" && inserted !== null && typeof inserted.seconds === "number") {
    return Math.floor(inserted.seconds); 
  } else {
    return null; 
  }
}

export async function loadAllRecords(userId, tableName, notBefore_) {
  let notBefore = Math.floor(notBefore_);
  console.log(typeof notBefore);
  console.log(`Loading all content with metadata from table ${tableName} after ${notBefore}`);
  const colRef = getTable(userId, tableName);

  console.log(`Fetching table...`);
  const querySnapshot = await getDocs(colRef);

  const resultMap = {};
  const updates = []; 

  querySnapshot.forEach(doc => {
    const data = doc.data();
    const inserted = extractSeconds(data.inserted) || 0; 

    if (inserted > (notBefore + 1)) {
      resultMap[doc.id] = {
        id: doc.id,
        content: data.content,
        last_modified: data.lastModified.seconds,
        inserted: extractSeconds(data.inserted)
      };
    } 

    if (inserted == 0) {
      console.log(`Adding serverTimestamp to document ${doc.id}`);
      updates.push(
        updateDoc(doc.ref, { inserted })
      );
    }
  });

  await Promise.all(updates);

  console.log(`Done loading all content with metadata`);
  return resultMap;
}

export async function loadAllIds(userId, tableName) {
  console.log(`loading all id from table ${tableName}`)
  const colRef = getTable(userId, tableName);
  console.log(`fetched table..`)
  const querySnapshot = await getDocs(colRef);
  console.log(`done loading`)
  return querySnapshot.docs.map(doc => doc.id);
}

export async function saveContents(userId, tableName, contents) {
    console.log(`staring batch save conents`);
    const batch = writeBatch(db);

    contents.forEach(({ id, content, lastModified, inserted}) => {
        let intserted = Math.floor(inserted);
        const docRef = doc(db, `users/${userId}/${tableName}`, id);
        batch.set(docRef, {
            id,
            content,
            lastModified,
            inserted: intserted
        }, { merge: true });
    });

    await batch.commit();
}

export async function deleteContent(userId, tableName, contentId) {
  const docRef = getContentRef(userId, tableName, contentId);
  await deleteDoc(docRef);
}


export async function signInWithGoogle() {
  try {
      const result = await signInWithPopup(auth, provider);
      console.log('User signed in:', result.user);
      return result.user;
  } catch (error) {
      console.error('Error signing in with Google:', error);
      throw error;
  }
}

export async function signOutUser() {
  try {
      await signOut(auth);
      console.log('User signed out');
  } catch (error) {
      console.error('Error signing out:', error);
      throw error;
  }
}

export async function getCurrentUser() {
  if (typeof auth === 'undefined' || auth === null) {
    console.log("Auth is not initialized or unavailable");
    return null;
  }

  const currentUser = auth.currentUser;

  if (!currentUser) {
    console.log("No user authenticated");
    return null;
  }

  if (await isUserAuthenticated()) {
    return currentUser;
  } else {
    return null;
  }
}

export async function isUserAuthenticated() {
  const user = auth.currentUser;
  if (user) {
      try {
          await user.getIdToken(true);
          console.log(`user authenticated!`);
          return true;
      } catch (error) {
          console.error('Error refreshing token:', error);
          return false;
      }
  } else {
      console.log(`user yok`);
      return false;
  }
}
