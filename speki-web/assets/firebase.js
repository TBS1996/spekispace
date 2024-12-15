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

// Helper functions
function getTable(userId, tableName) {
  return collection(db, `users/${userId}/${tableName}`);
}

function getContentRef(userId, tableName, contentId) {
  return doc(getTable(userId, tableName), contentId);
}

// Core CRUD functions
async function getContent(userId, tableName, contentId) {
  const docRef = getContentRef(userId, tableName, contentId);
  const docSnap = await getDoc(docRef);
  return docSnap;
}

export async function loadContent(userId, tableName, contentId) {
  const docSnap = await getContent(userId, tableName, contentId);
  return docSnap.exists() ? docSnap.data().content : null;
}

export async function lastModified(userId, tableName, contentId) {
  const docSnap = await getContent(userId, tableName, contentId);
  return docSnap.exists() ? docSnap.data().lastModified.toMillis() : null;
}

export async function loadAllContent(userId, tableName) {
  console.log(`loading all content from table ${tableName}`)
  const colRef = getTable(userId, tableName);
  console.log(`fetched table..`)
  const querySnapshot = await getDocs(colRef);
  console.log(`done loading`)
  return querySnapshot.docs.map(doc => doc.data().content);
}

export async function loadAllIds(userId, tableName) {
  console.log(`loading all id from table ${tableName}`)
  const colRef = getTable(userId, tableName);
  console.log(`fetched table..`)
  const querySnapshot = await getDocs(colRef);
  console.log(`done loading`)
  return querySnapshot.docs.map(doc => doc.id);
}

export async function saveContent(userId, tableName, contentId, content) {
    const docRef = doc(db, `users/${userId}/${tableName}`, contentId);
    await setDoc(docRef, {
        id: contentId,
        content,
        lastModified: serverTimestamp()
    }, { merge: true });
}

export async function deleteContent(userId, tableName, contentId) {
  const docRef = getContentRef(userId, tableName, contentId);
  await deleteDoc(docRef);
}

export async function loadAll(userId, tableName) {
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
      lastModified: data.lastModified ? data.lastModified.toMillis() : null
    };
  });

  console.log(`Done loading all content with metadata`);
  return resultMap;
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
