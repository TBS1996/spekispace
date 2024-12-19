import{getFirestore as e,collection as t,doc as o,setDoc as n,getDocs as r,getDoc as a,deleteDoc as i,serverTimestamp as s}from"https://www.gstatic.com/firebasejs/11.0.2/firebase-firestore.js";import{initializeApp as l}from"https://www.gstatic.com/firebasejs/11.0.2/firebase-app.js";import{getAuth as c,signInWithPopup as d,GoogleAuthProvider as f,signOut as u}from"https://www.gstatic.com/firebasejs/11.0.2/firebase-auth.js";let g=e(l({apiKey:"AIzaSyBm4O49Wo2SKA3tamdS0M_fhOfOflSYngU",authDomain:"speki-72569.firebaseapp.com",projectId:"speki-72569",storageBucket:"speki-72569.firebasestorage.app",messagingSenderId:"557666655647",appId:"1:557666655647:web:71b1cc379d1068f3f871aa",measurementId:"G-BSJFY9KBFS"})),p=c(),m=new f;function w(e,o){return t(g,`users/${e}/${o}`)}async function h(e,t,n){let r=o(w(e,t),n);return await a(r)}export async function loadRecord(e,t,o){let n=await h(e,t,o);if(!n.exists())return null;let r=n.data();return{content:r.content,last_modified:r.lastModified?Math.floor(r.lastModified.toMillis()/1e3):null}}export async function loadAllRecords(e,t){console.log(`Loading all content with metadata from table ${t}`);let o=w(e,t);console.log("Fetching table...");let n=await r(o);console.log("Processing documents...");let a={};return n.forEach(e=>{let t=e.data();a[e.id]={content:t.content,last_modified:t.lastModified?Math.floor(t.lastModified.toMillis()/1e3):null}}),console.log("Done loading all content with metadata"),a}export async function loadAllIds(e,t){console.log(`loading all id from table ${t}`);let o=w(e,t);console.log("fetched table..");let n=await r(o);return console.log("done loading"),n.docs.map(e=>e.id)}export async function saveContent(e,t,r,a){let i=o(g,`users/${e}/${t}`,r);await n(i,{id:r,content:a,lastModified:s()},{merge:!0})}export async function deleteContent(e,t,n){let r=o(w(e,t),n);await i(r)}export async function signInWithGoogle(){try{let e=await d(p,m);return console.log("User signed in:",e.user),e.user}catch(e){throw console.error("Error signing in with Google:",e),e}}export async function signOutUser(){try{await u(p),console.log("User signed out")}catch(e){throw console.error("Error signing out:",e),e}}export function getCurrentUser(){return p.currentUser}export async function isUserAuthenticated(){let e=p.currentUser;if(!e)return!1;try{return await e.getIdToken(!0),!0}catch(e){return console.error("Error refreshing token:",e),!1}}