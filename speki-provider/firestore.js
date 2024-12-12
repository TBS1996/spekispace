import { getFirestore, collection, doc, setDoc, getDocs, getDoc, serverTimestamp } from 'https://www.gstatic.com/firebasejs/11.0.2/firebase-firestore.js';

const firebaseConfig = {
  apiKey: "AIzaSyCq-vG1DGqXRauJMquYQPccfR3nMSeX8Gc",
  authDomain: "firelog-3aa10.firebaseapp.com",
  projectId: "firelog-3aa10",
  storageBucket: "firelog-3aa10.appspot.com",
  messagingSenderId: "1030553617541",
  appId: "1:1030553617541:web:d0677286240dafa3c155ec",
  measurementId: "G-D5DF5VQ9CY"
};

const app = initializeApp(firebaseConfig);
const db = getFirestore(app);

function getTable(userId, tableName) {
    return collection(db, `users/${userId}/${tableName}`);
}

export async function loadContent(userId, tableName, id) {
    const table = getTable(userId, tableName); 
    const docRef = doc(table, id); 
    const docSnap = await getDoc(docRef);
    return docSnap.exists() ? docSnap.data().content : null;
}


export async function lastModified(tableName, id) {
    const docRef = doc(db, tableName, id);
    const docSnap = await getDoc(docRef);
    return docSnap.exists() ? docSnap.data().lastModified.toMillis() : null; 
}

export async function loadAllContent(tableName) {
    const colRef = getTable(tableName);
    const querySnapshot = await getDocs(colRef);
    return querySnapshot.docs.map(doc => doc.data().content);
}

export async function loadAllIds(tableName) {
    const colRef = getTable(tableName);
    const querySnapshot = await getDocs(colRef);
    return querySnapshot.docs.map(doc => doc.id);
}

export async function saveContent(tableName, id, content) {
    const docRef = doc(db, tableName, id);
    await setDoc(docRef, {
        id,
        content,
        lastModified: serverTimestamp() 
    }, { merge: true });
}

export async function deleteContent(tableName, id) {
    const docRef = doc(db, tableName, id);
    await deleteDoc(docRef);
}