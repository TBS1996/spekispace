import { 
  getFirestore, collection, doc, setDoc, getDocs, getDoc, deleteDoc, serverTimestamp 
} from 'https://www.gstatic.com/firebasejs/11.0.2/firebase-firestore.js';
import { initializeApp } from 'https://www.gstatic.com/firebasejs/11.0.2/firebase-app.js';
import { 
  getAuth, signInWithPopup, GoogleAuthProvider, signOut, onAuthStateChanged 
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
const auth = getAuth();
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


export async function loadAllRecords(userId, tableName) {
  console.log(`Loading all content with metadata from table ${tableName}`);
  const colRef = getTable(userId, tableName);

  console.log(`Fetching table...`);
  const querySnapshot = await getDocs(colRef);

  console.log(`Processing documents...`);
  const resultMap = {};
  querySnapshot.forEach(doc => {
    const data = doc.data();
    resultMap[doc.id] = {
      content: data.content,
      last_modified: data.lastModified ? Math.floor(data.lastModified.toMillis() / 1000) : null 
    };
  });

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

export async function saveContent(userId, tableName, contentId, content, lastModified) {
    const docRef = doc(db, `users/${userId}/${tableName}`, contentId);
    await setDoc(docRef, {
        id: contentId,
        content,
        lastModified
    }, { merge: true });
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

export function getCurrentUser() {
  return auth.currentUser;
}

export async function isUserAuthenticated() {
  const user = auth.currentUser;
  if (user) {
      try {
          await user.getIdToken(true);
          return true;
      } catch (error) {
          console.error('Error refreshing token:', error);
          return false;
      }
  } else {
      return false;
  }
}
