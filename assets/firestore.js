import { initializeApp } from 'https://www.gstatic.com/firebasejs/9.6.1/firebase-app.js';
import { getFirestore, collection, doc, setDoc, getDocs, serverTimestamp } from 'https://www.gstatic.com/firebasejs/9.6.1/firebase-firestore.js';

console.log("Initializing Firebase...");

// Initialize Firebase
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
console.log("Firebase initialized:", db);

window.firestoreInitialized = true;

export async function addFirestoreTaskLog(taskId, logId) {
    const taskRef = doc(collection(db, 'task_logs'), taskId);
    const logRef = doc(collection(taskRef, 'logs'), logId);
    await setDoc(logRef, {});
}

export async function loadAllLogs() {
    const querySnapshot = await getDocs(collection(db, 'task_logs'));
    let logs = [];
    let promises = [];

    querySnapshot.forEach(doc => {
        let taskId = doc.id;
        let subCollectionRef = collection(db, 'task_logs', taskId, 'logs');
        promises.push(getDocs(subCollectionRef).then(subQuerySnapshot => {
            subQuerySnapshot.forEach(subDoc => {
                logs.push({
                    task_id: taskId,
                    timestamp: subDoc.id
                });
            });
        }));
    });

    await Promise.all(promises);
    return logs;
}

export async function loadLogsForTask(taskId) {
    const taskRef = doc(collection(db, 'task_logs'), taskId);
    const subCollectionRef = collection(taskRef, 'logs');
    const querySnapshot = await getDocs(subCollectionRef);
    let logs = [];

    querySnapshot.forEach(subDoc => {
        logs.push({
            task_id: taskId,
            timestamp: subDoc.id
        });
    });

    return logs;
}

// Add or update a task in Firestore by ID
export function upsertFirestoreTask(id, task) {
  return new Promise((resolve, reject) => {
    if (typeof db === 'undefined') {
      console.error('Firestore has not been initialized');
      reject(new Error('Firestore has not been initialized'));
      return;
    }
    console.log(`Firestore is initialized. Adding/Updating task with ID ${id}:`, task);
    const taskRef = doc(db, 'tasks', id);
    setDoc(taskRef, {
      ...task,
      updated_at: serverTimestamp()
    }, { merge: true })
      .then(() => {
        console.log(`Task with ID ${id} added/updated in Firestore successfully`);
        resolve();
      })
      .catch((error) => {
        console.error(`Error adding/updating task with ID ${id} in Firestore:`, error);
        reject(error);
      });
  });
}

// Load all tasks from Firestore
export function loadAllTasks() {
  return new Promise((resolve, reject) => {
    if (typeof db === 'undefined') {
      console.error('Firestore has not been initialized');
      reject(new Error('Firestore has not been initialized'));
      return;
    }
    console.log('Firestore is initialized. Loading all tasks...');
    getDocs(collection(db, 'tasks'))
      .then((querySnapshot) => {
        const tasks = [];
        querySnapshot.forEach((doc) => {
          tasks.push({ id: doc.id, ...doc.data() });
        });
        console.log('All tasks loaded from Firestore successfully');
        resolve(tasks);
      })
      .catch((error) => {
        console.error('Error loading tasks from Firestore:', error);
        reject(error);
      });
  });
}

window.upsertFirestoreTask = upsertFirestoreTask;
window.loadAllTasks = loadAllTasks;

export function isFirestoreInitialized() {
  return typeof db !== 'undefined';
}

window.addEventListener('load', (event) => {
});

